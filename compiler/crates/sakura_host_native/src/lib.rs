//! The native host (design §17.170 (1), §17.171; ABI §1b, §8.4–§8.6): one instance's embedder
//! for the runtime's [`sakura_rt::native::Instance`]. It supplies the root handlers' imports
//! (Console, Clock, Random, `log_write`, traces) and the `hosted` services of N1a (Env, Fs and
//! `file_*`) with the JS host's semantics, keeps the instance's handle and file tables, and
//! drives `async` requests on a current-thread Tokio runtime owned by the instance's thread.
//! The standard services of N1b (Http, Sql, serve, Llm, `cli_*`) answer `D091` through the
//! `Host` defaults until then.
//!
//! Everything here is safe Rust (the workspace forbids `unsafe_code`): the host never receives
//! the instance context, and its inputs and answers are copied by the runtime.

#![cfg(feature = "native")]

pub mod fs;

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sakura_rt::exec::outcome;
use sakura_rt::native::{FileMode, HandleId, Host, HostError, Hosted, Instance, Outcome};
use sakura_rt::vt::DIR;
use tokio::sync::mpsc;

/// `[permissions]` of the manifest (`SAKURA_PERMISSIONS`): `None` lists are no limit.
#[derive(Clone, Debug, Default)]
pub struct Permissions {
    pub fs: Option<fs::Grants>,
    pub env: Option<Vec<String>>,
}

impl Permissions {
    /// `{"fs": {"read": […], "write": […]}, "net": […], "env": […]}` as the driver passes it. As
    /// JS, a set variable means `Fs.root()` is limited to the `fs` lists, empty when absent.
    pub fn parse(json: &str) -> Result<Permissions, String> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let strings = |v: Option<&serde_json::Value>| -> Vec<String> {
            v.and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        let fs = v.get("fs");
        Ok(Permissions {
            fs: Some(fs::Grants {
                read: strings(fs.and_then(|f| f.get("read")))
                    .into_iter()
                    .map(PathBuf::from)
                    .collect(),
                write: strings(fs.and_then(|f| f.get("write")))
                    .into_iter()
                    .map(PathBuf::from)
                    .collect(),
            }),
            env: v.get("env").map(|e| strings(Some(e))),
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
    /// `SAKURA_BLOCKING_THREADS` (design §17.171 (2)): the bounded pool for blocking I/O that
    /// answers through a request. N1a's Fs answers synchronously (`hosted`), so N1b's
    /// `Sql.sqlite` is its first user.
    pub blocking_threads: usize,
    /// `SAKURA_MAX_COPY_BYTES`, the instance's `Limits::max_copy_bytes`: a read stops one byte
    /// past it, so the runtime refuses the answer (`D091`) without the host holding the rest.
    pub max_copy_bytes: u64,
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let permissions = match std::env::var("SAKURA_PERMISSIONS") {
            Ok(json) => Some(Permissions::parse(&json)?),
            Err(_) => None,
        };
        let blocking_threads = match std::env::var("SAKURA_BLOCKING_THREADS") {
            Ok(n) => n
                .parse::<usize>()
                .ok()
                .filter(|&n| n > 0)
                .ok_or("SAKURA_BLOCKING_THREADS must be a positive integer")?,
            Err(_) => 4,
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
            blocking_threads,
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
            blocking_threads: 1,
            max_copy_bytes: MAX_COPY_BYTES,
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
    files: fs::Files,
    executor: tokio::runtime::Runtime,
    tx: mpsc::UnboundedSender<Completion>,
    rx: mpsc::UnboundedReceiver<Completion>,
    inflight: HashMap<u32, tokio::task::AbortHandle>,
}

impl NativeHost {
    pub fn new(config: Config) -> Result<NativeHost, String> {
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(config.blocking_threads)
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
            files: fs::Files::default(),
            executor,
            tx,
            rx,
            inflight: HashMap::new(),
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
            if self.inflight.remove(&c.0).is_some() {
                return Some(c);
            }
        }
    }

    /// The resource ledger (ABI §8.6, design §17.171 (4)): abort every request in flight, drop
    /// queued completions, retire every handle and close every file. The instance is gone or
    /// never entered again.
    pub fn close(&mut self) -> Ledger {
        let mut ledger = Ledger {
            aborted_requests: self.inflight.len(),
            closed_handles: self.dirs.live(),
            closed_files: self.files.len(),
            ..Ledger::default()
        };
        for (_, h) in self.inflight.drain() {
            h.abort();
        }
        while self.rx.try_recv().is_ok() {
            ledger.dropped_completions += 1;
        }
        self.dirs.clear();
        self.files.close_all();
        self.flush_out();
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

    fn clock_sleep_ms(&mut self, request: u32, ms: i64) {
        let tx = self.tx.clone();
        let task = self.executor.spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms.max(0) as u64)).await;
            let _ = tx.send((request, outcome::OK, vec![]));
        });
        self.inflight.insert(request, task.abort_handle());
    }

    fn abort(&mut self, request: u32) {
        if let Some(h) = self.inflight.remove(&request) {
            h.abort();
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
        if handle.kind == DIR {
            self.dirs.retire(handle);
        }
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

    fn fs_memory_root(&mut self, files: Vec<(String, Vec<u8>)>) -> Hosted<HandleId> {
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
}

/// Drive `task` to an outcome (the Node host's `runTask`): run until nothing can, then wait on
/// the executor for the next completion of a request in flight; with none in flight, every
/// task waits on another and the run ends `D090`.
pub fn run_task(inst: &mut Instance<NativeHost>, task: u32) -> Outcome {
    let mut kind = inst.run(task);
    while kind == outcome::PENDING {
        let host = inst.host();
        host.flush_out();
        let Some((req, status, payload)) = host.next_completion() else {
            return Outcome::Defect {
                code: 90,
                site: 0,
                message: "every task is waiting".into(),
            };
        };
        if let Err(d) = inst.complete(req, status, &payload) {
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
        assert_eq!(p.env, None);
        assert_eq!(p.fs.unwrap().write, Vec::<PathBuf>::new());
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
    fn sleeps_complete_and_aborted_ones_never_do() {
        let mut h = host();
        h.clock_sleep_ms(7, 5);
        h.clock_sleep_ms(8, 0);
        h.abort(7);
        assert_eq!(h.next_completion(), Some((8, 0, vec![])));
        assert_eq!(h.next_completion(), None);
        h.clock_sleep_ms(9, 60_000);
        let d = h.fs_memory_root(vec![("f".into(), vec![1])]).unwrap();
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
