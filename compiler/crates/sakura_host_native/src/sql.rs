//! `std::sql::Sql` (design §10.2, §17.171): each database is a `rusqlite` connection (SQLite
//! bundled) owned by its own worker thread, which runs the database's operations in order.
//! `query` and `execute` complete their request from the worker; a cancelled request is skipped
//! if it is still queued and interrupted (`sqlite3_interrupt`, and a progress handler for the
//! moment before its first step) if it is running outside a transaction, and a write that
//! committed stays committed (§17.171 (3)). Inside a transaction a running statement is left
//! to finish, as on JS: SQLite answers an interrupted write by rolling back the whole
//! transaction. `begin`, `release` and `rollback` wait for the worker, so they wait behind a
//! running statement. Savepoint names and tokens follow JS `sqlConnection`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, InterruptHandle};
use sakura_rt::native::SqlValue;
use sakura_rt::vt::t;
use tokio::sync::mpsc::UnboundedSender;

use sakura_rt::exec::status;

use crate::Completion;

type Job = Box<dyn FnOnce(&mut Option<Connection>) + Send>;

/// What a request's cancellation reaches: the request the worker runs now (and whether it may
/// be interrupted), the ones queued, and those of them cancelled before they started.
#[derive(Default)]
struct Running {
    now: u32,
    interruptible: bool,
    /// Set when `now` is cancelled; the progress handler stops the statement.
    stop: bool,
    queued: HashSet<u32>,
    cancelled: HashSet<u32>,
    interrupt: Option<InterruptHandle>,
}

pub struct Db {
    jobs: mpsc::Sender<Job>,
    running: Arc<Mutex<Running>>,
    /// `Sql.sqlite(path)` until it opens: the path inside `Fs.root()`'s directory.
    pending_file: Option<String>,
    depth: u32,
    next: i64,
    live: HashMap<i64, String>,
}

/// An owned bound parameter.
enum Param {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl rusqlite::ToSql for Param {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value};
        Ok(ToSqlOutput::Borrowed(match self {
            Param::Null => ValueRef::Null,
            Param::Int(i) => return Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            Param::Float(f) => return Ok(ToSqlOutput::Owned(Value::Real(*f))),
            Param::Text(s) => ValueRef::Text(s.as_bytes()),
            Param::Blob(b) => ValueRef::Blob(b),
        }))
    }
}

/// SQLite's own message for a failure, as `node:sqlite` reports it.
fn message(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m))
        | rusqlite::Error::SqlInputError { msg: m, .. } => m,
        e => e.to_string(),
    }
}

impl Db {
    /// A database on a new worker: in memory (`file` `None`), or a file opened at `root`.
    pub fn new(file: Option<&str>, workers: &mut Vec<JoinHandle<()>>) -> Result<Db, String> {
        let (jobs, rx) = mpsc::channel::<Job>();
        let worker = std::thread::Builder::new()
            .name("sakura-sql".into())
            .spawn(move || {
                let mut conn = None;
                for job in rx {
                    job(&mut conn);
                }
            })
            .map_err(|e| e.to_string())?;
        workers.push(worker);
        let db = Db {
            jobs,
            running: Arc::default(),
            pending_file: file.map(String::from),
            depth: 0,
            next: 1,
            live: HashMap::new(),
        };
        if file.is_none() {
            db.open(None)?;
        }
        Ok(db)
    }

    pub fn needs_root(&self) -> bool {
        self.pending_file.is_some()
    }

    pub fn file(&self) -> Option<&str> {
        self.pending_file.as_deref()
    }

    /// Open the connection on the worker: the file at `path`, or in memory.
    pub fn open(&self, path: Option<PathBuf>) -> Result<(), String> {
        let running = self.running.clone();
        self.wait(move |conn| {
            let c = match path {
                Some(p) => Connection::open(p),
                None => Connection::open_in_memory(),
            }
            .map_err(message)?;
            // `node:sqlite`'s default: a locked database fails at once.
            c.busy_timeout(std::time::Duration::ZERO).map_err(message)?;
            let stop = running.clone();
            c.progress_handler(1000, Some(move || stop.lock().is_ok_and(|r| r.stop)))
                .map_err(message)?;
            running.lock().expect("sql state").interrupt = Some(c.get_interrupt_handle());
            *conn = Some(c);
            Ok(())
        })
    }

    pub fn opened(&mut self) {
        self.pending_file = None;
    }

    /// Run `f` on the worker and wait for its answer.
    fn wait<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Option<Connection>) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        let job: Job = Box::new(move |conn| {
            let _ = tx.send(f(conn));
        });
        self.jobs
            .send(job)
            .map_err(|_| "the database is closed".to_string())?;
        rx.recv()
            .unwrap_or_else(|_| Err("the database is closed".into()))
    }

    /// `query` (`rows`) or `execute`: complete `request` from the worker. An answer past
    /// `max_copy` bytes is a host failure (`D091`), the read stopped there.
    pub fn run(
        &self,
        request: u32,
        text: &str,
        params: &[SqlValue<'_>],
        rows: bool,
        max_copy: u64,
        tx: UnboundedSender<Completion>,
    ) {
        let text = text.to_string();
        let params: Vec<Param> = params
            .iter()
            .map(|p| match *p {
                SqlValue::Null => Param::Null,
                SqlValue::Int(i) => Param::Int(i),
                SqlValue::Float(f) => Param::Float(f),
                SqlValue::Text(s) => Param::Text(s.into()),
                SqlValue::Blob(b) => Param::Blob(b.into()),
            })
            .collect();
        let running = self.running.clone();
        running.lock().expect("sql state").queued.insert(request);
        let closed = tx.clone();
        let job: Job = Box::new(move |conn| {
            {
                let mut r = running.lock().expect("sql state");
                r.queued.remove(&request);
                if r.cancelled.remove(&request) {
                    return;
                }
                r.now = request;
                r.stop = false;
                r.interruptible = conn.as_ref().is_some_and(|c| c.is_autocommit());
            }
            let answer = match conn.as_mut() {
                None => Err("the database is not open".to_string()),
                Some(c) => statement(c, &text, &params, rows, max_copy),
            };
            {
                let mut r = running.lock().expect("sql state");
                r.now = 0;
                r.stop = false;
            }
            let _ = tx.send(match answer {
                Ok(Ok(bytes)) => (request, status::OK, bytes),
                Ok(Err(too_large)) => (request, status::HOST_FAILURE, too_large.into_bytes()),
                Err(m) => (request, status::ERR, m.into_bytes()),
            });
        });
        if self.jobs.send(job).is_err() {
            let _ = closed.send((request, status::ERR, b"the database is closed".to_vec()));
        }
    }

    /// Cancel `request`: skip it if it is still queued, interrupt it if it is running outside a
    /// transaction (module doc); a finished one is left alone.
    pub fn abort(&self, request: u32) {
        let mut r = self.running.lock().expect("sql state");
        if r.queued.contains(&request) {
            r.cancelled.insert(request);
        } else if r.now == request && r.interruptible {
            r.stop = true;
            if let Some(h) = &r.interrupt {
                h.interrupt();
            }
        }
    }

    /// `SAVEPOINT sakura_tx<depth>`: its token.
    pub fn begin(&mut self) -> Result<i64, String> {
        let name = format!("sakura_tx{}", self.depth);
        let sql = format!("SAVEPOINT {name}");
        self.wait(move |conn| exec(conn, &sql))?;
        self.depth += 1;
        let token = self.next;
        self.next += 1;
        self.live.insert(token, name);
        Ok(token)
    }

    /// End savepoint `token` (released, or rolled back first); a second end does nothing. A
    /// release that fails (the database is busy) is the host's failure: the transaction would
    /// stay open with every later write in it.
    pub fn end(&mut self, token: i64, commit: bool) -> Result<(), String> {
        let Some(name) = self.live.remove(&token) else {
            return Ok(());
        };
        self.depth -= 1;
        let sql = if commit {
            format!("RELEASE {name}")
        } else {
            format!("ROLLBACK TO {name}; RELEASE {name}")
        };
        self.wait(move |conn| exec(conn, &sql))
    }
}

fn exec(conn: &mut Option<Connection>, sql: &str) -> Result<(), String> {
    conn.as_mut()
        .ok_or("the database is not open")?
        .execute_batch(sql)
        .map_err(message)
}

/// One statement: `Ok(Ok(answer))`, `Ok(Err(why))` past the copy budget, or SQLite's message.
/// A query's answer is the `sakura-vt1` tuple of its column names and its rows' cell tuples
/// (`Unit` for NULL); an execute's is the `Int` count of rows changed.
fn statement(
    c: &mut Connection,
    text: &str,
    params: &[Param],
    rows: bool,
    max_copy: u64,
) -> Result<Result<Vec<u8>, String>, String> {
    let mut st = c.prepare(text).map_err(message)?;
    if params.len() > st.parameter_count() {
        return Err("column index out of range".into());
    }
    for (i, p) in params.iter().enumerate() {
        st.raw_bind_parameter(i + 1, p).map_err(message)?;
    }
    let names: Vec<String> = st.column_names().into_iter().map(String::from).collect();
    let mut out = Vec::new();
    let put_len = |out: &mut Vec<u8>, tag: u8, b: &[u8]| {
        out.push(tag);
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    };
    if rows {
        out.push(t::TUPLE);
        out.extend_from_slice(&2u32.to_le_bytes());
        out.push(t::LIST);
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for n in &names {
            put_len(&mut out, t::STRING, n.as_bytes());
        }
        out.push(t::LIST);
    }
    let count_at = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    let mut count = 0u32;
    let mut q = st.raw_query();
    while let Some(row) = q.next().map_err(message)? {
        if !rows {
            // `execute` steps a statement that returns rows to its end, as `node:sqlite`'s `run`.
            continue;
        }
        out.push(t::TUPLE);
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for i in 0..names.len() {
            match row.get_ref(i).map_err(message)? {
                ValueRef::Null => out.push(t::UNIT),
                ValueRef::Integer(v) => {
                    out.push(t::INT);
                    out.extend_from_slice(&v.to_le_bytes());
                }
                ValueRef::Real(f) => {
                    out.push(t::FLOAT);
                    out.extend_from_slice(&f.to_bits().to_le_bytes());
                }
                // `node:sqlite` decodes TEXT as UTF-8 with replacement characters.
                ValueRef::Text(b) => {
                    put_len(&mut out, t::STRING, String::from_utf8_lossy(b).as_bytes())
                }
                ValueRef::Blob(b) => put_len(&mut out, t::BYTES, b),
            }
        }
        count += 1;
        if out.len() as u64 > max_copy {
            // The WASM host's `copyOut` text for an answer past the budget.
            return Ok(Err(format!(
                "response too large: {} bytes over SAKURA_MAX_COPY_BYTES",
                out.len()
            )));
        }
    }
    drop(q);
    if !rows {
        let mut out = vec![t::INT];
        out.extend_from_slice(&(c.changes() as i64).to_le_bytes());
        return Ok(Ok(out));
    }
    out[count_at..count_at + 4].copy_from_slice(&count.to_le_bytes());
    Ok(Ok(out))
}
