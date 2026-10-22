//! `Serve.host()` (design §17.170 (2)): JS `serveHttp` over `hyper` on the instance's
//! current-thread executor. `addr` must be in the manifest's `net` list; each request becomes a
//! `Request` for the guest ([`Incoming`], answered by [`answer`]). A body over 1 MiB is a 413
//! with the connection closed and the handler not run (so is a `Request` past
//! `SAKURA_MAX_COPY_BYTES`, reported as `D091`); a response Node would refuse is a `D091`
//! defect answered 500; a `D096` is 503; a failure or defect is reported on stderr and answered
//! 500; once the server stops a request still arriving is 503 with the connection closed. An
//! upgrade request (`connection: upgrade` with an `upgrade` header, Node's `upgrade` event)
//! reaches the guest with body `Empty`, nothing of it read; `ws_accept` answers it 101 and the
//! connection becomes the socket's (`ws`), else its answer carries `connection: close`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use sakura_rt::native::serve::{Invoked, Served};
use sakura_rt::native::{Instance, Outcome};
use sakura_rt::vt::t;
use tokio::sync::{mpsc, oneshot};

use crate::{HostError, Inflight, NativeHost, ws};

/// The largest request body the server reads (design §17.159 (6)).
pub const MAX_REQUEST_BODY: usize = 1 << 20;

/// `std::request::Method`'s constructors, as the methods' names.
const METHODS: [&str; 8] = [
    "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
];

/// An HTTP answer for the connection.
pub struct Reply {
    status: u16,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
}

/// A request read in full, waiting for the guest.
pub struct Incoming {
    listener: u32,
    method: u32,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    reply: oneshot::Sender<Reply>,
    conn: Option<Conn>,
}

/// An upgrade request's connection, for `ws_accept`: the `sec-websocket-key` it carried, and
/// the connection `hyper` hands over after a 101.
pub(crate) struct Conn {
    key: Option<String>,
    upgrade: Option<hyper::upgrade::OnUpgrade>,
    upgraded: bool,
}

/// A served request's task: how to reply, and the response once `serve_respond` gave it (or
/// the `D091` of one past the copy budget).
pub(crate) struct Pending {
    /// Taken by the 101 of an upgraded connection.
    reply: Option<oneshot::Sender<Reply>>,
    answer: Option<Result<Vec<u8>, String>>,
    conn: Option<Conn>,
}

/// The served requests by task, and the channel requests reach the instance's thread on.
pub(crate) struct Servers {
    pub(crate) pending: HashMap<u32, Pending>,
    pub(crate) tx: mpsc::UnboundedSender<Incoming>,
    pub(crate) rx: mpsc::UnboundedReceiver<Incoming>,
    /// `main` ended with a defect, whose record is the one a waiting request's 500 leaves.
    pub(crate) defected: bool,
}

impl Default for Servers {
    fn default() -> Servers {
        let (tx, rx) = mpsc::unbounded_channel();
        Servers {
            pending: HashMap::new(),
            tx,
            rx,
            defected: false,
        }
    }
}

fn plain(status: u16, text: &str, extra: &[(&str, &str)]) -> Reply {
    let mut headers = vec![("content-type".into(), b"text/plain".to_vec())];
    headers.extend(
        extra
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec())),
    );
    Reply {
        status,
        headers,
        body: text.as_bytes().to_vec(),
    }
}

fn too_large() -> Reply {
    plain(413, "request body too large", &[("connection", "close")])
}

fn internal() -> Reply {
    plain(500, "internal error", &[])
}

impl NativeHost {
    /// `serve_listen` (async): listen on `host:port` for `listener`; `request` completes with the
    /// reason the server could not start or stopped.
    pub(crate) fn serve(&mut self, request: u32, listener: u32, host: &str, port: i64) {
        let at = format!("{host}:{port}");
        let tx = self.tx.clone();
        let fail = move |reason: String| {
            let _ = tx.send((request, sakura_rt::exec::outcome::OK, reason.into_bytes()));
        };
        let granted = self
            .config
            .permissions
            .as_ref()
            .and_then(|p| p.net.as_ref());
        if granted.is_some_and(|net| !net.contains(&at)) {
            self.inflight.insert(request, Inflight::Queued);
            return fail(format!("{at} is not in the manifest's net list"));
        }
        let open = Arc::new(AtomicBool::new(true));
        let bound = {
            let _guard = self.executor.enter();
            std::net::TcpListener::bind((host, port as u16))
                .and_then(|l| l.set_nonblocking(true).map(|()| l))
                .and_then(tokio::net::TcpListener::from_std)
        };
        let socket = match bound {
            Ok(s) => s,
            Err(e) => {
                self.inflight.insert(request, Inflight::Queued);
                return fail(node_listen_error(&e, &at));
            }
        };
        let (incoming, flag) = (self.servers.tx.clone(), open.clone());
        let task = self.executor.spawn(async move {
            loop {
                let Ok((stream, _)) = socket.accept().await else {
                    continue;
                };
                // As Node's server (`noDelay`), which a WebSocket's small frames need.
                let _ = stream.set_nodelay(true);
                let (incoming, open) = (incoming.clone(), flag.clone());
                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        let (incoming, open) = (incoming.clone(), open.clone());
                        async move {
                            Ok::<_, Infallible>(serve_one(req, listener, incoming, open).await)
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .with_upgrades()
                        .await;
                });
            }
        });
        self.inflight
            .insert(request, Inflight::Serve(task.abort_handle(), open));
    }

    /// `serve_respond`: the response of the served request whose task is `task`.
    pub(crate) fn respond(&mut self, task: u32, response: &[u8]) {
        let max = self.config.max_copy_bytes;
        if let Some(p) = self.servers.pending.get_mut(&task) {
            p.answer = Some(if response.len() as u64 > max {
                Err(format!(
                    "response too large: {} bytes over SAKURA_MAX_COPY_BYTES",
                    response.len()
                ))
            } else {
                Ok(response.to_vec())
            });
        }
    }

    /// `ws_accept` (JS `wsAcceptOn`): upgrades the served request answered by `task`, once and
    /// only for the request whose `sec-websocket-key` `a` read: answers it 101 (`upgrade`,
    /// `connection`, `sec-websocket-accept`, `sec-websocket-protocol` when one was chosen, then
    /// `a.headers`) and opens the server socket. Its id and the subprotocol, or why not.
    pub(crate) fn accept(
        &mut self,
        task: u32,
        a: &sakura_rt::native::WsAccept<'_>,
    ) -> Result<(i64, String), HostError> {
        let not = || HostError::Err("not a served upgrade request".into());
        let p = self.servers.pending.get_mut(&task).ok_or_else(not)?;
        let conn = (p.conn.as_mut())
            .filter(|c| !c.upgraded && c.key.as_deref() == Some(a.key))
            .ok_or_else(not)?;
        let (chosen, headers) = ws::accept_headers(a).map_err(HostError::Err)?;
        if p.reply.as_ref().is_none_or(|r| r.is_closed()) {
            return Err(HostError::Err("the connection is closed".into()));
        }
        let id = self.files.reserve_id();
        let _enter = self.executor.enter();
        ws::lock(&self.ws).accepted(id, a.max_message)?;
        conn.upgraded = true;
        if let Some(reply) = p.reply.take() {
            let _ = reply.send(Reply {
                status: 101,
                headers,
                body: vec![],
            });
        }
        let shared = self.ws.clone();
        match conn.upgrade.take() {
            Some(u) => drop(tokio::spawn(ws::served(shared, id, u))),
            None => ws::lock(&shared).free(id),
        }
        Ok((id, chosen))
    }

    /// One `defect` record on stderr, as `serveHttp`'s `report` (no spans).
    fn report(&mut self, code: &str, message: &str) {
        let text = |s: &str| serde_json::Value::from(s).to_string();
        self.write_err(&format!(
            "{{\"schema_version\":\"1.0\",\"kind\":\"defect\",\"code\":{},\"message\":{}}}",
            text(code),
            text(message)
        ));
    }

    /// The HTTP answer of a served request's outcome `o`, with its response `answer` when it
    /// returned (`serveHttp`'s `run` callback).
    fn answer(&mut self, o: Outcome, answer: Option<Result<Vec<u8>, String>>) -> Reply {
        let defect = match (o, answer) {
            (Outcome::Ok, Some(Ok(v))) => match response(&v) {
                Ok(r) => return r,
                Err(m) => (
                    "D091".to_string(),
                    format!("host operation failed: invalid response: {m}"),
                ),
            },
            (Outcome::Ok, Some(Err(m))) => ("D091".to_string(), m),
            (Outcome::Defect { code, message, .. }, _) => (format!("D{code:03}"), message),
            (Outcome::Fail(shown), _) => {
                self.write_err(&format!("error: request failed with {shown}"));
                return internal();
            }
            _ => return internal(),
        };
        self.report(&defect.0, &defect.1);
        if defect.0 == "D096" {
            return plain(503, "server out of memory", &[("retry-after", "1")]);
        }
        internal()
    }

    /// Close the servers' bookkeeping at the instance's end: every request still waiting is
    /// answered 500 once, with one `D090` "server stopped" record unless `main` defected.
    pub(crate) fn close_servers(&mut self) {
        if !self.servers.pending.is_empty() && !self.servers.defected {
            self.report("D090", "server stopped");
        }
        for p in self.servers.pending.drain().filter_map(|(_, p)| p.reply) {
            let _ = p.send(internal());
        }
    }
}

/// Node's `listen` error text for `e` on `at` (`listen EADDRINUSE: address already in use
/// 127.0.0.1:8080`), as JS's `ServeError` reason carries it.
fn node_listen_error(e: &std::io::Error, at: &str) -> String {
    use std::io::ErrorKind as K;
    let (code, what) = match e.kind() {
        K::AddrInUse => ("EADDRINUSE", "address already in use"),
        K::PermissionDenied => ("EACCES", "permission denied"),
        K::AddrNotAvailable => ("EADDRNOTAVAIL", "address not available"),
        _ => return format!("listen {e} {at}"),
    };
    format!("listen {code}: {what} {at}")
}

/// Whether `h` asks for an upgrade as Node's parser reads it: `connection` names `upgrade` and
/// an `upgrade` header is present.
fn upgrade_asked(h: &hyper::HeaderMap) -> bool {
    h.contains_key(hyper::header::UPGRADE)
        && h.get_all(hyper::header::CONNECTION).iter().any(|v| {
            v.to_str().is_ok_and(|v| {
                v.split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
            })
        })
}

/// Read one request (its body bounded) and wait for the guest's answer. An upgrade request's
/// body is not read, and an answer other than its 101 closes the connection.
async fn serve_one(
    mut req: hyper::Request<hyper::body::Incoming>,
    listener: u32,
    incoming: mpsc::UnboundedSender<Incoming>,
    open: Arc<AtomicBool>,
) -> hyper::Response<Full<Bytes>> {
    if upgrade_asked(req.headers()) {
        let conn = Conn {
            key: (req.headers().get("sec-websocket-key"))
                .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned()),
            upgrade: Some(hyper::upgrade::on(&mut req)),
            upgraded: false,
        };
        let (parts, _) = req.into_parts();
        let mut r = answer_one(parts, vec![], listener, incoming, open, Some(conn)).await;
        if r.status != 101 {
            r.headers
                .retain(|(k, _)| !matches!(k.as_str(), "connection" | "transfer-encoding"));
            r.headers.push(("connection".into(), b"close".to_vec()));
        }
        return http(r);
    }
    let declared = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok());
    if declared.is_some_and(|n| n > MAX_REQUEST_BODY as u64) {
        return http(too_large());
    }
    let (parts, mut body) = req.into_parts();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let Ok(frame) = frame else {
            return http(too_large());
        };
        if let Some(chunk) = frame.data_ref() {
            if bytes.len() + chunk.len() > MAX_REQUEST_BODY {
                return http(too_large());
            }
            bytes.extend_from_slice(chunk);
        }
    }
    http(answer_one(parts, bytes, listener, incoming, open, None).await)
}

/// The guest's answer to `req` with body `bytes` (`serveHttp`'s `answer`).
async fn answer_one(
    parts: hyper::http::request::Parts,
    bytes: Vec<u8>,
    listener: u32,
    incoming: mpsc::UnboundedSender<Incoming>,
    open: Arc<AtomicBool>,
    conn: Option<Conn>,
) -> Reply {
    if !open.load(Ordering::Relaxed) {
        return plain(503, "server stopped", &[("connection", "close")]);
    }
    let Some(method) = METHODS.iter().position(|m| *m == parts.method.as_str()) else {
        return plain(405, "method not allowed", &[]);
    };
    // ponytail: header names arrive lowercased and grouped by name (hyper's `HeaderMap`), where
    // Node's `rawHeaders` keep the client's case and order; names are case-insensitive.
    let headers = parts
        .headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let (reply, answer) = oneshot::channel();
    let sent = incoming.send(Incoming {
        listener,
        method: method as u32,
        url: parts
            .uri
            .path_and_query()
            .map_or_else(|| parts.uri.to_string(), |p| p.to_string()),
        headers,
        body: bytes,
        reply,
        conn,
    });
    match (sent, answer.await) {
        (Ok(()), Ok(r)) => r,
        _ => internal(),
    }
}

fn http(r: Reply) -> hyper::Response<Full<Bytes>> {
    let mut out = hyper::Response::builder().status(r.status);
    for (k, v) in &r.headers {
        out = out.header(k.as_str(), v.as_slice());
    }
    if r.status != 101 {
        out = out.header(hyper::header::CONTENT_LENGTH, r.body.len());
    }
    out.body(Full::new(Bytes::from(r.body)))
        .unwrap_or_else(|_| hyper::Response::new(Full::new(Bytes::new())))
}

/// The `Response` in `v` (the `sakura-vt1` tuple `serve_respond` sends) as the reply Node's
/// `writeHead` would send, or why Node refuses it.
fn response(v: &[u8]) -> Result<Reply, String> {
    let mut r = Vt { b: v, at: 0 };
    let bad = || "a malformed response".to_string();
    r.head(t::TUPLE, 4).ok_or_else(bad)?;
    r.head(t::RECORD, 8).ok_or_else(bad)?;
    let status = r.int().ok_or_else(bad)?;
    let n = r.head(t::LIST, 4).ok_or_else(bad)?;
    let mut headers: Vec<(String, String)> = vec![];
    for _ in 0..n {
        r.head(t::TUPLE, 4).ok_or_else(bad)?;
        let (k, v) = (r.string().ok_or_else(bad)?, r.string().ok_or_else(bad)?);
        // `Object.fromEntries`: a repeated name keeps its first place and its last value.
        match headers.iter_mut().find(|(n, _)| *n == k) {
            Some(h) => h.1 = v,
            None => headers.push((k, v)),
        }
    }
    r.head(t::VARIANT, 4).ok_or_else(bad)?;
    let ctor = r.u32().ok_or_else(bad)?;
    r.u32().ok_or_else(bad)?;
    let body = match ctor {
        0 => vec![],
        1 => r.string().ok_or_else(bad)?.into_bytes(),
        _ => r.bytes().ok_or_else(bad)?,
    };
    if status == 101 {
        return Err("101 for a connection that was not upgraded".into());
    }
    if !(100..=999).contains(&status) {
        return Err(format!("Invalid status code: {status}"));
    }
    let mut out = vec![];
    for (k, v) in headers {
        if k == "content-length" {
            continue;
        }
        if let Some(why) = header_error(&k, &v) {
            return Err(why);
        }
        // Node writes a header value as latin1.
        out.push((k, v.chars().map(|c| c as u8).collect()));
    }
    Ok(Reply {
        status: status as u16,
        headers: out,
        body,
    })
}

/// Why Node's `validateHeaderName` or `validateHeaderValue` refuses header `k: v`.
pub(crate) fn header_error(k: &str, v: &str) -> Option<String> {
    if !ws::token(k) {
        return Some(format!("Header name must be a valid HTTP token [\"{k}\"]"));
    }
    let ok = |c: char| c == '\t' || (' '..='~').contains(&c) || ('\u{80}'..='\u{ff}').contains(&c);
    (!v.chars().all(ok)).then(|| format!("Invalid character in header content [\"{k}\"]"))
}

/// A reader of a `sakura-vt1` value: `head` checks a tag and returns the first `u32` after it,
/// skipping `len` bytes of header in all.
struct Vt<'a> {
    b: &'a [u8],
    at: usize,
}

impl Vt<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    /// Tag `tag`, then `len` bytes of words; the last word (a count or a constructor).
    fn head(&mut self, tag: u8, len: usize) -> Option<u32> {
        (self.take(1)? == [tag]).then_some(())?;
        let mut last = 0;
        for _ in 0..len / 4 {
            last = self.u32()?;
        }
        Some(last)
    }
    fn int(&mut self) -> Option<i64> {
        (self.take(1)? == [t::INT]).then_some(())?;
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.head(t::STRING, 4)? as usize;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.head(t::BYTES, 4)? as usize;
        Some(self.take(n)?.to_vec())
    }
}

/// A request read on the executor: start its task (`invoke`), or answer it at once when the
/// guest cannot take it.
pub fn deliver(inst: &mut Instance<NativeHost>, r: Incoming) {
    let served = Served {
        method: r.method,
        url: &r.url,
        headers: &r.headers,
        body: &r.body,
    };
    match inst.serve_invoke(r.listener, &served) {
        Invoked::Task(task) => {
            inst.host().servers.pending.insert(
                task,
                Pending {
                    reply: Some(r.reply),
                    answer: None,
                    conn: r.conn,
                },
            );
        }
        Invoked::TooLarge(n) => {
            let host = inst.host();
            host.report(
                "D091",
                &format!("request too large: {n} bytes over SAKURA_MAX_COPY_BYTES"),
            );
            let _ = r.reply.send(too_large());
        }
        Invoked::NotStarted => {
            let host = inst.host();
            host.report("D096", "heap exhausted: request not started");
            let _ = r
                .reply
                .send(plain(503, "server out of memory", &[("retry-after", "1")]));
        }
    }
}

/// Answer the served requests whose tasks finished (WASM's `reply`).
pub fn reply(inst: &mut Instance<NativeHost>) {
    let tasks: Vec<u32> = inst.host().servers.pending.keys().copied().collect();
    for task in tasks {
        let o = inst.outcome(task);
        if o == Outcome::Pending {
            continue;
        }
        let host = inst.host();
        let Some(p) = host.servers.pending.remove(&task) else {
            continue;
        };
        if p.conn.as_ref().is_some_and(|c| c.upgraded) {
            // The connection is a WebSocket now: the handler's return is discarded, and
            // `file_close` closed the socket with 1001 (design §17.289 (10)).
            match o {
                Outcome::Defect { code, message, .. } => {
                    host.report(&format!("D{code:03}"), &message);
                }
                Outcome::Fail(shown) => {
                    host.write_err(&format!("error: request failed with {shown}"))
                }
                _ => {}
            }
            continue;
        }
        let r = host.answer(o, p.answer);
        if let Some(reply) = p.reply {
            let _ = reply.send(r);
        }
    }
}
