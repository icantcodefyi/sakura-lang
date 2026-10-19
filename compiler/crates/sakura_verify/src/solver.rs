//! The solver worker (design §17.85, §17.98): one fresh process per query over SMT-LIB2, pinned
//! versions and options, fixed seed, per-goal `timeout` and `rlimit`, an external deadline, an
//! aggregate budget, a memory limit (Z3) and an output cap. Nothing is downloaded: the binary is
//! the one given or the first on `PATH`, and a version other than the pinned one is refused.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::record::{Answer, Model, Reason, ReasonKind, Record, SolverInfo, decide};
use crate::vc::{DefVc, Obligation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverKind {
    Z3,
    Cvc5,
}

impl SolverKind {
    pub fn binary(self) -> &'static str {
        match self {
            SolverKind::Z3 => "z3",
            SolverKind::Cvc5 => "cvc5",
        }
    }

    /// The versions research/09 measured (plan §3.5).
    pub fn pinned(self) -> &'static str {
        match self {
            SolverKind::Z3 => "5.3.1",
            SolverKind::Cvc5 => "1.4.2",
        }
    }

    fn args(self, l: &Limits) -> Vec<String> {
        match self {
            SolverKind::Z3 => vec![
                "-smt2".into(),
                "-in".into(),
                format!("timeout={}", l.timeout_ms),
                format!("rlimit={}", l.rlimit),
                "smt.random_seed=0".into(),
                format!("memory_max_size={}", l.memory_mb),
            ],
            SolverKind::Cvc5 => vec![
                "--lang=smt2".into(),
                format!("--tlimit-per={}", l.timeout_ms),
                format!("--rlimit-per={}", l.rlimit),
                "--seed=0".into(),
            ],
        }
    }
}

/// Per-query limits of §17.106 (each of an obligation's queries gets all of them); cvc5's
/// `rlimit` units are not Z3's and are recorded separately. The `rlimit` binds first on the
/// reference machine, so statuses do not depend on machine speed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub timeout_ms: u64,
    pub rlimit: u64,
    /// The worker is killed after this long, whatever it is doing.
    pub deadline: Duration,
    /// Wall-clock budget for every query of one [`Solver`].
    pub aggregate: Duration,
    pub memory_mb: u64,
    pub output_cap: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            timeout_ms: 2000,
            rlimit: 10_000_000,
            deadline: Duration::from_secs(5),
            aggregate: Duration::from_secs(600),
            memory_mb: 1024,
            output_cap: 1 << 20,
        }
    }
}

impl Limits {
    /// The defaults with the `rlimit` of `sakura verify --rlimit N`, else of the manifest's
    /// `[verify] rlimit`, if either is given (§17.106). Zero is refused: Z3 reads it as no limit.
    pub fn with_rlimit(flag: Option<u64>, manifest: Option<u64>) -> Result<Limits, String> {
        let mut l = Limits::default();
        match flag.or(manifest) {
            Some(0) => return Err("rlimit must be at least 1".into()),
            Some(n) => l.rlimit = n,
            None => {}
        }
        Ok(l)
    }

    /// The [`SolverInfo`] the pinned `kind` records under these limits: what [`Solver::info`]
    /// returns for a solver [`Solver::find`] accepted, so a proof's cache key can be recomputed
    /// without running it (`sakura build`).
    pub fn info(&self, kind: SolverKind) -> SolverInfo {
        SolverInfo {
            name: kind.binary().into(),
            version: kind.pinned().into(),
            timeout_ms: self.timeout_ms,
            rlimit: self.rlimit,
            deadline_ms: self.deadline.as_millis() as u64,
        }
    }
}

#[derive(Debug)]
pub struct Solver {
    pub kind: SolverKind,
    pub path: PathBuf,
    pub version: String,
    pub limits: Limits,
    budget_end: Instant,
}

impl Solver {
    /// `explicit` (`--solver`) or the binary on `PATH`, if its version is the pinned one.
    pub fn find(
        kind: SolverKind,
        explicit: Option<&Path>,
        limits: Limits,
    ) -> Result<Solver, Reason> {
        let unavailable = |d: String| Reason::new(ReasonKind::SolverUnavailable, d);
        let path = match explicit {
            Some(p) => p.to_owned(),
            None => std::env::var_os("PATH")
                .and_then(|p| on_path(kind.binary(), &p))
                .ok_or_else(|| unavailable(format!("`{}` is not on PATH", kind.binary())))?,
        };
        let out = Command::new(&path)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| unavailable(format!("cannot run {}: {e}", path.display())))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let version = text
            .split_whitespace()
            .find(|w| w.split('.').count() == 3 && w.split('.').all(|p| p.parse::<u32>().is_ok()))
            .unwrap_or("")
            .to_owned();
        if version != kind.pinned() {
            return Err(unavailable(format!(
                "{} reports version `{}`; {} is pinned",
                path.display(),
                if version.is_empty() {
                    text.trim()
                } else {
                    &version
                },
                kind.pinned()
            )));
        }
        Ok(Solver {
            kind,
            path,
            version,
            budget_end: Instant::now() + limits.aggregate,
            limits,
        })
    }

    pub fn info(&self) -> SolverInfo {
        SolverInfo {
            version: self.version.clone(),
            ..self.limits.info(self.kind)
        }
    }

    /// Runs one query in a fresh process.
    pub fn ask(&self, script: &str) -> Answer {
        let now = Instant::now();
        if now >= self.budget_end {
            return Answer::Unknown(Reason::new(
                ReasonKind::Timeout,
                "aggregate budget exhausted",
            ));
        }
        let deadline = (now + self.limits.deadline).min(self.budget_end);
        let mut child = match Command::new(&self.path)
            .args(self.kind.args(&self.limits))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return Answer::Unknown(Reason::new(ReasonKind::Crash, format!("spawn: {e}")));
            }
        };
        let mut stdin = child.stdin.take().expect("piped");
        let input = script.to_owned();
        thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
        let mut stdout = child.stdout.take().expect("piped");
        let cap = self.limits.output_cap;
        let reader = thread::spawn(move || {
            let mut buf = vec![];
            let _ = (&mut stdout).take(cap as u64 + 1).read_to_end(&mut buf);
            buf
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() >= deadline => {
                    if let Some(s) = kill_at_deadline(&mut child) {
                        break s;
                    }
                    // The reader is detached: a grandchild may still hold the pipe.
                    return Answer::Unknown(Reason::new(
                        ReasonKind::Timeout,
                        format!(
                            "killed at the {} ms deadline",
                            self.limits.deadline.as_millis()
                        ),
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(2)),
                Err(e) => {
                    return Answer::Unknown(Reason::new(ReasonKind::Crash, format!("wait: {e}")));
                }
            }
        };
        let out = reader.join().unwrap_or_default();
        if out.len() > cap {
            return Answer::Unknown(Reason::new(
                ReasonKind::OutputLimit,
                format!("more than {cap} bytes of output"),
            ));
        }
        parse(
            &String::from_utf8_lossy(&out),
            status.success(),
            now.elapsed(),
            &self.limits,
        )
    }
}

/// Kills `child` at the deadline: `Some` with its own status when it ended on its own after the
/// last poll (a crash still being reported, say), so that status decides; `None` when the kill
/// ended it.
#[cfg(unix)]
fn kill_at_deadline(child: &mut Child) -> Option<ExitStatus> {
    use std::os::unix::process::ExitStatusExt;
    const SIGKILL: i32 = 9;
    let _ = child.kill();
    child.wait().ok().filter(|s| s.signal() != Some(SIGKILL))
}

#[cfg(not(unix))]
use kill_flagged as kill_at_deadline;

/// [`kill_at_deadline`] without signals (Windows): the kill is a flag, so a child still running
/// at the last look before it is the deadline's (design §17.180 (4c)).
#[cfg_attr(unix, allow(dead_code))]
fn kill_flagged(child: &mut Child) -> Option<ExitStatus> {
    if let Ok(Some(s)) = child.try_wait() {
        return Some(s);
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// The first `<dir>/<binary>` on `path`, with the platform's executable suffix (`z3.exe`).
fn on_path(binary: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let file = format!("{binary}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(path)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

/// Reads a reply to `(check-sat) (get-info :reason-unknown) (get-model)`.
pub fn parse(out: &str, exited_ok: bool, elapsed: Duration, limits: &Limits) -> Answer {
    let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("");
    match first {
        "sat" => Answer::Sat(model(out.split_once("sat").map_or("", |x| x.1))),
        "unsat" => Answer::Unsat,
        "unknown" => {
            let detail = out
                .split_once(":reason-unknown")
                .and_then(|(_, r)| r.split('"').nth(1))
                .unwrap_or("")
                .to_owned();
            let d = detail.to_lowercase();
            let kind = if d.contains("timeout") {
                ReasonKind::Timeout
            } else if d.contains("canceled") {
                // Z3 says `canceled` for both limits; its own timeout fires at `timeout_ms`.
                if elapsed.as_millis() as u64 >= limits.timeout_ms * 9 / 10 {
                    ReasonKind::Timeout
                } else {
                    ReasonKind::ResourceLimit
                }
            } else if d.contains("resource") || d.contains("rlimit") {
                ReasonKind::ResourceLimit
            } else if d.contains("memout") || d.contains("memory") {
                ReasonKind::MemoryLimit
            } else {
                ReasonKind::SolverUnknown
            };
            Answer::Unknown(Reason::new(kind, detail))
        }
        _ if first.is_empty() && !exited_ok => {
            Answer::Unknown(Reason::new(ReasonKind::Crash, "exited without an answer"))
        }
        _ if first.is_empty() => {
            Answer::Unknown(Reason::new(ReasonKind::MalformedReply, "empty reply"))
        }
        other => {
            let shown: String = other.chars().take(200).collect();
            Answer::Unknown(Reason::new(ReasonKind::MalformedReply, shown))
        }
    }
}

#[derive(Debug, PartialEq)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn sexps(s: &str) -> Vec<Sexp> {
    let mut stack: Vec<Vec<Sexp>> = vec![vec![]];
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => stack.push(vec![]),
            ')' => {
                if stack.len() > 1 {
                    let l = stack.pop().expect("nonempty");
                    stack.last_mut().expect("root").push(Sexp::List(l));
                }
            }
            '|' | '"' => {
                let tok: String = chars.by_ref().take_while(|&x| x != c).collect();
                stack.last_mut().expect("root").push(Sexp::Atom(tok));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut tok = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '(' || n == ')' {
                        break;
                    }
                    tok.push(n);
                    chars.next();
                }
                stack.last_mut().expect("root").push(Sexp::Atom(tok));
            }
        }
    }
    stack.swap_remove(0)
}

/// `define-fun` entries with literal `Int`/`Bool` values; anything else is skipped.
fn model(text: &str) -> Model {
    fn value(v: &Sexp) -> Option<String> {
        match v {
            Sexp::Atom(a) if a == "true" || a == "false" || a.parse::<i128>().is_ok() => {
                Some(a.clone())
            }
            Sexp::List(xs) => match &xs[..] {
                [Sexp::Atom(m), Sexp::Atom(n)] if m == "-" && n.parse::<u128>().is_ok() => {
                    Some(format!("-{n}"))
                }
                _ => None,
            },
            _ => None,
        }
    }
    fn walk(s: &Sexp, out: &mut Model) {
        if let Sexp::List(xs) = s {
            if let [Sexp::Atom(d), Sexp::Atom(name), Sexp::List(args), _sort, v] = &xs[..]
                && d == "define-fun"
                && args.is_empty()
            {
                if let Some(v) = value(v) {
                    out.insert(name.clone(), v);
                }
                return;
            }
            xs.iter().for_each(|x| walk(x, out));
        }
    }
    let mut out = Model::new();
    sexps(text).iter().for_each(|s| walk(s, &mut out));
    out
}

/// Decides `o` with `primary`; every `unsat` is re-asked of `cross` when given, and a `sat` there
/// blocks the proof (§17.85): the obligation becomes `unknown` with `solver_disagreement`.
pub fn verify(vc: &DefVc, o: &Obligation, primary: &Solver, cross: Option<&Solver>) -> Record {
    solve(vc, o, primary, cross).0
}

/// [`verify`] with the answers [`decide`] was given, in order: what a proof cache entry keeps so
/// that the record can be rebuilt without a solver (`crate::cache::Cache::reuse`).
pub fn solve(
    vc: &DefVc,
    o: &Obligation,
    primary: &Solver,
    cross: Option<&Solver>,
) -> (Record, Vec<Answer>) {
    let mut answers = vec![];
    let mut r = decide(vc, o, Some(primary.info()), |_, script| {
        let a = match primary.ask(script) {
            Answer::Unsat => match cross.map(|c| (c, c.ask(script))) {
                Some((c, Answer::Sat(_))) => Answer::Unknown(Reason::new(
                    ReasonKind::SolverDisagreement,
                    format!("{} {} answered sat", c.kind.binary(), c.version),
                )),
                _ => Answer::Unsat,
            },
            a => a,
        };
        answers.push(a.clone());
        a
    });
    r.cross_check = cross.map(Solver::info);
    (r, answers)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::Duration;

    #[test]
    fn the_path_lookup_appends_the_executable_suffix() {
        let dir = std::env::temp_dir().join(format!("sakura-exe-suffix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("z3{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&file, "").unwrap();
        let path = std::env::join_paths([&dir]).unwrap();
        assert_eq!(super::on_path("z3", &path), Some(file));
        assert_eq!(super::on_path("cvc5", &path), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_kill_flag_tells_a_deadline_kill_from_an_exit() {
        let mut running = Command::new("sleep").arg("30").spawn().unwrap();
        assert_eq!(super::kill_flagged(&mut running), None);
        let mut exited = Command::new("sh").args(["-c", "exit 3"]).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let status = super::kill_flagged(&mut exited).expect("it ended on its own");
        assert_eq!(status.code(), Some(3));
    }
}
