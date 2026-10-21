//! The native host (design §17.170 (1), §17.171; ABI §1b, §8.4–§8.6): one instance's embedder
//! for the runtime's [`sakura_rt::native::Instance`]. It supplies the root handlers' imports
//! (Console, Clock, Random, `log_write`, traces), the `hosted` services (Env, Fs, `file_*`,
//! `Cli.args`) and Sql with the JS host's semantics, keeps the instance's handle and file
//! tables, and drives `async` requests on a current-thread Tokio runtime owned by the
//! instance's thread; each database runs on its own worker thread (`sql`). `Http.host()`'s
//! sends, streamed bodies and `Http.recording`'s cassettes are `http`'s. Serve and Llm have no
//! host service until N1b reaches them: their host handlers' operations are `D090` (the
//! runtime's `rt_code_unsupported`).
//!
//! Everything here is safe Rust (the workspace forbids `unsafe_code`): the host never receives
//! the instance context, and its inputs and answers are copied by the runtime.

#![cfg(feature = "native")]

pub mod fs;
mod http;
mod sql;
pub mod worker;

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sakura_rt::exec::outcome;
use sakura_rt::native::{
    FileMode, HandleId, Host, HostError, Hosted, HttpRequest, HttpResponse, Instance, Outcome,
    SqlValue, WorkerGrants,
};
use sakura_rt::vt::{BODY, DIR, SQL};
use tokio::sync::mpsc;

/// `[permissions]` of the manifest (`SAKURA_PERMISSIONS`): `None` lists are no limit (an unset
/// variable); in a parsed table an absent list is empty, so denied (design §17.304).
#[derive(Clone, Debug, Default)]
pub struct Permissions {
    pub fs: Option<fs::Grants>,
    /// The `net` hosts (`host:port`), checked on every Http send and a live worker's grants.
    pub net: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
}

impl Permissions {
    /// `{"fs": {"read": […], "write": […]}, "net": […], "env": […]}` as the driver passes it. As
    /// JS, a set variable is the whole ceiling: an absent list is empty, and a table that is not
    /// an object of string lists is an error naming the shape, never the value. Unknown keys are
    /// ignored.
    pub fn parse(json: &str) -> Result<Permissions, String> {
        const BAD: &str = r#"SAKURA_PERMISSIONS must be a JSON object {"fs": {"read": [...], "write": [...]}, "net": [...], "env": [...]} with string lists"#;
        let v: serde_json::Value = serde_json::from_str(json).map_err(|_| BAD)?;
        let strings = |v: Option<&serde_json::Value>| -> Result<Vec<String>, String> {
            let Some(v) = v else { return Ok(vec![]) };
            v.as_array()
                .ok_or(BAD)?
                .iter()
                .map(|s| s.as_str().map(String::from).ok_or_else(|| BAD.to_string()))
                .collect()
        };
        let fs = v.get("fs");
        if !v.is_object() || fs.is_some_and(|f| !f.is_object()) {
            return Err(BAD.to_string());
        }
        let paths = |key| -> Result<Vec<PathBuf>, String> {
            Ok(strings(fs.and_then(|f| f.get(key)))?
                .into_iter()
                .map(PathBuf::from)
                .collect())
        };
        Ok(Permissions {
            fs: Some(fs::Grants {
                read: paths("read")?,
                write: paths("write")?,
            }),
            net: Some(strings(v.get("net"))?),
            env: Some(strings(v.get("env"))?),
        })
    }
}

/// `SAKURA_MAX_COPY_BYTES`'s default (ABI §8.5), as `Limits::default`.
pub const MAX_COPY_BYTES: u64 = 256 << 20;

/// How an instance's host is set up (`from_env` reads what the Node host reads).
#[derive(Clone, Debug)]
pub struct Config {
    /// `Fs.root()`'s directory.
    pub cwd: PathBuf,
    /// `None`: no limits (as a plain `node` run).
    pub permissions: Option<Permissions>,
    /// Keep stdout and stderr in the host ([`NativeHost::captured`]) instead of writing them.
    pub capture: bool,
    /// Scripted stdin lines; `None` reads the process's stdin.
    pub stdin: Option<Vec<String>>,
    /// `SAKURA_TRACE`: the file effect-trace lines are appended to.
    pub trace: Option<PathBuf>,
    /// `SAKURA_MAX_COPY_BYTES`, the instance's `Limits::max_copy_bytes`: a read stops one byte
    /// past it, so the runtime refuses the answer (`D091`) without the host holding the rest.
    pub max_copy_bytes: u64,
    /// `Cli.args()`: the executable's arguments after its name.
    pub args: Vec<String>,
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let permissions = match std::env::var("SAKURA_PERMISSIONS") {
            Ok(json) => Some(Permissions::parse(&json)?),
            Err(_) => None,
        };
        let max_copy_bytes = match std::env::var("SAKURA_MAX_COPY_BYTES") {
            Ok(n) => n
                .parse::<u64>()
                .map_err(|_| "SAKURA_MAX_COPY_BYTES must be a byte count")?,
            Err(_) => MAX_COPY_BYTES,
        };
        Ok(Config {
            cwd: std::env::current_dir().map_err(|e| e.to_string())?,
            permissions,
            capture: false,
            stdin: None,
            trace: std::env::var_os("SAKURA_TRACE").map(PathBuf::from),
            args: std::env::args().skip(1).collect(),
            max_copy_bytes,
        })
    }

    /// A host for tests: captured output, empty stdin, no limits, no trace.
    pub fn test(cwd: PathBuf) -> Config {
        Config {
            cwd,
            permissions: None,
            capture: true,
            stdin: Some(vec![]),
            trace: None,
            max_copy_bytes: MAX_COPY_BYTES,
            args: vec![],
        }
    }
}

/// One host table entry (ABI §8.4): a slot's generation and what it holds while live.
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// A handle table of one kind: freed slots are reused with the generation incremented, and a
/// slot whose generation would wrap is retired for good (ABI §1b, §8.4).
struct Table<T> {
    kind: u32,
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Table<T> {
    fn new(kind: u32) -> Table<T> {
        Table {
            kind,
            slots: vec![],
            free: vec![],
        }
    }

    fn insert(&mut self, value: T) -> HandleId {
        let slot = match self.free.pop() {
            Some(s) => {
                let e = &mut self.slots[s as usize - 1];
                e.generation += 1;
                e.value = Some(value);
                s
            }
            None => {
                self.slots.push(Slot {
                    generation: 1,
                    value: Some(value),
                });
                self.slots.len() as u32
            }
        };
        HandleId {
            kind: self.kind,
            slot,
            generation: self.slots[slot as usize - 1].generation,
        }
    }

    fn get(&self, h: HandleId) -> Hosted<&T> {
        self.slots
            .get((h.slot as usize).wrapping_sub(1))
            .filter(|e| h.kind == self.kind && e.generation == h.generation)
            .and_then(|e| e.value.as_ref())
            .ok_or_else(|| HostError::Fault("a released or unknown handle".into()))
    }

    fn retire(&mut self, h: HandleId) {
        if let Some(e) = self.slots.get_mut((h.slot as usize).wrapping_sub(1))
            && e.generation == h.generation
            && e.value.take().is_some()
            && e.generation < u32::MAX
        {
            self.free.push(h.slot);
        }
    }

    fn live(&self) -> usize {
        self.slots.iter().filter(|e| e.value.is_some()).count()
    }

    fn clear(&mut self) {
        for (i, e) in self.slots.iter_mut().enumerate() {
            if e.value.take().is_some() && e.generation < u32::MAX {
                self.free.push(i as u32 + 1);
            }
        }
    }
}

/// What [`NativeHost::close`] released (the §8.6 ledger), for tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub aborted_requests: usize,
    pub dropped_completions: usize,
    pub closed_handles: usize,
    pub closed_files: usize,
}

/// A completion waiting for the guest: request, status, payload.
type Completion = (u32, u32, Vec<u8>);

/// A request in flight, as its cancellation reaches it.
enum Inflight {
    /// A sleep or a worker operation on the executor.
    Task(tokio::task::AbortHandle),
    /// A statement on database `.0`'s worker.
    Sql(u32),
    /// `http_stream_open` on the executor, and the `Body` it fills: retired unless the guest
    /// receives it.
    Open(tokio::task::AbortHandle, HandleId),
}

pub struct NativeHost {
    config: Config,
    out: Vec<String>,
    captured_out: String,
    captured_err: String,
    stdin: Option<VecDeque<String>>,
    started: Instant,
    urandom: Option<std::fs::File>,
    trace: Option<std::fs::File>,
    dirs: Table<fs::Dir>,
    /// `Body` handles: `http_body_of`'s buffered ones and `Http.stream`'s live ones.
    bodies: Table<http::Shared>,
    /// `Http.host()`'s client, built at the first send.
    client: Option<reqwest::Client>,
    cassettes: http::Cassettes,
    files: fs::Files,
    executor: tokio::runtime::Runtime,
    tx: mpsc::UnboundedSender<Completion>,
    rx: mpsc::UnboundedReceiver<Completion>,
    inflight: HashMap<u32, Inflight>,
    /// Databases by id (`Sql.memory`, `Sql.sqlite`); `None` once the handler's handle retired.
    dbs: Vec<Option<sql::Db>>,
    /// Every database worker, joined at `close`.
    workers: Vec<std::thread::JoinHandle<()>>,
    /// Live workers (`worker`): the module's worker table, how to start one, the links by id
    /// (0: this worker's parent), and the exits of stopped ones not joined yet.
    worker_fns: Vec<worker::WorkerFn>,
    launcher: Option<worker::Launcher>,
    links: HashMap<i64, worker::Side>,
    exits: HashMap<i64, tokio::sync::watch::Receiver<()>>,
    next_worker: i64,
    /// A worker's link state, which says whether its parent stopped it.
    stopped: Option<std::sync::Arc<worker::Shared>>,
    /// Closes when a worker's instance goes, for its parent's `join`.
    _exit: Option<tokio::sync::watch::Sender<()>>,
    /// Topic messages and stops from other threads, and this instance's topic registry id.
    signal_tx: mpsc::UnboundedSender<worker::Signal>,
    signals: mpsc::UnboundedReceiver<worker::Signal>,
    instance: u64,
}

impl NativeHost {
    pub fn new(config: Config) -> Result<NativeHost, String> {
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let trace = match &config.trace {
            Some(p) => Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .map_err(|e| format!("SAKURA_TRACE: {e}"))?,
            ),
            None => None,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let (signal_tx, signals) = mpsc::unbounded_channel();
        Ok(NativeHost {
            stdin: config.stdin.clone().map(VecDeque::from),
            config,
            out: vec![],
            captured_out: String::new(),
            captured_err: String::new(),
            started: Instant::now(),
            urandom: None,
            trace,
            dirs: Table::new(DIR),
            bodies: Table::new(BODY),
            client: None,
            cassettes: http::Cassettes::default(),
            files: fs::Files::default(),
            executor,
            tx,
            rx,
            inflight: HashMap::new(),
            dbs: vec![],
            workers: vec![],
            worker_fns: vec![],
            launcher: None,
            links: HashMap::new(),
            exits: HashMap::new(),
            next_worker: 1,
            stopped: None,
            _exit: None,
            signal_tx,
            signals,
            instance: worker::instance_id(),
        })
    }

    /// Stdout and stderr so far, with [`Config::capture`].
    pub fn captured(&mut self) -> (&str, &str) {
        self.flush_out();
        (&self.captured_out, &self.captured_err)
    }

    /// Write buffered stdout (before waiting, before stderr, when a task ends; as JS
    /// `flushOut`).
    pub fn flush_out(&mut self) {
        if self.out.is_empty() {
            return;
        }
        if self.is_stopped() {
            self.out.clear();
            return;
        }
        let mut text = self.out.join("\n");
        text.push('\n');
        self.out.clear();
        if self.config.capture {
            self.captured_out.push_str(&text);
        } else {
            let mut o = std::io::stdout().lock();
            let _ = o.write_all(text.as_bytes());
            let _ = o.flush();
        }
    }

    fn write_err(&mut self, line: &str) {
        self.flush_out();
        if self.is_stopped() {
            return;
        }
        if self.config.capture {
            self.captured_err.push_str(line);
            self.captured_err.push('\n');
        } else {
            eprintln!("{line}");
        }
    }

    /// Live `Dir` handles and open files.
    pub fn resources(&self) -> (usize, usize) {
        (self.dirs.live(), self.files.len())
    }

    /// Requests in flight.
    pub fn in_flight(&self) -> usize {
        self.inflight.len()
    }

    /// The next completion of a request still in flight, waiting on the executor; `None` when
    /// nothing is in flight (every task is waiting on the guest itself).
    pub fn next_completion(&mut self) -> Option<Completion> {
        loop {
            if self.inflight.is_empty() {
                // Late completions of aborted requests are discarded with them.
                while self.rx.try_recv().is_ok() {}
                return None;
            }
            let c = self.executor.block_on(self.rx.recv())?;
            match self.inflight.remove(&c.0) {
                Some(Inflight::Open(_, body)) if c.1 != outcome::OK => {
                    self.bodies.retire(body);
                    return Some(c);
                }
                Some(_) => return Some(c),
                None => {}
            }
        }
    }

    /// The resource ledger (ABI §8.6, design §17.171 (4)): abort every request in flight, wait
    /// for the database workers to finish (an aborted statement is interrupted), drop queued
    /// completions, retire every handle and close every file. The instance is gone or never
    /// entered again.
    pub fn close(&mut self) -> Ledger {
        let mut ledger = Ledger {
            aborted_requests: self.inflight.len(),
            closed_handles: self.dirs.live()
                + self.bodies.live()
                + self.dbs.iter().flatten().count(),
            closed_files: self.files.len(),
            ..Ledger::default()
        };
        for request in self.inflight.keys().copied().collect::<Vec<_>>() {
            self.abort(request);
        }
        self.dbs.clear();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
        while self.rx.try_recv().is_ok() {
            ledger.dropped_completions += 1;
        }
        self.dirs.clear();
        self.bodies.clear();
        self.files.close_all();
        self.flush_out();
        self.unlisten();
        ledger
    }

    fn entropy(&mut self, buf: &mut [u8]) {
        // ponytail: `/dev/urandom` (platform A); `getrandom` when platform B lands.
        let f = self.urandom.get_or_insert_with(|| {
            std::fs::File::open("/dev/urandom").expect("/dev/urandom is readable")
        });
        f.read_exact(buf).expect("/dev/urandom is readable");
    }

    fn u64(&mut self) -> u64 {
        let mut b = [0; 8];
        self.entropy(&mut b);
        u64::from_le_bytes(b)
    }

    fn dir(&self, h: HandleId) -> Hosted<fs::Dir> {
        self.dirs.get(h).cloned()
    }

    fn db(&mut self, id: u32) -> Hosted<&mut sql::Db> {
        self.dbs
            .get_mut(id as usize)
            .and_then(Option::as_mut)
            .ok_or_else(|| HostError::Fault("a released or unknown database".into()))
    }
}

impl NativeHost {
    /// Run `f` on the executor for `request`, completing it with `f`'s status and payload;
    /// aborting `request` drops `f` (a connection closes with it).
    fn spawn(
        &mut self,
        request: u32,
        f: impl std::future::Future<Output = http::Done> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let task = self.executor.spawn(async move {
            let (st, payload) = f.await;
            let _ = tx.send((request, st, payload));
        });
        self.inflight
            .insert(request, Inflight::Task(task.abort_handle()));
    }

    /// The client (built at the first send) and the `reqwest` request for `req`.
    fn request_of(
        &mut self,
        req: &HttpRequest<'_>,
    ) -> Result<(reqwest::Client, reqwest::Request), String> {
        let client = match &self.client {
            Some(c) => c.clone(),
            None => self.client.insert(http::client()?).clone(),
        };
        let rq = http::build(&client, req)?;
        Ok((client, rq))
    }
}

fn op<T>(r: Result<T, String>) -> Hosted<T> {
    r.map_err(HostError::Err)
}

impl Host for NativeHost {
    fn console_print(&mut self, line: &str) {
        self.out.push(line.to_string());
        if self.out.len() >= 256 {
            self.flush_out();
        }
    }

    fn console_read_line(&mut self) -> Option<String> {
        if let Some(lines) = &mut self.stdin {
            return lines.pop_front();
        }
        let mut bytes = vec![];
        match std::io::stdin().lock().read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                // Node decodes stdin as UTF-8 with replacement characters.
                let mut line = String::from_utf8_lossy(&bytes).into_owned();
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }
                Some(line)
            }
        }
    }

    fn clock_now_ms(&mut self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn clock_monotonic_ms(&mut self) -> i64 {
        self.started.elapsed().as_millis() as i64
    }

    fn clock_monotonic_us(&mut self) -> i64 {
        self.started.elapsed().as_micros() as i64
    }

    fn clock_sleep_ms(&mut self, request: u32, ms: i64) {
        let tx = self.tx.clone();
        let task = self.executor.spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms.max(0) as u64)).await;
            let _ = tx.send((request, outcome::OK, vec![]));
        });
        self.inflight
            .insert(request, Inflight::Task(task.abort_handle()));
    }

    fn abort(&mut self, request: u32) {
        match self.inflight.remove(&request) {
            Some(Inflight::Task(h)) => h.abort(),
            Some(Inflight::Open(h, body)) => {
                h.abort();
                self.bodies.retire(body);
            }
            Some(Inflight::Sql(db)) => {
                if let Ok(db) = self.db(db) {
                    db.abort(request);
                }
            }
            None => {}
        }
    }

    fn random_int(&mut self, lo: i64, hi: i64) -> i64 {
        // Uniform without modulo bias: rejection over the largest multiple of the range.
        let range = (i128::from(hi) - i128::from(lo) + 1) as u128;
        if range > u128::from(u64::MAX) {
            return self.u64() as i64;
        }
        let zone = (1u128 << 64) / range * range;
        loop {
            let x = u128::from(self.u64());
            if x < zone {
                return (i128::from(lo) + (x % range) as i128) as i64;
            }
        }
    }

    fn random_float(&mut self) -> f64 {
        (self.u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn random_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut b = vec![0; n];
        self.entropy(&mut b);
        b
    }

    fn random_uuid(&mut self) -> String {
        let mut b = [0u8; 16];
        self.entropy(&mut b);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
        format!(
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        )
    }

    fn log_write(&mut self, line: &str) {
        self.write_err(line);
    }

    fn trace_enabled(&mut self) -> bool {
        self.trace.is_some()
    }

    fn trace_write(&mut self, line: &str) {
        self.flush_out();
        if let Some(f) = &mut self.trace {
            let _ = writeln!(f, "{line}");
        }
    }

    fn retire_handle(&mut self, handle: HandleId) {
        match handle.kind {
            DIR => self.dirs.retire(handle),
            BODY => self.bodies.retire(handle),
            // The worker finishes what it was given and closes the connection.
            SQL => {
                if let Some(db) = self.dbs.get_mut(handle.slot as usize) {
                    *db = None;
                }
            }
            _ => {}
        }
    }

    fn topic_listen(&mut self, name: &str) {
        self.listen(name);
    }

    fn topic_post(&mut self, name: &str, text: &str) {
        self.publish(name, text);
    }

    fn worker_start(&mut self, code: u32, grants: WorkerGrants, capacity: i64) -> i64 {
        self.start_worker(code, grants, capacity)
    }

    fn worker_post(&mut self, request: u32, id: i64, text: &str) {
        self.post(request, id, text);
    }

    fn worker_take(&mut self, request: u32, id: i64) {
        self.take(request, id);
    }

    fn worker_join(&mut self, request: u32, id: i64) {
        self.join(request, id);
    }

    fn worker_stop(&mut self, id: i64) {
        self.stop(id);
    }

    fn worker_shut(&mut self, id: i64) {
        self.shut(id);
    }

    fn cli_args(&mut self) -> Hosted<Vec<String>> {
        Ok(self.config.args.clone())
    }

    fn env_get(&mut self, key: &str) -> Hosted<Option<String>> {
        if let Some(allowed) = self
            .config
            .permissions
            .as_ref()
            .and_then(|p| p.env.as_ref())
            && !allowed.iter().any(|k| k == key)
        {
            return Ok(None);
        }
        Ok(std::env::var(key).ok())
    }

    fn fs_root(&mut self) -> Hosted<HandleId> {
        let grants = self.config.permissions.as_ref().and_then(|p| p.fs.as_ref());
        let d = fs::Dir::root(&self.config.cwd, grants).map_err(HostError::Failure)?;
        Ok(self.dirs.insert(d))
    }

    fn fs_memory_root(&mut self, files: Vec<(String, Option<Vec<u8>>)>) -> Hosted<HandleId> {
        Ok(self.dirs.insert(fs::Dir::memory(files)))
    }

    fn fs_subdir(&mut self, dir: HandleId, name: &str) -> Hosted<HandleId> {
        let d = op(self.dir(dir)?.subdir(name))?;
        Ok(self.dirs.insert(d))
    }

    fn fs_read(&mut self, dir: HandleId, path: &str) -> Hosted<Vec<u8>> {
        let limit = self.config.max_copy_bytes.saturating_add(1);
        op(self.dir(dir)?.read(path, limit))
    }

    fn fs_write(&mut self, dir: HandleId, path: &str, data: &[u8]) -> Hosted<()> {
        op(self.dir(dir)?.write(path, data))
    }

    fn fs_list(&mut self, dir: HandleId, path: &str) -> Hosted<Vec<(String, bool)>> {
        op(self.dir(dir)?.list(path))
    }

    fn fs_exists(&mut self, dir: HandleId, path: &str) -> Hosted<bool> {
        Ok(self.dir(dir)?.exists(path))
    }

    fn file_open(&mut self, dir: HandleId, path: &str, mode: FileMode) -> Hosted<i64> {
        let d = self.dir(dir)?;
        op(self.files.open(&d, path, mode))
    }

    fn file_read(&mut self, id: i64, max: i64) -> Hosted<Vec<u8>> {
        let limit = i64::try_from(self.config.max_copy_bytes.saturating_add(1)).unwrap_or(i64::MAX);
        op(self.files.read(id, max.min(limit)))
    }

    fn file_write(&mut self, id: i64, data: &[u8]) -> Hosted<()> {
        op(self.files.write(id, data))
    }

    fn file_close(&mut self, id: i64) {
        self.files.close(id);
    }

    fn http_body_of(&mut self, bytes: &[u8]) -> Hosted<HandleId> {
        let b = http::Body {
            buf: bytes.iter().copied().collect(),
            ..Default::default()
        };
        Ok(self
            .bodies
            .insert(std::sync::Arc::new(std::sync::Mutex::new(b))))
    }

    fn http_body_read(&mut self, body: HandleId, max: i64) -> Hosted<Vec<u8>> {
        let mut b = http::lock(self.bodies.get(body)?);
        if b.closed {
            return Err(HostError::Err("the response body is closed".into()));
        }
        let n = b.buf.len().min(usize::try_from(max).unwrap_or(0));
        Ok(b.buf.drain(..n).collect())
    }

    fn http_body_close(&mut self, body: HandleId) {
        if let Ok(b) = self.bodies.get(body) {
            let mut b = http::lock(b);
            b.closed = true;
            b.live = None;
            b.buf.clear();
        }
    }

    fn http_body_waits(&mut self, body: HandleId) -> bool {
        self.bodies.get(body).is_ok_and(|b| {
            let b = http::lock(b);
            !b.closed && b.buf.is_empty() && b.live.is_some()
        })
    }

    fn http_body_fetch(&mut self, request: u32, body: HandleId, max: i64) {
        let Ok(shared) = self.bodies.get(body).cloned() else {
            let m = b"a released or unknown handle".to_vec();
            return self.spawn(request, async move {
                (sakura_rt::exec::status::HOST_FAILURE, m)
            });
        };
        self.spawn(request, http::read(shared, max));
    }

    fn http_refused(&mut self, req: &HttpRequest<'_>) -> Option<String> {
        let net = self
            .config
            .permissions
            .as_ref()
            .and_then(|p| p.net.as_deref());
        http::refusal(net, req.url)
    }

    fn http_send(&mut self, request: u32, req: &HttpRequest<'_>) {
        let max = self.config.max_copy_bytes;
        match self.request_of(req) {
            Ok((client, rq)) => self.spawn(request, http::send(client, rq, max)),
            Err(m) => self.spawn(request, async move {
                (sakura_rt::exec::status::ERR, m.into_bytes())
            }),
        }
    }

    fn http_stream_open(&mut self, request: u32, req: &HttpRequest<'_>) {
        let (client, rq) = match self.request_of(req) {
            Ok(r) => r,
            Err(m) => {
                return self.spawn(request, async move {
                    (sakura_rt::exec::status::ERR, m.into_bytes())
                });
            }
        };
        let shared = http::Shared::default();
        let h = self.bodies.insert(shared.clone());
        let tx = self.tx.clone();
        let task = self.executor.spawn(async move {
            let (st, payload) =
                http::open(client, rq, shared, [h.kind, h.slot, h.generation]).await;
            let _ = tx.send((request, st, payload));
        });
        self.inflight
            .insert(request, Inflight::Open(task.abort_handle(), h));
    }

    fn http_recording_open(&mut self, dir: HandleId, path: &str) -> Hosted<i64> {
        let d = self.dir(dir)?;
        op(self.cassettes.open(d, path))
    }

    fn http_recording_lookup(
        &mut self,
        id: i64,
        req: &HttpRequest<'_>,
    ) -> Hosted<Option<HttpResponse>> {
        op(self.cassettes.lookup(id, req))
    }

    fn http_recording_append(
        &mut self,
        id: i64,
        req: &HttpRequest<'_>,
        resp: &HttpResponse,
    ) -> Hosted<()> {
        op(self.cassettes.append(id, req, resp))
    }

    fn sql_open(&mut self, path: Option<&str>) -> Hosted<u32> {
        let db = sql::Db::new(path, &mut self.workers).map_err(HostError::Failure)?;
        self.dbs.push(Some(db));
        Ok(self.dbs.len() as u32 - 1)
    }

    fn sql_needs_root(&mut self, db: u32) -> bool {
        self.db(db).is_ok_and(|d| d.needs_root())
    }

    fn sql_root(&mut self, db: u32, dir: HandleId) -> Hosted<()> {
        let d = self.dir(dir)?;
        let db = self.db(db)?;
        let rel = db.file().unwrap_or_default().to_string();
        let path = d
            .sqlite_path(&rel)
            .map_err(|e| HostError::Err(format!("{rel}: {e}")))?;
        db.open(Some(path)).map_err(HostError::Err)?;
        db.opened();
        Ok(())
    }

    fn sql_run(&mut self, request: u32, db: u32, text: &str, params: &[SqlValue<'_>], rows: bool) {
        let (max, tx) = (self.config.max_copy_bytes, self.tx.clone());
        self.inflight.insert(request, Inflight::Sql(db));
        match self.db(db) {
            Ok(d) => d.run(request, text, params, rows, max, tx),
            Err(_) => {
                let m = b"a released or unknown database".to_vec();
                let _ = tx.send((request, sakura_rt::exec::status::HOST_FAILURE, m));
            }
        }
    }

    fn sql_begin(&mut self, db: u32) -> Hosted<i64> {
        self.db(db)?.begin().map_err(HostError::Err)
    }

    fn sql_end(&mut self, db: u32, token: i64, commit: bool) -> Hosted<()> {
        self.db(db)?.end(token, commit).map_err(HostError::Failure)
    }
}

/// Drive `task` to an outcome (the Node host's `runTask`): run until nothing can, then wait on
/// the executor for the next completion of a request in flight or a topic message; with none
/// in flight and no worker link, every task waits on another and the run ends `D090`. A
/// stopped worker's task ends `Cancelled`.
pub fn run_task(inst: &mut Instance<NativeHost>, task: u32) -> Outcome {
    let mut kind = inst.run(task);
    while kind == outcome::PENDING {
        let host = inst.host();
        host.flush_out();
        let r = match host.wait() {
            worker::Wake::Done(req, status, payload) => {
                inst.complete(req, status, &payload).map(|_| ())
            }
            worker::Wake::Topic(name, text) => inst.topic_deliver(&name, &text),
            worker::Wake::Stop => return Outcome::Cancelled,
            worker::Wake::Idle => {
                return Outcome::Defect {
                    code: 90,
                    site: 0,
                    message: "every task is waiting".into(),
                };
            }
        };
        if let Err(d) = r {
            return Outcome::Defect {
                code: d.code,
                site: 0,
                message: d.message.into(),
            };
        }
        kind = inst.run(task);
    }
    inst.host().flush_out();
    inst.outcome(task)
}

/// A source span of a defect record (design §17.162), as the JS and WASM hosts render it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub uri: String,
    pub start_byte: u32,
    pub end_byte: u32,
    /// `primary` or `definition`.
    pub role: &'static str,
}

/// How `main` ended as the Node host's `runMain` reports it: the exit code, after writing a
/// failure or defect record to stderr.
pub fn report(host: &mut NativeHost, o: &Outcome) -> i32 {
    report_spans(host, o, &[])
}

/// [`report`], with a defect record's spans.
pub fn report_spans(host: &mut NativeHost, o: &Outcome, spans: &[Span]) -> i32 {
    match o {
        Outcome::Ok => 0,
        Outcome::Fail(shown) => {
            host.write_err(&format!("error: main failed with {shown}"));
            1
        }
        Outcome::Exit(code) => *code,
        Outcome::Cancelled | Outcome::Pending => 70,
        Outcome::Defect { code, message, .. } => {
            // Keys in the JS and WASM hosts' order, so the record's bytes match theirs.
            let text = |s: &str| serde_json::Value::from(s).to_string();
            let mut record = format!(
                "{{\"schema_version\":\"1.0\",\"kind\":\"defect\",\"code\":\"D{code:03}\",\"message\":{}",
                text(message)
            );
            if !spans.is_empty() {
                let spans: Vec<String> = spans
                    .iter()
                    .map(|s| {
                        format!(
                            "{{\"uri\":{},\"start_byte\":{},\"end_byte\":{},\"role\":{}}}",
                            text(&s.uri),
                            s.start_byte,
                            s.end_byte,
                            text(s.role)
                        )
                    })
                    .collect();
                record.push_str(&format!(",\"spans\":[{}]", spans.join(",")));
            }
            record.push('}');
            host.write_err(&record);
            70
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> NativeHost {
        NativeHost::new(Config::test(std::env::temp_dir())).unwrap()
    }

    #[test]
    fn handle_slots_are_reused_with_a_new_generation_and_retired_before_wrap() {
        let mut t: Table<u8> = Table::new(DIR);
        let a = t.insert(1);
        assert_eq!((a.slot, a.generation), (1, 1));
        t.retire(a);
        assert!(t.get(a).is_err(), "a stale handle names nothing");
        let b = t.insert(2);
        assert_eq!((b.slot, b.generation), (1, 2));
        assert!(matches!(t.get(a), Err(HostError::Fault(_))));
        assert_eq!(t.get(b).ok(), Some(&2));
        // A forged kind or slot is a fault, never a lookup elsewhere.
        assert!(t.get(HandleId { kind: 2, ..b }).is_err());
        assert!(t.get(HandleId { slot: 0, ..b }).is_err());
        assert!(t.get(HandleId { slot: 9, ..b }).is_err());
        // A retired-twice handle does not free the slot again.
        t.retire(b);
        t.retire(b);
        assert_eq!(t.free, vec![1]);
        // The last generation is never reused.
        t.slots[0].generation = u32::MAX - 1;
        let c = t.insert(3);
        assert_eq!(c.generation, u32::MAX);
        t.retire(c);
        assert!(t.free.is_empty());
        assert_eq!(t.insert(4).slot, 2);
    }

    #[test]
    fn random_stays_in_range_and_uuids_are_canonical_v4() {
        let mut h = host();
        for _ in 0..1000 {
            assert!((-3..=3).contains(&h.random_int(-3, 3)));
            let f = h.random_float();
            assert!((0.0..1.0).contains(&f));
        }
        assert_eq!(h.random_int(7, 7), 7);
        let _ = h.random_int(i64::MIN, i64::MAX);
        assert_eq!(h.random_bytes(33).len(), 33);
        let u = h.random_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert!("89ab".contains(&u[19..20]));
    }

    #[test]
    fn env_is_limited_by_the_manifest() {
        let mut h = host();
        // `PATH` is set wherever the tests run.
        assert!(h.env_get("PATH").unwrap().is_some());
        h.config.permissions = Some(Permissions::parse(r#"{"env": ["HOME"]}"#).unwrap());
        assert_eq!(h.env_get("PATH"), Ok(None));
        let p = Permissions::parse(r#"{"fs": {"read": ["a"]}}"#).unwrap();
        assert_eq!(
            p.env,
            Some(vec![]),
            "an absent key is denied (design §17.304)"
        );
        assert_eq!(p.fs.unwrap().write, Vec::<PathBuf>::new());
        h.config.permissions = Some(Permissions::parse("{}").unwrap());
        assert_eq!(h.env_get("PATH"), Ok(None));
        for bad in [
            r#"{"net": "x-secret"}"#,
            r#"{"env": [1]}"#,
            r#"{"fs": []}"#,
            r#"["x-secret"]"#,
            "x-secret",
        ] {
            let e = Permissions::parse(bad).unwrap_err();
            assert!(e.starts_with("SAKURA_PERMISSIONS must be"), "{bad}: {e}");
            assert!(!e.contains("x-secret"), "the error never carries the value");
        }
    }

    #[test]
    fn stdout_is_buffered_and_flushed_before_stderr() {
        let mut h = host();
        h.console_print("one");
        h.log_write("err");
        h.console_print("two");
        assert_eq!(h.captured(), ("one\ntwo\n", "err\n"));
    }

    #[test]
    fn a_cancelled_statement_is_interrupted_or_skipped_and_committed_writes_stay() {
        let dir = std::env::temp_dir().join(format!("sakura-sql-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = NativeHost::new(Config::test(dir.clone())).unwrap();
        let root = h.fs_root().unwrap();
        let db = h.sql_open(Some("t.db")).unwrap();
        assert!(h.sql_needs_root(db));
        assert!(matches!(
            h.sql_root(
                db,
                HandleId {
                    generation: 9,
                    ..root
                }
            ),
            Err(HostError::Fault(_))
        ));
        h.sql_root(db, root).unwrap();
        assert!(!h.sql_needs_root(db));
        let int = |n: i64| [&[sakura_rt::vt::t::INT][..], &n.to_le_bytes()].concat();
        h.sql_run(1, db, "create table t (x integer)", &[], false);
        assert_eq!(h.next_completion(), Some((1, 0, int(0))));
        h.sql_run(
            2,
            db,
            "insert into t values (?)",
            &[SqlValue::Int(7)],
            false,
        );
        assert_eq!(h.next_completion(), Some((2, 0, int(1))));
        // A statement that would run for minutes, and a write queued behind it.
        let forever = "with recursive c(i) as (select 1 union all select i + 1 from c \
                       where i < 1000000000) select count(*) from c";
        let started = Instant::now();
        h.sql_run(3, db, forever, &[], true);
        h.sql_run(4, db, "insert into t values (8)", &[], false);
        std::thread::sleep(Duration::from_millis(50));
        h.abort(4);
        h.abort(3);
        h.sql_run(5, db, "select count(*) from t", &[], true);
        let (req, status, rows) = h.next_completion().unwrap();
        assert_eq!((req, status), (5, 0), "the aborted requests never complete");
        assert!(
            rows.ends_with(&int(1)),
            "the queued write was skipped, the committed one kept"
        );
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "the statement was interrupted"
        );
        h.close();
        assert!(dir.join("t.db").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_statement_inside_a_transaction_is_never_interrupted() {
        let mut h = host();
        let db = h.sql_open(None).unwrap();
        let int = |n: i64| [&[sakura_rt::vt::t::INT][..], &n.to_le_bytes()].concat();
        h.sql_run(1, db, "create table t (x integer)", &[], false);
        assert_eq!(h.next_completion().map(|c| c.0), Some(1));
        let token = h.sql_begin(db).unwrap();
        h.sql_run(2, db, "insert into t values (0)", &[], false);
        assert_eq!(h.next_completion(), Some((2, 0, int(1))));
        // Interrupting this write would roll the whole transaction back (row 0 with it).
        let many = "insert into t with recursive c(i) as (select 1 union all select i + 1 \
                    from c where i < 200000) select i from c";
        h.sql_run(3, db, many, &[], false);
        std::thread::sleep(Duration::from_millis(5));
        h.abort(3);
        h.sql_end(db, token, true).unwrap();
        h.sql_run(4, db, "select count(*) from t", &[], true);
        let (req, _, rows) = h.next_completion().unwrap();
        assert_eq!(req, 4);
        assert!(
            rows.ends_with(&int(200_001)),
            "the write ran to its end inside the transaction"
        );
    }

    /// `Http.host()` against a local server (no outside network): a send with its secret header
    /// on the wire, a streamed body read in chunks, the size bound, and the `net` refusal.
    #[test]
    fn http_sends_streams_bounds_and_refuses_on_a_local_server() {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = vec![];
            for _ in 0..3 {
                let (s, _) = listener.accept().unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                seen.push(head);
                let body = "hello world";
                let mut s = s;
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nx-b: 2\r\nx-a: 1\r\nx-a: 3\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
            seen
        });
        let mut h = host();
        let url = format!("http://127.0.0.1:{port}/p");
        let req = HttpRequest {
            method: "GET",
            url: &url,
            headers: vec![("accept", "text/plain")],
            secret_headers: vec![("authorization", "Bearer tok")],
            ..Default::default()
        };
        h.http_send(1, &req);
        let (r, st, payload) = h.next_completion().unwrap();
        assert_eq!((r, st), (1, 0));
        let text = String::from_utf8_lossy(&payload);
        assert!(
            text.contains("x-a") && text.ends_with("hello world"),
            "{text}"
        );
        let a = text.find("1, 3").unwrap();
        assert!(
            a < text.find("x-b").unwrap(),
            "sorted, one name joined: {text}"
        );
        h.http_stream_open(2, &req);
        let (r, st, payload) = h.next_completion().unwrap();
        assert_eq!((r, st), (2, 0));
        let w = |i: usize| u32::from_le_bytes(payload[i..i + 4].try_into().unwrap());
        let n = payload.len();
        let body = HandleId {
            kind: w(n - 12),
            slot: w(n - 8),
            generation: w(n - 4),
        };
        assert!(h.http_body_waits(body));
        h.http_body_fetch(3, body, 5);
        assert_eq!(h.next_completion(), Some((3, 0, b"hello".to_vec())));
        assert!(
            !h.http_body_waits(body),
            "the rest of the chunk is buffered"
        );
        assert_eq!(h.http_body_read(body, 100), Ok(b" world".to_vec()));
        h.http_body_close(body);
        assert!(h.http_body_read(body, 1).is_err());
        h.config.max_copy_bytes = 4;
        h.http_send(4, &req);
        assert_eq!(
            h.next_completion(),
            Some((4, 1, b"response too large".to_vec()))
        );
        let seen = server.join().unwrap();
        assert!(seen[0].to_lowercase().contains("authorization: bearer tok"));
        h.config.permissions = Some(Permissions::parse(r#"{"net": ["a.example:443"]}"#).unwrap());
        assert_eq!(
            h.http_refused(&req),
            Some(format!(
                "127.0.0.1:{port} is not in the manifest's net list"
            ))
        );
        assert_eq!(h.close().aborted_requests, 0);
    }

    #[test]
    fn sleeps_complete_and_aborted_ones_never_do() {
        let mut h = host();
        h.clock_sleep_ms(7, 5);
        h.clock_sleep_ms(8, 0);
        h.abort(7);
        assert_eq!(h.next_completion(), Some((8, 0, vec![])));
        assert_eq!(h.next_completion(), None);
        h.clock_sleep_ms(9, 60_000);
        let d = h.fs_memory_root(vec![("f".into(), Some(vec![1]))]).unwrap();
        let f = h.file_open(d, "f", FileMode::Read).unwrap();
        assert_eq!(h.file_read(f, 9), Ok(vec![1]));
        assert_eq!(
            h.close(),
            Ledger {
                aborted_requests: 1,
                dropped_completions: 0,
                closed_handles: 1,
                closed_files: 1,
            }
        );
        assert_eq!(h.resources(), (0, 0));
        assert!(matches!(h.fs_read(d, "f"), Err(HostError::Fault(_))));
    }
}
