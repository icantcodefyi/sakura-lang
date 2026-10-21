//! Live workers and topics natively (`std/worker`, RFC 0002 §2.9, plan `web-apis.md` phase
//! 11b, ABI 1.15.0), the twin of the JS runtime's `worker_*` functions. A worker is a second
//! [`sakura_rt::native::Instance`] of the program on its own OS thread, which the driver's
//! [`Launcher`] starts at the entry the module's worker table names, with a [`NativeHost`]
//! whose permissions are exactly the worker's grants and whose heap bound is its parent's. A
//! link is a pair of bounded `mpsc` channels of the `Schema` codec's text, one per direction,
//! holding `capacity` messages: `Worker.close` drops this side's sender, an end is recorded in
//! the link before the worker's side is dropped. Topics reach the process's other instances
//! through one registry of their signal channels.
//!
//! ponytail: a stopped worker's thread is told to stop and its output is dropped, but a
//! thread that never waits again (a loop without a request) runs until the process exits;
//! Node's `terminate` interrupts it. Preempting compiled code needs a safepoint poll.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use sakura_rt::exec::outcome;
use sakura_rt::native::{Outcome, WorkerGrants};
use sakura_rt::vt::t;
use tokio::sync::{mpsc, watch};

use crate::{Config, Inflight, NativeHost, Permissions, fs};

/// At most this many live workers per instance (`std/worker`'s `Limit`).
const WORKER_LIMIT: usize = 16;

/// One `@worker` function of the module's worker table (`sakura_lir::image::workers_table`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerFn {
    /// The code its function values hold (`worker_start`'s `code`).
    pub fun: u32,
    /// The code a worker's task starts at.
    pub entry: u32,
    pub key: String,
    /// The std effects the entry declares.
    pub effects: Vec<String>,
}

/// The worker table's bytes, or `None` when they are malformed.
pub fn parse_table(mut b: &[u8]) -> Option<Vec<WorkerFn>> {
    fn take<'a>(b: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        let (head, rest) = b.split_at_checked(n)?;
        *b = rest;
        Some(head)
    }
    fn u32_(b: &mut &[u8]) -> Option<u32> {
        Some(u32::from_le_bytes(take(b, 4)?.try_into().ok()?))
    }
    fn text(b: &mut &[u8]) -> Option<String> {
        let n = u32_(b)? as usize;
        String::from_utf8(take(b, n)?.to_vec()).ok()
    }
    let mut out = vec![];
    for _ in 0..u32_(&mut b)? {
        let (fun, entry) = (u32_(&mut b)?, u32_(&mut b)?);
        let key = text(&mut b)?;
        let effects = text(&mut b)?;
        let effects = effects
            .split('\n')
            .filter(|e| !e.is_empty())
            .map(String::from)
            .collect();
        out.push(WorkerFn {
            fun,
            entry,
            key,
            effects,
        });
    }
    b.is_empty().then_some(out)
}

/// What a new worker's thread runs, for the [`Launcher`].
pub struct Job {
    pub worker: WorkerFn,
    /// The worker's host: its grants as its permissions, no trace.
    pub config: Config,
    /// Its link to the parent, for [`NativeHost::attach`].
    pub parent: Parent,
}

/// Starts a worker's thread (the driver builds the instance there). A thread that does not
/// start drops its [`Parent`], which ends the worker on `D090`.
pub type Launcher = Arc<dyn Fn(Job) + Send + Sync>;

/// A wakeup from another thread, between runs.
pub(crate) enum Signal {
    Topic(String, String),
    Stop,
}

/// What both sides of a link share.
#[derive(Default)]
pub(crate) struct Shared {
    /// How the worker's entry ended: `(1, "")` finished, `(2, code)` failed.
    end: Mutex<Option<(i64, String)>>,
    /// The parent stopped the worker: its output is dropped.
    stopped: AtomicBool,
}

/// One side of a link.
pub(crate) struct Side {
    tx: Option<mpsc::Sender<String>>,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<String>>>,
    shared: Arc<Shared>,
    /// The parent's side: the worker's signals, and a receiver that closes when its thread
    /// has exited.
    child: Option<(mpsc::UnboundedSender<Signal>, watch::Receiver<()>)>,
}

impl Drop for Side {
    fn drop(&mut self) {
        if self.child.is_none() {
            // The worker's side goes when its instance does: a worker that did not report an
            // end did not finish.
            let mut end = self.shared.end.lock().unwrap_or_else(|e| e.into_inner());
            end.get_or_insert((2, "D090".into()));
        }
    }
}

/// A worker's link to its parent, moved to the worker's thread.
pub struct Parent {
    side: Side,
    signals: (
        mpsc::UnboundedSender<Signal>,
        mpsc::UnboundedReceiver<Signal>,
    ),
    exit: watch::Sender<()>,
}

/// The topic registry: every instance listening to a name, by instance id.
type Listener = (String, u64, mpsc::UnboundedSender<Signal>);
static TOPICS: Mutex<Vec<Listener>> = Mutex::new(vec![]);
static INSTANCES: AtomicU64 = AtomicU64::new(1);

pub(crate) fn instance_id() -> u64 {
    INSTANCES.fetch_add(1, Ordering::Relaxed)
}

fn topics() -> std::sync::MutexGuard<'static, Vec<Listener>> {
    TOPICS.lock().unwrap_or_else(|e| e.into_inner())
}

/// What [`NativeHost::wait`] woke on.
pub enum Wake {
    /// A request completed: request, status, payload.
    Done(u32, u32, Vec<u8>),
    /// A message published on a topic elsewhere: name, text.
    Topic(String, String),
    /// The parent stopped this worker.
    Stop,
    /// Nothing is in flight and nothing links this instance to another: every task waits.
    Idle,
}

fn vt_int(n: i64) -> Vec<u8> {
    let mut b = vec![t::INT];
    b.extend(n.to_le_bytes());
    b
}

/// The `sakura-vt1` tuple `(kind, text)` `worker_take` completes with.
fn vt_taken(kind: i64, text: &str) -> Vec<u8> {
    let mut b = vec![t::TUPLE];
    b.extend(2u32.to_le_bytes());
    b.extend(vt_int(kind));
    b.push(t::STRING);
    b.extend((text.len() as u32).to_le_bytes());
    b.extend(text.as_bytes());
    b
}

/// Whether every entry of `xs` is in `own` (`None`: no limit).
fn within<T: PartialEq>(xs: &[T], own: Option<&[T]>) -> bool {
    own.is_none_or(|own| xs.iter().all(|x| own.contains(x)))
}

/// `sakura run`'s refusal of `main` (design §6.2), for a worker's declared `effects`.
fn denied(effects: &[String], g: &WorkerGrants) -> bool {
    const NET: &[&str] = &["Http", "Serve", "Sql", "WebSocket"];
    effects.iter().any(|e| {
        (e == "Fs" && g.fs_read.is_empty()) || (NET.contains(&e.as_str()) && g.net.is_empty())
    })
}

impl NativeHost {
    /// Lets this instance start live workers of `table`, each through `launcher`.
    pub fn set_workers(&mut self, table: Vec<WorkerFn>, launcher: Launcher) {
        self.worker_fns = table;
        self.launcher = Some(launcher);
    }

    /// Makes this instance a worker: `parent` is its link 0.
    pub fn attach(&mut self, parent: Parent) {
        let Parent {
            side,
            signals,
            exit,
        } = parent;
        self.stopped = Some(side.shared.clone());
        (self.signal_tx, self.signals) = signals;
        self.links.insert(0, side);
        self._exit = Some(exit);
    }

    /// Whether the parent stopped this worker, whose output is then dropped.
    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped
            .as_ref()
            .is_some_and(|s| s.stopped.load(Ordering::Relaxed))
    }

    /// A worker's end, told to its parent (the JS runtime's `workerChild`): the one plain
    /// stderr line of a failure or defect, then the end recorded for its `take`.
    pub fn report_end(&mut self, key: &str, o: &Outcome) {
        let end = match o {
            Outcome::Ok => (1, String::new()),
            Outcome::Exit(code) => (2, format!("exit {code}")),
            Outcome::Fail(shown) => {
                self.write_err(&format!("error: worker {key} failed with {shown}"));
                (2, "fail".into())
            }
            Outcome::Defect { code, message, .. } => {
                let code = format!("D{code:03}");
                self.write_err(&format!("error: worker {key} ended on {code}: {message}"));
                (2, code)
            }
            Outcome::Cancelled | Outcome::Pending => (2, "D090".into()),
        };
        self.flush_out();
        if let Some(s) = &self.stopped {
            let mut e = s.end.lock().unwrap_or_else(|e| e.into_inner());
            e.get_or_insert(end);
        }
        // Senders go after the end, so the parent reads every message first.
        self.links.clear();
    }

    /// Wait for the next thing that lets a task run (the JS host's event loop): a completion,
    /// a topic message or a stop; [`Wake::Idle`] when nothing is in flight and no link keeps
    /// the instance alive.
    pub fn wait(&mut self) -> Wake {
        loop {
            if self.inflight.is_empty() && self.links.is_empty() {
                while self.rx.try_recv().is_ok() {}
                return Wake::Idle;
            }
            let (rx, signals) = (&mut self.rx, &mut self.signals);
            let woke = self.executor.block_on(async {
                tokio::select! {
                    c = rx.recv() => c.map(|(r, s, p)| Wake::Done(r, s, p)),
                    s = signals.recv() => s.map(|s| match s {
                        Signal::Topic(n, t) => Wake::Topic(n, t),
                        Signal::Stop => Wake::Stop,
                    }),
                }
            });
            match woke {
                Some(Wake::Done(r, s, p)) => {
                    if self.inflight.remove(&r).is_some() {
                        return Wake::Done(r, s, p);
                    }
                }
                Some(w) => return w,
                // `self` holds a sender of both.
                None => return Wake::Idle,
            }
        }
    }

    /// Complete `request` with `payload` from the executor.
    fn async_op(&mut self, request: u32, op: impl Future<Output = Vec<u8>> + Send + 'static) {
        let tx = self.tx.clone();
        let task = self.executor.spawn(async move {
            let payload = op.await;
            let _ = tx.send((request, outcome::OK, payload));
        });
        self.inflight
            .insert(request, Inflight::Task(task.abort_handle()));
    }

    pub(crate) fn start_worker(&mut self, code: u32, g: WorkerGrants, capacity: i64) -> i64 {
        let Some(launcher) = self.launcher.clone() else {
            return -4;
        };
        let Some(worker) = self.worker_fns.iter().find(|w| w.fun == code).cloned() else {
            return -3;
        };
        if self.links.keys().filter(|&&id| id != 0).count() >= WORKER_LIMIT {
            return -2;
        }
        // The grants must be within this instance's own table (`None`: no limits), entry by
        // entry with exact string equality, as the JS host compares them.
        if let Some(own) = &self.config.permissions {
            let dirs = |own: Option<&Vec<std::path::PathBuf>>| {
                own.map(|d| {
                    d.iter()
                        .filter_map(|p| p.to_str())
                        .map(String::from)
                        .collect::<Vec<_>>()
                })
            };
            let fs = own.fs.as_ref();
            if !(within(&g.fs_read, dirs(fs.map(|f| &f.read)).as_deref())
                && within(&g.fs_write, dirs(fs.map(|f| &f.write)).as_deref())
                && within(&g.net, own.net.as_deref())
                && within(&g.env, own.env.as_deref()))
            {
                return -1;
            }
        }
        if denied(&worker.effects, &g) {
            return -1;
        }
        let cap = usize::try_from(capacity).unwrap_or(1).max(1);
        let (to_tx, to_rx) = mpsc::channel(cap);
        let (from_tx, from_rx) = mpsc::channel(cap);
        let (sig_tx, sig_rx) = mpsc::unbounded_channel();
        let (exit_tx, exit_rx) = watch::channel(());
        let shared = Arc::new(Shared::default());
        let parent = Parent {
            side: Side {
                tx: Some(from_tx),
                rx: Arc::new(tokio::sync::Mutex::new(to_rx)),
                shared: shared.clone(),
                child: None,
            },
            signals: (sig_tx.clone(), sig_rx),
            exit: exit_tx,
        };
        let config = Config {
            permissions: Some(Permissions {
                fs: Some(fs::Grants {
                    read: g.fs_read.iter().map(Into::into).collect(),
                    write: g.fs_write.iter().map(Into::into).collect(),
                }),
                net: Some(g.net),
                env: Some(g.env),
            }),
            trace: None,
            ..self.config.clone()
        };
        let id = self.next_worker;
        self.next_worker += 1;
        self.links.insert(
            id,
            Side {
                tx: Some(to_tx),
                rx: Arc::new(tokio::sync::Mutex::new(from_rx)),
                shared,
                child: Some((sig_tx, exit_rx)),
            },
        );
        launcher(Job {
            worker,
            config,
            parent,
        });
        id
    }

    pub(crate) fn post(&mut self, request: u32, id: i64, text: &str) {
        let tx = self.links.get(&id).and_then(|l| {
            let ended = l.shared.end.lock().is_ok_and(|e| e.is_some()) && l.child.is_some();
            l.tx.clone().filter(|_| !ended)
        });
        let text = text.to_string();
        self.async_op(request, async move {
            let sent = match tx {
                Some(tx) => tx.send(text).await.is_ok(),
                None => false,
            };
            vt_int(if sent { 0 } else { 2 })
        });
    }

    pub(crate) fn take(&mut self, request: u32, id: i64) {
        let link = self
            .links
            .get(&id)
            .map(|l| (l.rx.clone(), l.shared.clone()));
        let child = self.links.get(&id).is_some_and(|l| l.child.is_some());
        self.async_op(request, async move {
            let Some((rx, shared)) = link else {
                return vt_taken(1, "");
            };
            if let Some(text) = rx.lock().await.recv().await {
                return vt_taken(0, &text);
            }
            // The other side's sender is gone: its end, or it shut its direction.
            let end = if child {
                shared.end.lock().ok().and_then(|e| e.clone())
            } else {
                None
            };
            let (kind, code) = end.unwrap_or((1, String::new()));
            vt_taken(kind, &code)
        });
    }

    pub(crate) fn join(&mut self, request: u32, id: i64) {
        let exit = self.exits.remove(&id).or_else(|| {
            self.links
                .get(&id)
                .and_then(|l| l.child.as_ref())
                .map(|c| c.1.clone())
        });
        self.async_op(request, async move {
            if let Some(mut exit) = exit {
                // Never sent: it closes when the worker's thread drops its instance.
                while exit.changed().await.is_ok() {}
            }
            vec![]
        });
    }

    pub(crate) fn stop(&mut self, id: i64) {
        if id == 0 {
            return;
        }
        if let Some(l) = self.links.remove(&id) {
            l.shared.stopped.store(true, Ordering::Relaxed);
            if let Some((signals, exit)) = &l.child {
                let _ = signals.send(Signal::Stop);
                self.exits.insert(id, exit.clone());
            }
        }
    }

    pub(crate) fn shut(&mut self, id: i64) {
        if let Some(l) = self.links.get_mut(&id) {
            l.tx = None;
        }
    }

    pub(crate) fn listen(&mut self, name: &str) {
        let mut topics = topics();
        if !topics
            .iter()
            .any(|(n, i, _)| n == name && *i == self.instance)
        {
            topics.push((name.to_string(), self.instance, self.signal_tx.clone()));
        }
    }

    pub(crate) fn publish(&mut self, name: &str, text: &str) {
        for (n, i, tx) in topics().iter() {
            if n == name && *i != self.instance {
                let _ = tx.send(Signal::Topic(name.to_string(), text.to_string()));
            }
        }
    }

    pub(crate) fn unlisten(&mut self) {
        topics().retain(|(_, i, _)| *i != self.instance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worker_table_round_trips_and_refuses_trailing_bytes() {
        let mut b = 1u32.to_le_bytes().to_vec();
        b.extend(7u32.to_le_bytes());
        b.extend(9u32.to_le_bytes());
        for s in ["main.summer", "Fs\nConsole"] {
            b.extend((s.len() as u32).to_le_bytes());
            b.extend(s.as_bytes());
        }
        let w = parse_table(&b).unwrap();
        assert_eq!(w[0].fun, 7);
        assert_eq!(w[0].entry, 9);
        assert_eq!(w[0].effects, ["Fs", "Console"]);
        b.push(0);
        assert_eq!(parse_table(&b), None);
    }

    #[test]
    fn grants_beyond_the_parents_table_or_an_unpermitted_effect_are_refused() {
        let mut h = NativeHost::new(Config {
            permissions: Some(
                Permissions::parse(r#"{"fs":{"read":["data"]},"env":["A"]}"#).unwrap(),
            ),
            ..Config::test(".".into())
        })
        .unwrap();
        let fun = |effects: &[&str]| WorkerFn {
            fun: 3,
            entry: 4,
            key: "w".into(),
            effects: effects.iter().map(|e| e.to_string()).collect(),
        };
        let started = Arc::new(Mutex::new(vec![]));
        let s = started.clone();
        h.set_workers(
            vec![fun(&["Fs"])],
            Arc::new(move |j: Job| s.lock().unwrap().push(j.config.permissions)),
        );
        let g = |read: &[&str], env: &[&str]| WorkerGrants {
            fs_read: read.iter().map(|s| s.to_string()).collect(),
            env: env.iter().map(|s| s.to_string()).collect(),
            ..WorkerGrants::default()
        };
        assert_eq!(h.start_worker(3, g(&["other"], &[]), 64), -1);
        assert_eq!(h.start_worker(3, g(&["data"], &["B"]), 64), -1);
        // `Fs` needs an `fs.read` grant.
        assert_eq!(h.start_worker(3, g(&[], &["A"]), 64), -1);
        assert_eq!(h.start_worker(5, g(&["data"], &[]), 64), -3);
        assert_eq!(h.start_worker(3, g(&["data"], &["A"]), 64), 1);
        let p = started.lock().unwrap()[0].clone().unwrap();
        assert_eq!(p.fs.unwrap().read, [std::path::PathBuf::from("data")]);
        assert_eq!(p.env.unwrap(), ["A"]);
        assert_eq!(p.net.unwrap(), Vec::<String>::new());
    }
}
