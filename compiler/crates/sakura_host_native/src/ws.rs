//! `WebSocket.host()` on the native host (RFC 0002 §1, §2.10, §6; design §17.289): the JS
//! runtime's RFC 6455 sockets ported with their limits, close codes and messages. A client
//! opens with `reqwest` (the `Http` client's TLS, roots and `net` check) and upgrades; a server
//! socket is a served request's connection that `hyper` hands over after the 101 `ws_accept`
//! answered. Frames are parsed as they arrive: a data frame that would take its message past
//! `max_message` fails with 1009 before its payload is held, and while 64 messages, or more
//! than `max_message` bytes of them, wait for `receive`, the socket is not read, so TCP holds
//! the peer back. A ping is answered only while the write buffer is under its high-water mark,
//! the close follows the messages and is returned again by every later receive, and a close
//! this side starts waits 5 s for the peer's. A socket is an id in the files' id space, closed
//! by `file_close` (1001 when still open).
//!
//! Every socket's state is in one [`Shared`] table, locked by the instance's thread and by the
//! I/O tasks on its current-thread executor, which run only while that thread waits: one lock,
//! never contended, as the JS runtime's one event loop.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use sakura_rt::exec::status;
use sakura_rt::native::WsReceived;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Notify, mpsc};
use tokio::task::AbortHandle;

use crate::http::{self, failure};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Received messages a socket holds before it stops reading.
const QUEUE: usize = 64;
/// How long a close this side started, or a socket's end, waits for the peer.
const CLOSE: Duration = Duration::from_millis(5000);
/// Headers the opening handshake sets itself (RFC 6455 §4.1); `headers` may not.
const OWN: [&str; 5] = [
    "host",
    "upgrade",
    "connection",
    "content-length",
    "transfer-encoding",
];
pub(crate) const DEFAULT_MAX_SOCKETS: u64 = 1024;
/// The write buffer past which a write reports it is full (Node's socket `highWaterMark`).
// ponytail: Node's default, not its per-socket figure; only backpressure timing reads it.
const HIGH_WATER: usize = 64 << 10;

pub(crate) type Completion = (u32, u32, Vec<u8>);

/// A response's header names and Latin-1 values.
pub(crate) type Head = Vec<(String, Vec<u8>)>;

/// What the writer task takes: a frame, or the end of this side.
enum Out {
    Frame(Vec<u8>),
    End,
}

struct Sock {
    server: bool,
    max: u64,
    /// Bytes read and not parsed yet, from `start`.
    buf: Vec<u8>,
    start: usize,
    /// The message being assembled: its fragments' payloads and its opcode (0: none).
    frags: Vec<u8>,
    op: u8,
    queue: VecDeque<(WsReceived, usize)>,
    queued: usize,
    /// The receive request waiting for the next value.
    waiter: Option<u32>,
    closed: Option<(i64, String, bool)>,
    closing: bool,
    paused: bool,
    resume: Arc<Notify>,
    /// `end()` sent, the connection gone (or its reading side ended), the table entry closed.
    ended: bool,
    destroyed: bool,
    freed: bool,
    topics: Vec<String>,
    out: mpsc::UnboundedSender<Out>,
    /// The writer's end, until the connection arrives.
    rx: Option<mpsc::UnboundedReceiver<Out>>,
    /// Bytes handed to the writer and not written yet, and the sends waiting for them to drain.
    pending: usize,
    drains: Vec<u32>,
    timer: Option<AbortHandle>,
    io: Vec<AbortHandle>,
}

impl Sock {
    fn new(server: bool, max: i64) -> Sock {
        let (out, rx) = mpsc::unbounded_channel();
        Sock {
            server,
            max: max.max(0) as u64,
            buf: vec![],
            start: 0,
            frags: vec![],
            op: 0,
            queue: VecDeque::new(),
            queued: 0,
            waiter: None,
            closed: None,
            closing: false,
            paused: false,
            resume: Arc::new(Notify::new()),
            ended: false,
            destroyed: false,
            freed: false,
            topics: vec![],
            out,
            rx: Some(rx),
            pending: 0,
            drains: vec![],
            timer: None,
            io: vec![],
        }
    }

    /// Whether the received messages fill the queue.
    fn full(&self) -> bool {
        self.queue.len() >= QUEUE || self.queued as u64 > self.max
    }

    /// Neither closing nor closed, nor closed by `file_close`.
    fn open(&self) -> bool {
        !self.freed && !self.closing && self.closed.is_none()
    }

    /// A frame this socket sends: masked in the client role only.
    fn frame(&self, op: u8, payload: &[u8]) -> Vec<u8> {
        frame(op, payload, !self.server)
    }

    fn close_frame(&self, code: i64, reason: &str) -> Vec<u8> {
        let mut p = (code as u16).to_be_bytes().to_vec();
        p.extend_from_slice(reason.as_bytes());
        self.frame(8, &p)
    }

    fn set_timer(&mut self, t: AbortHandle) {
        if let Some(old) = self.timer.replace(t) {
            old.abort();
        }
    }

    fn clear_timer(&mut self) {
        if let Some(t) = self.timer.take() {
            t.abort();
        }
    }
}

/// One final frame of `op` and `payload` (RFC 6455 §5.2): masked with a fresh mask as a
/// client's (§5.3), or unmasked as a server's (§5.1).
fn frame(op: u8, payload: &[u8], masked: bool) -> Vec<u8> {
    let n = payload.len();
    let mut f = Vec::with_capacity(n + 14);
    f.push(0x80 | op);
    let m = if masked { 0x80 } else { 0 };
    if n < 126 {
        f.push(m | n as u8);
    } else if n < 65536 {
        f.push(m | 126);
        f.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        f.push(m | 127);
        f.extend_from_slice(&(n as u64).to_be_bytes());
    }
    if !masked {
        f.extend_from_slice(payload);
        return f;
    }
    let mut mask = [0; 4];
    getrandom::fill(&mut mask).expect("the OS random source is readable");
    f.extend_from_slice(&mask);
    f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i & 3]));
    f
}

/// Every socket of one instance, its open count and its topics.
pub(crate) struct Ws {
    socks: HashMap<i64, Sock>,
    /// Open sockets of both roles, and connects in flight (`connecting`).
    count: u64,
    connecting: Vec<i64>,
    /// Topic → the ids subscribed, in the order they joined.
    topics: HashMap<String, Vec<i64>>,
    tx: mpsc::UnboundedSender<Completion>,
    me: Weak<Mutex<Ws>>,
}

pub(crate) type Shared = Arc<Mutex<Ws>>;

pub(crate) fn shared(tx: mpsc::UnboundedSender<Completion>) -> Shared {
    Arc::new_cyclic(|me| {
        Mutex::new(Ws {
            socks: HashMap::new(),
            count: 0,
            connecting: vec![],
            topics: HashMap::new(),
            tx,
            me: me.clone(),
        })
    })
}

pub(crate) fn lock(s: &Shared) -> MutexGuard<'_, Ws> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

/// `SAKURA_MAX_SOCKETS`, read at each use as JS `bytesEnv` reads it; an invalid value is the
/// `D091` message.
pub(crate) fn max_sockets() -> Result<u64, String> {
    let Ok(raw) = std::env::var("SAKURA_MAX_SOCKETS") else {
        return Ok(DEFAULT_MAX_SOCKETS);
    };
    let n = if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        raw.parse::<u64>().unwrap_or(0)
    } else {
        0
    };
    if (1..=(1 << 53) - 1).contains(&n) {
        return Ok(n);
    }
    Err(format!(
        "SAKURA_MAX_SOCKETS must be a positive integer number of sockets (1 to 2^53 - 1), got {}",
        serde_json::Value::from(raw)
    ))
}

/// RFC 9110 `token`.
pub(crate) fn token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
}

/// Why `name` may not be an extra handshake header.
fn own(name: &str) -> Option<String> {
    let n = name.to_ascii_lowercase();
    (OWN.contains(&n.as_str()) || n.starts_with("sec-websocket-"))
        .then(|| format!("header {name} is set by the handshake"))
}

/// `sec-websocket-accept` for `key`.
pub(crate) fn accept_of(key: &str) -> String {
    let d = ring::digest::digest(
        &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
        format!("{key}{GUID}").as_bytes(),
    );
    http::base64(d.as_ref())
}

fn json(s: &str) -> String {
    serde_json::Value::from(s).to_string()
}

impl Ws {
    fn complete(&self, request: u32, st: u32, payload: Vec<u8>) {
        let _ = self.tx.send((request, st, payload));
    }

    /// Takes a socket slot, or why not.
    fn reserve(&mut self) -> Result<(), crate::HostError> {
        let max = max_sockets().map_err(crate::HostError::Failure)?;
        if self.count >= max {
            return Err(crate::HostError::Err(format!(
                "the socket limit of {max} is reached"
            )));
        }
        self.count += 1;
        Ok(())
    }

    /// Whether `id` names a socket `file_close` has not closed.
    pub(crate) fn has(&self, id: i64) -> bool {
        self.socks.get(&id).is_some_and(|s| !s.freed)
    }

    /// Writes `frame` (as `socket.write`): whether the buffer is still under its mark.
    fn write(&mut self, id: i64, frame: Vec<u8>) -> bool {
        let Some(s) = self.socks.get_mut(&id) else {
            return false;
        };
        if s.ended || s.destroyed {
            return false;
        }
        s.pending += frame.len();
        let _ = s.out.send(Out::Frame(frame));
        s.pending < HIGH_WATER
    }

    /// The writer wrote `n` bytes: the sends waiting for the drain complete once it is empty.
    fn written(&mut self, id: i64, n: usize) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        s.pending = s.pending.saturating_sub(n);
        if s.pending == 0 {
            let drains = std::mem::take(&mut s.drains);
            for r in drains {
                self.complete(r, status::OK, vec![1]);
            }
        }
    }

    /// `socket.end()`, once.
    fn shut(&mut self, id: i64) {
        if let Some(s) = self.socks.get_mut(&id)
            && !s.ended
        {
            s.ended = true;
            let _ = s.out.send(Out::End);
        }
    }

    /// `socket.destroy()`: the connection dropped, the waiting sends released, then the
    /// socket's `close` (1006 unless the close is known).
    fn destroy(&mut self, id: i64) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        s.destroyed = true;
        s.ended = true;
        for h in s.io.drain(..) {
            h.abort();
        }
        let drains = std::mem::take(&mut s.drains);
        for r in drains {
            self.complete(r, status::OK, vec![1]);
        }
        self.end(id, 1006, "", false);
    }

    /// The reading side ended (the peer's FIN or an error): this side ends too, then the
    /// socket's `close`.
    fn eof(&mut self, id: i64) {
        self.shut(id);
        if let Some(s) = self.socks.get_mut(&id) {
            s.destroyed = true;
        }
        self.end(id, 1006, "", false);
        if self.socks.get(&id).is_some_and(|s| s.freed) {
            self.remove(id);
        }
    }

    fn remove(&mut self, id: i64) {
        if let Some(mut s) = self.socks.remove(&id) {
            s.clear_timer();
        }
    }

    /// After `end()`: the peer's FIN is waited for at most 5 s, then the connection dropped, so
    /// a peer that never closes TCP holds nothing.
    fn linger(&mut self, id: i64) {
        let me = self.me.clone();
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if s.destroyed {
            return;
        }
        let t = tokio::spawn(async move {
            tokio::time::sleep(CLOSE).await;
            if let Some(shared) = me.upgrade() {
                let mut ws = lock(&shared);
                ws.destroy(id);
                if ws.socks.get(&id).is_some_and(|s| s.freed) {
                    ws.remove(id);
                }
            }
        });
        s.set_timer(t.abort_handle());
    }

    /// Queues `r`, a received value of `n` message bytes (0 for the close), and answers a
    /// waiting receive.
    fn queue(&mut self, id: i64, r: WsReceived, n: usize) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        s.queue.push_back((r, n));
        s.queued += n;
        if let Some(w) = s.waiter.take()
            && let Some(r) = self.take(id)
        {
            self.complete(w, status::OK, r.encode());
        }
    }

    /// The next received value; a paused socket resumes once the queue is no longer full.
    fn take(&mut self, id: i64) -> Option<WsReceived> {
        let s = self.socks.get_mut(&id)?;
        let r = match s.queue.pop_front() {
            Some((r, n)) => {
                s.queued -= n;
                r
            }
            None => {
                let (code, reason, clean) = s.closed.clone()?;
                WsReceived::Closed {
                    code,
                    reason,
                    clean,
                }
            }
        };
        if s.paused && !s.full() {
            s.paused = false;
            s.resume.notify_one();
            self.data(id, &[]);
        }
        Some(r)
    }

    /// Queues the close (once) after the messages, answers a waiting receive and ends the
    /// connection, dropping the bytes held past the close.
    fn end(&mut self, id: i64, code: i64, reason: &str, clean: bool) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if s.closed.is_some() {
            return;
        }
        s.closed = Some((code, reason.to_string(), clean));
        s.clear_timer();
        s.buf.clear();
        s.start = 0;
        let r = WsReceived::Closed {
            code,
            reason: reason.to_string(),
            clean,
        };
        self.queue(id, r, 0);
        self.shut(id);
        self.linger(id);
    }

    /// Fails the connection (RFC 6455 §7.1.7): the close frame with `code`, then the end.
    fn fail(&mut self, id: i64, code: i64, reason: &str) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if !s.closing {
            let f = s.close_frame(code, "");
            self.write(id, f);
        }
        if let Some(s) = self.socks.get_mut(&id) {
            s.closing = true;
        }
        self.end(id, code, reason, false);
    }

    /// Parses the frames in `chunk` and whatever is held, stopping while the queue is full; once
    /// the close is known, the peer's further bytes are dropped.
    fn data(&mut self, id: i64, chunk: &[u8]) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if s.closed.is_some() {
            return;
        }
        s.buf.extend_from_slice(chunk);
        self.parse(id);
        if let Some(s) = self.socks.get_mut(&id) {
            s.buf.drain(..s.start.min(s.buf.len()));
            s.start = 0;
        }
    }

    fn parse(&mut self, id: i64) {
        loop {
            let Some(s) = self.socks.get_mut(&id) else {
                return;
            };
            if s.closed.is_some() {
                return;
            }
            if s.full() {
                s.paused = true;
                return;
            }
            let b = &s.buf[s.start..];
            if b.len() < 2 {
                return;
            }
            let fin = b[0] & 0x80 != 0;
            let op = b[0] & 0x0f;
            let mut len = u64::from(b[1] & 0x7f);
            let mut at = 2;
            if len == 126 {
                if b.len() < 4 {
                    return;
                }
                len = u64::from(u16::from_be_bytes([b[2], b[3]]));
                at = 4;
            } else if len == 127 {
                if b.len() < 10 {
                    return;
                }
                let big = u64::from_be_bytes(b[2..10].try_into().expect("8 bytes"));
                len = if big > s.max { u64::MAX } else { big };
                at = 10;
            }
            let control = op >= 8;
            // Reserved bits (no extension was agreed), a masked server frame or an unmasked
            // client one, an unknown opcode, a fragmented or long control frame, or a
            // continuation out of place (RFC 6455 §5.1–§5.5).
            let masked = b[1] & 0x80 != 0;
            if b[0] & 0x70 != 0 || masked != s.server || ![0, 1, 2, 8, 9, 10].contains(&op) {
                return self.fail(id, 1002, "protocol error");
            }
            let misplaced = if control {
                !fin || len > 125
            } else {
                (op == 0) == (s.op == 0)
            };
            if misplaced {
                return self.fail(id, 1002, "protocol error");
            }
            if !control && (s.frags.len() as u64).saturating_add(len) > s.max {
                return self.fail(id, 1009, "message too large");
            }
            let key = if masked { 4 } else { 0 };
            if (b.len() as u64) < (at + key) as u64 + len {
                return;
            }
            let len = len as usize;
            let mut payload = b[at + key..at + key + len].to_vec();
            if masked {
                let mask = [b[at], b[at + 1], b[at + 2], b[at + 3]];
                for (i, x) in payload.iter_mut().enumerate() {
                    *x ^= mask[i & 3];
                }
            }
            s.start += at + key + len;
            match op {
                9 => {
                    if !s.closing && s.pending < HIGH_WATER {
                        let f = s.frame(10, &payload);
                        self.write(id, f);
                    }
                }
                8 => self.peer_close(id, &payload),
                10 => {}
                _ => {
                    if op != 0 {
                        s.op = op;
                    }
                    s.frags.extend_from_slice(&payload);
                    if fin {
                        self.message(id);
                    }
                }
            }
        }
    }

    /// The message the held fragments make, queued; text that is not UTF-8 fails with 1007.
    fn message(&mut self, id: i64) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        let data = std::mem::take(&mut s.frags);
        let text = s.op == 1;
        s.op = 0;
        if text && std::str::from_utf8(&data).is_err() {
            return self.fail(id, 1007, "invalid UTF-8");
        }
        let n = data.len();
        self.queue(id, WsReceived::Message { text, data }, n);
    }

    /// The peer's close frame: answered unless this side closed first, then the clean end.
    fn peer_close(&mut self, id: i64, payload: &[u8]) {
        if payload.len() == 1 {
            return self.fail(id, 1002, "protocol error");
        }
        let code = match payload {
            [a, b, ..] => i64::from(u16::from_be_bytes([*a, *b])),
            _ => 1005,
        };
        let Ok(reason) = std::str::from_utf8(payload.get(2..).unwrap_or_default()) else {
            return self.fail(id, 1007, "invalid UTF-8");
        };
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if !s.closing {
            let f = if payload.is_empty() {
                s.frame(8, payload)
            } else {
                s.close_frame(code, "")
            };
            self.write(id, f);
        }
        self.end(id, code, reason, true);
    }

    /// `file_close`'s step for a socket: 1001 when still open, then the connection released,
    /// its slot given back and its subscriptions ended.
    pub(crate) fn free(&mut self, id: i64) {
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        if s.freed {
            return;
        }
        s.clear_timer();
        if !s.closing && s.closed.is_none() {
            let f = s.close_frame(1001, "");
            self.write(id, f);
        }
        let Some(s) = self.socks.get_mut(&id) else {
            return;
        };
        s.closing = true;
        s.freed = true;
        let topics = std::mem::take(&mut s.topics);
        let destroyed = s.destroyed;
        self.shut(id);
        self.linger(id);
        self.count -= 1;
        for t in topics {
            self.leave(id, &t);
        }
        if destroyed {
            self.remove(id);
        }
    }

    fn leave(&mut self, id: i64, topic: &str) {
        if let Some(ids) = self.topics.get_mut(topic) {
            ids.retain(|&i| i != id);
            if ids.is_empty() {
                self.topics.remove(topic);
            }
        }
        if let Some(s) = self.socks.get_mut(&id) {
            s.topics.retain(|t| t != topic);
        }
    }

    /// `ws_receive`: the answer now, or `None` with `request` waiting for the next value.
    pub(crate) fn receive(&mut self, id: i64, request: u32) -> Option<(u32, Vec<u8>)> {
        let Some(s) = self.socks.get_mut(&id).filter(|s| !s.freed) else {
            return Some((status::ERR, b"the socket is closed".to_vec()));
        };
        if !s.queue.is_empty() || s.closed.is_some() {
            return self.take(id).map(|r| (status::OK, r.encode()));
        }
        s.waiter = Some(request);
        None
    }

    /// `ws_send`: the answer now (`[0]` once closing or closed, `[1]` when the buffer took it),
    /// or `None` with `request` waiting for the drain.
    pub(crate) fn send(&mut self, id: i64, text: bool, payload: &[u8], request: u32) -> Option<u8> {
        let Some(s) = self.socks.get(&id).filter(|s| s.open()) else {
            return Some(0);
        };
        let f = s.frame(if text { 1 } else { 2 }, payload);
        if self.write(id, f) {
            return Some(1);
        }
        let s = self.socks.get_mut(&id)?;
        if s.destroyed {
            return Some(1);
        }
        s.drains.push(request);
        None
    }

    /// A cancelled receive or send stops waiting.
    pub(crate) fn forget(&mut self, id: i64, request: u32) {
        if let Some(s) = self.socks.get_mut(&id) {
            if s.waiter == Some(request) {
                s.waiter = None;
            }
            s.drains.retain(|&r| r != request);
        }
    }

    /// `ws_close`: starts the closing handshake; 5 s later, 1006.
    pub(crate) fn close(&mut self, id: i64, code: i64, reason: &str) {
        let me = self.me.clone();
        let Some(s) = self.socks.get_mut(&id).filter(|s| s.open()) else {
            return;
        };
        s.closing = true;
        let f = s.close_frame(code, reason);
        let t = tokio::spawn(async move {
            tokio::time::sleep(CLOSE).await;
            if let Some(shared) = me.upgrade() {
                lock(&shared).destroy(id);
            }
        });
        if let Some(s) = self.socks.get_mut(&id) {
            s.set_timer(t.abort_handle());
        }
        self.write(id, f);
    }

    pub(crate) fn subscribe(&mut self, id: i64, topic: &str, join: bool) -> bool {
        let Some(s) = self.socks.get_mut(&id).filter(|s| s.open()) else {
            return false;
        };
        if !join {
            self.leave(id, topic);
            return true;
        }
        if !s.topics.iter().any(|t| t == topic) {
            s.topics.push(topic.to_string());
        }
        let ids = self.topics.entry(topic.to_string()).or_default();
        if !ids.contains(&id) {
            ids.push(id);
        }
        true
    }

    /// Writes the message to every socket subscribed to `topic` but `except` that is open, whose
    /// `max_message` it fits and whose write buffer holds less than that (Bun's
    /// `backpressureLimit`), without waiting for any: how many.
    pub(crate) fn publish(
        &mut self,
        topic: &str,
        text: bool,
        payload: &[u8],
        except: Option<i64>,
    ) -> i64 {
        let ids = self.topics.get(topic).cloned().unwrap_or_default();
        let op = if text { 1 } else { 2 };
        let n = payload.len();
        let mut unmasked = None;
        let mut count = 0;
        for id in ids {
            let Some(s) = self.socks.get(&id) else {
                continue;
            };
            if Some(id) == except || !s.open() || n as u64 > s.max || s.pending as u64 >= s.max {
                continue;
            }
            // A client's frame needs a mask of its own; a server's is the same bytes for all.
            let f = if s.server {
                unmasked
                    .get_or_insert_with(|| frame(op, payload, false))
                    .clone()
            } else {
                frame(op, payload, true)
            };
            self.write(id, f);
            count += 1;
        }
        count
    }

    /// A served request's connection upgraded by `ws_accept`: socket `id`, its slot taken.
    pub(crate) fn accepted(&mut self, id: i64, max: i64) -> Result<(), crate::HostError> {
        self.reserve()?;
        self.socks.insert(id, Sock::new(true, max));
        Ok(())
    }

    /// A cancelled `ws_connect`: its slot back, or its socket closed once it opened.
    pub(crate) fn abandon(&mut self, id: i64) {
        if let Some(i) = self.connecting.iter().position(|&c| c == id) {
            self.connecting.remove(i);
            self.count -= 1;
        } else {
            self.free(id);
        }
    }

    /// Close every socket at the instance's end.
    pub(crate) fn close_all(&mut self) {
        let ids: Vec<i64> = self.socks.keys().copied().collect();
        for id in ids {
            if let Some(s) = self.socks.get_mut(&id) {
                s.clear_timer();
                for h in s.io.drain(..) {
                    h.abort();
                }
            }
        }
        self.socks.clear();
        self.topics.clear();
    }
}

/// Runs socket `id` over `io`: its reader and writer tasks.
fn run<T>(shared: &Shared, id: i64, io: T)
where
    T: AsyncRead + AsyncWrite + Send + 'static,
{
    let mut ws = lock(shared);
    let Some(s) = ws.socks.get_mut(&id) else {
        return;
    };
    let Some(rx) = s.rx.take().filter(|_| !s.destroyed) else {
        return;
    };
    let (r, w) = tokio::io::split(io);
    let reader = tokio::spawn(read(shared.clone(), id, r, s.resume.clone()));
    let writer = tokio::spawn(write(shared.clone(), id, w, rx));
    s.io = vec![reader.abort_handle(), writer.abort_handle()];
}

async fn read<R: AsyncRead + Unpin>(shared: Shared, id: i64, mut r: R, resume: Arc<Notify>) {
    let mut chunk = vec![0; 64 << 10];
    loop {
        loop {
            match lock(&shared).socks.get(&id) {
                None => return,
                Some(s) if s.paused => {}
                Some(_) => break,
            }
            resume.notified().await;
        }
        let n = r.read(&mut chunk).await;
        let mut ws = lock(&shared);
        match n {
            Ok(0) | Err(_) => return ws.eof(id),
            Ok(n) => ws.data(id, &chunk[..n]),
        }
    }
}

async fn write<W: AsyncWrite + Unpin>(
    shared: Shared,
    id: i64,
    mut w: W,
    mut rx: mpsc::UnboundedReceiver<Out>,
) {
    while let Some(o) = rx.recv().await {
        match o {
            Out::Frame(b) => {
                let ok = w.write_all(&b).await.is_ok() && w.flush().await.is_ok();
                let mut ws = lock(&shared);
                ws.written(id, b.len());
                if !ok {
                    return ws.destroy(id);
                }
            }
            Out::End => {
                let _ = w.shutdown().await;
                return;
            }
        }
    }
}

/// A server socket's connection, once `hyper` hands it over after the 101.
pub(crate) async fn served(shared: Shared, id: i64, upgrade: hyper::upgrade::OnUpgrade) {
    match upgrade.await {
        Ok(io) => run(&shared, id, hyper_util::rt::TokioIo::new(io)),
        Err(_) => lock(&shared).eof(id),
    }
}

/// `ws_connect`'s checks before anything opens (JS order): the URL, the `net` list, the
/// subprotocols, the headers; then a slot. The request to send and its key, or why not.
pub(crate) fn prepare(
    ws: &mut Ws,
    net: Option<&[String]>,
    client: &reqwest::Client,
    c: &sakura_rt::native::WsConnect<'_>,
) -> Result<(reqwest::Request, String), crate::HostError> {
    use crate::HostError::Err as E;
    let mut url = reqwest::Url::parse(c.url).map_err(|_| E(format!("invalid URL {}", c.url)))?;
    if let Some(refused) = http::refusal(net, c.url) {
        return Err(E(refused));
    }
    for (i, p) in c.subprotocols.iter().enumerate() {
        if !token(p) || c.subprotocols[..i].contains(p) {
            return Err(E(format!("invalid or repeated subprotocol {}", json(p))));
        }
    }
    for (name, _) in &c.headers {
        if let Some(why) = own(name) {
            return Err(E(why));
        }
    }
    let mut key = [0; 16];
    getrandom::fill(&mut key).expect("the OS random source is readable");
    let key = http::base64(&key);
    let scheme = if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    let _ = url.set_scheme(scheme);
    let mut b = client.get(url);
    for (name, value) in &c.headers {
        b = b.header(*name, *value);
    }
    b = b
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-key", &key)
        .header("sec-websocket-version", "13");
    if !c.subprotocols.is_empty() {
        b = b.header("sec-websocket-protocol", c.subprotocols.join(", "));
    }
    let rq = b.build().map_err(|e| E(failure(&e)))?;
    ws.reserve()?;
    Ok((rq, key))
}

/// A prepared `ws_connect`: its request, socket id (slot taken), key, offer and bound.
pub(crate) struct Dial {
    pub request: u32,
    pub id: i64,
    pub key: String,
    pub offered: Vec<String>,
    pub max: i64,
}

/// The handshake of a prepared `ws_connect`: completes its request with the id and the chosen
/// subprotocol, or the reason (the slot given back).
pub(crate) async fn connect(
    shared: Shared,
    client: reqwest::Client,
    rq: reqwest::Request,
    d: Dial,
) {
    let Dial {
        request,
        id,
        key,
        offered,
        max,
    } = d;
    let r = handshake(client, rq, &key, &offered).await;
    let mut ws = lock(&shared);
    ws.connecting.retain(|&c| c != id);
    match r {
        Ok((io, chosen)) => {
            ws.socks.insert(id, Sock::new(false, max));
            drop(ws);
            run(&shared, id, io);
            let mut payload = id.to_le_bytes().to_vec();
            payload.extend_from_slice(chosen.as_bytes());
            lock(&shared).complete(request, status::OK, payload);
        }
        Err(why) => {
            ws.count -= 1;
            ws.complete(request, status::ERR, why.into_bytes());
        }
    }
}

async fn handshake(
    client: reqwest::Client,
    rq: reqwest::Request,
    key: &str,
    offered: &[String],
) -> Result<(reqwest::Upgraded, String), String> {
    let r = client.execute(rq).await.map_err(|e| failure(&e))?;
    if r.status().as_u16() != 101 {
        return Err(format!(
            "the handshake answered {}, not 101",
            r.status().as_u16()
        ));
    }
    let h = r.headers();
    let get = |n: &str| {
        h.get(n)
            .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
    };
    let chosen = get("sec-websocket-protocol").unwrap_or_default();
    if get("sec-websocket-accept").as_deref() != Some(accept_of(key).as_str()) {
        return Err("the handshake answered a wrong sec-websocket-accept".into());
    }
    if !chosen.is_empty() && !offered.contains(&chosen) {
        return Err(format!(
            "the handshake chose subprotocol {}, which was not offered",
            json(&chosen)
        ));
    }
    if h.contains_key("sec-websocket-extensions") {
        return Err("the handshake answered an extension that was not offered".into());
    }
    let io = r.upgrade().await.map_err(|e| failure(&e))?;
    Ok((io, chosen))
}

impl Ws {
    /// A `ws_connect` in flight for socket `id` (its slot taken by `prepare`).
    pub(crate) fn connecting(&mut self, id: i64) {
        self.connecting.push(id);
    }
}

/// Why `ws_accept`'s `headers` and subprotocol cannot go on a 101 (JS `wsAcceptOn`'s order after
/// the connection check), or the chosen subprotocol and the 101's headers.
pub(crate) fn accept_headers(
    a: &sakura_rt::native::WsAccept<'_>,
) -> Result<(String, Head), String> {
    let mut extra = vec![];
    for (name, value) in &a.headers {
        if let Some(why) = own(name) {
            return Err(why);
        }
        if let Some(why) = crate::serve::header_error(name, value) {
            return Err(why);
        }
        extra.push((name.to_string(), value.chars().map(|c| c as u8).collect()));
    }
    let chosen = a.subprotocol.unwrap_or_default();
    if !chosen.is_empty() && !token(chosen) {
        return Err(format!("invalid subprotocol {}", json(chosen)));
    }
    let mut headers = vec![
        ("upgrade".to_string(), b"websocket".to_vec()),
        ("connection".to_string(), b"Upgrade".to_vec()),
        (
            "sec-websocket-accept".to_string(),
            accept_of(a.key).into_bytes(),
        ),
    ];
    if !chosen.is_empty() {
        headers.push((
            "sec-websocket-protocol".to_string(),
            chosen.as_bytes().to_vec(),
        ));
    }
    headers.extend(extra);
    Ok((chosen.to_string(), headers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accept_key_is_rfc_6455_s_sample() {
        assert_eq!(
            accept_of("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn frames_carry_their_length_in_three_forms() {
        assert_eq!(frame(1, b"hi", false), [0x81, 2, b'h', b'i']);
        let f = frame(2, &[7; 300], false);
        assert_eq!(&f[..4], [0x82, 126, 1, 44]);
        let f = frame(2, &vec![0; 70_000], false);
        assert_eq!(&f[..10], [0x82, 127, 0, 0, 0, 0, 0, 1, 0x11, 0x70]);
        let f = frame(1, b"abcd", true);
        let m = &f[2..6];
        let body: Vec<u8> = f[6..]
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ m[i & 3])
            .collect();
        assert_eq!((f[1], &body[..]), (0x84, &b"abcd"[..]));
    }

    #[test]
    fn frames_parse_into_messages_close_codes_and_limits() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let shared = shared(tx);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _g = rt.enter();
        let mut ws = lock(&shared);
        let masked = |op: u8, p: &[u8], fin: bool| {
            let mut f = frame(op, p, true);
            if !fin {
                f[0] &= 0x7f;
            }
            f
        };
        // A server socket: fragments joined around a ping, which is answered.
        ws.socks.insert(1, Sock::new(true, 8));
        let mut bytes = masked(1, b"he", false);
        bytes.extend(masked(9, b"p", true));
        bytes.extend(masked(0, b"llo", true));
        ws.data(1, &bytes[..5]);
        ws.data(1, &bytes[5..]);
        assert_eq!(ws.socks[&1].pending, 3, "the pong");
        assert_eq!(
            ws.receive(1, 9),
            Some((status::OK, vec![0, 1, b'h', b'e', b'l', b'l', b'o']))
        );
        assert_eq!(ws.receive(1, 10), None, "waits");
        // Over `max_message`: 1009 before the payload arrives, answering the waiting receive.
        ws.data(1, &masked(2, &[0; 9], true)[..4]);
        let closed = WsReceived::Closed {
            code: 1009,
            reason: "message too large".into(),
            clean: false,
        };
        assert_eq!(rx.try_recv().ok(), Some((10, status::OK, closed.encode())));
        assert_eq!(
            ws.receive(1, 11),
            Some((status::OK, closed.encode())),
            "again"
        );
        // An unmasked client frame is 1002; invalid UTF-8 text 1007; a peer's close is clean.
        for (bytes, code, reason, clean) in [
            (frame(1, b"x", false), 1002, "protocol error", false),
            (masked(1, &[0xc3], true), 1007, "invalid UTF-8", false),
            (masked(8, &[3, 232, b'o', b'k'], true), 1000, "ok", true),
            (masked(8, &[], true), 1005, "", true),
        ] {
            ws.socks.insert(2, Sock::new(true, 8));
            ws.data(2, &bytes);
            let r = WsReceived::Closed {
                code,
                reason: reason.into(),
                clean,
            };
            assert_eq!(ws.receive(2, 12), Some((status::OK, r.encode())));
        }
        // 64 messages fill the queue: the socket pauses, and resumes as receives take them.
        ws.socks.insert(3, Sock::new(true, 1 << 20));
        let many: Vec<u8> = (0..70).flat_map(|_| masked(1, b"m", true)).collect();
        ws.data(3, &many);
        assert!(ws.socks[&3].paused);
        assert_eq!(ws.socks[&3].queue.len(), 64);
        for _ in 0..70 {
            assert!(ws.receive(3, 13).is_some());
        }
        assert!(!ws.socks[&3].paused);
        // Topics: an open subscriber but `except`, in the order they joined.
        for id in [4, 5] {
            ws.socks.insert(id, Sock::new(true, 4));
            ws.count += 1;
            assert!(ws.subscribe(id, "room", true));
        }
        assert_eq!(ws.publish("room", true, b"hi", Some(4)), 1);
        assert_eq!(ws.publish("room", true, b"too long", None), 0);
        ws.free(5);
        assert_eq!(ws.publish("room", true, b"hi", None), 1);
        assert!(!ws.subscribe(5, "room", true));
        assert_eq!(
            ws.receive(5, 14),
            Some((status::ERR, b"the socket is closed".to_vec()))
        );
    }
}
