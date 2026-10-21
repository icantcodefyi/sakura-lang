//! The entry of a native Sakura program (plan M5 N2; ABI §1b). The object `sakura_codegen_native`
//! emits defines `main`, which calls [`sakura_native_main_v5`] with the module's descriptor; this
//! crate, built as a staticlib with `panic = "abort"`, brings the runtime and the native host.
//! It runs `main` as the Node host does: one instance on one thread, the outcome reported on
//! stderr and as the exit code. A live worker (`std/worker`, ABI 1.15.0) is another instance
//! of the module on its own thread, started at its entry ([`launcher`]).

#![cfg(feature = "native")]

use std::rc::Rc;
use std::sync::Arc;

use sakura_host_native::worker::{Job, Launcher, WorkerFn, parse_table};
use sakura_host_native::{Config, NativeHost, Span, report_spans, run_task};
use sakura_rt::native::{
    CODES, Ctx, Image, Instance, Limits, Module, Outcome, PROFILE, SakuraCode,
};
use sakura_rt::vt;

/// The stack of `main`'s and each worker's thread: `sakura_lir::STACK_SIZE`, mirrored because
/// the staticlib a program links carries no compiler crate.
const STACK_SIZE: usize = 1 << 28;

/// The module descriptor the compiler emits (`sakura_codegen_native`, symbol `sakura$module`):
/// pointers are the target's, every other field a `u32`. The layout is a contract between the
/// compiler and this crate, which ship together.
#[repr(C)]
pub struct SakuraModule {
    /// `sakura_abi_version` the code was compiled against (+0); the driver checks the
    /// `sakura.native` record's.
    _abi: u32,
    /// The module's functions: codes `0..n_codes` (+4).
    n_codes: u32,
    /// Their entry points (+8).
    codes: *const SakuraCode,
    /// The static block (+16) and its length (+24).
    image: *const u8,
    image_len: u32,
    /// Offsets in the static block (+28..+48).
    layout: u32,
    code_table: u32,
    effects: u32,
    ops: u32,
    natives: u32,
    /// The code of `main`, or `u32::MAX` (+48).
    entry: u32,
    /// The length of the runtime's `CODES` the code was numbered against (+52).
    n_natives: u32,
    /// The `sakura.debug` table (ABI §12) (+56) and its length (+64).
    debug: *const u8,
    debug_len: u32,
    /// The type descriptors' offset in the static block (+68).
    schemas: u32,
    /// The `sakura.native` record (+72) and its length (+80) (ABI §1b).
    native: *const u8,
    native_len: u32,
    /// The length (+84) and bytes (+88) of `main`'s root effects, one std effect name per
    /// line: the row after handling, whose root handlers the program installs.
    roots_len: u32,
    roots: *const u8,
    /// The length (+96) and bytes (+104) of the worker table
    /// (`sakura_lir::image::workers_table`, ABI 1.15.0).
    workers_len: u32,
    workers: *const u8,
}

const _: () = assert!(size_of::<SakuraModule>() == 112);

/// The std effects whose host service the native host does not implement: a `main` (or a
/// live worker's entry) that leaves one to the runtime is refused at startup, exit 75, before
/// any user code runs (RFC 0002 §7, design §17.271 (9); `WebSocket` since web-APIs phase 8;
/// `Http` is served since N1b); a use under a handler `main` installs is `D090` at first use.
const UNAVAILABLE: &[&str] = &["Llm", "Serve", "WebSocket"];

/// The exit code of a program refused at startup (`sakura run`'s permission denial too).
const EXIT_UNAVAILABLE: i32 = 75;

/// The first of `roots` (newline-separated effect names) the native host lacks.
fn unavailable(roots: &[u8]) -> Option<&str> {
    std::str::from_utf8(roots)
        .ok()?
        .split('\n')
        .find(|e| UNAVAILABLE.contains(e))
}

/// A bounds-checked reader of little-endian bytes.
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// A `sakura-vt1` tag (ABI §8.1).
    fn tag(&mut self, tag: u8) -> Option<()> {
        (self.take(1)? == [tag]).then_some(())
    }

    fn string(&mut self) -> Option<&'a str> {
        self.tag(vt::t::STRING)?;
        let n = self.u32()?;
        std::str::from_utf8(self.take(n as usize)?).ok()
    }
}

/// The `sakura.native` record's target, profile and ABI version (ABI §1b), or `None` when it
/// is malformed.
fn record(b: &[u8]) -> Option<(&str, &str, u32)> {
    let mut c = Cursor { b, at: 0 };
    c.tag(vt::t::TUPLE)?;
    (c.u32()? == 6).then_some(())?;
    let target = c.string()?;
    let (_isa, _cranelift) = (c.string()?, c.string()?);
    let profile = c.string()?;
    c.tag(vt::t::INT)?;
    let abi = i64::from_le_bytes(c.take(8)?.try_into().ok()?);
    c.tag(vt::t::BYTES)?;
    (c.u32()? == 32).then_some(())?;
    c.take(32)?;
    (c.at == b.len()).then_some((target, profile, u32::try_from(abi).ok()?))
}

/// Why this runtime refuses a program whose `sakura.native` record is `b`: another target or
/// profile, another ABI MAJOR or a newer MINOR (ABI §0, §1b).
fn refuse(b: &[u8]) -> Option<String> {
    let Some((target, profile, abi)) = record(b) else {
        return Some("the program's sakura.native record is malformed".into());
    };
    let host = target_lexicon::HOST.to_string();
    let have = sakura_rt::ABI_VERSION;
    if target != host {
        Some(format!(
            "the program was compiled for {target}, this runtime is {host}"
        ))
    } else if profile != PROFILE {
        Some(format!(
            "the program uses the layout profile {profile}, this runtime {PROFILE}"
        ))
    } else if abi >> 16 != have >> 16 || (abi >> 8 & 0xff) > (have >> 8 & 0xff) {
        Some(format!(
            "the program was compiled for runtime ABI {abi:#08x}, this runtime is {have:#08x}"
        ))
    } else {
        None
    }
}

/// The sources and sites of a `sakura.debug` table (ABI §12), or `None` when it is malformed:
/// it is bounds-checked, never trusted.
struct Debug {
    sources: Vec<String>,
    /// `(source, start, end, role, contract)`.
    sites: Vec<(u32, u32, u32, u8, u32)>,
}

impl Debug {
    fn parse(b: &[u8]) -> Option<Debug> {
        let mut c = Cursor { b, at: 0 };
        if c.take(8)? != b"SKRDBG01" {
            return None;
        }
        c.take(4 + 32)?;
        let mut sources = vec![];
        for _ in 0..c.u32()? {
            let len = c.u32()? as usize;
            sources.push(String::from_utf8(c.take(len)?.to_vec()).ok()?);
            c.take(32)?;
        }
        let mut sites = vec![];
        for _ in 0..c.u32()? {
            let (s, a, e) = (c.u32()?, c.u32()?, c.u32()?);
            let role = c.take(1)?[0];
            sites.push((s, a, e, role, c.u32()?));
        }
        Some(Debug { sources, sites })
    }

    /// The spans of a defect at site id `site` (0 is none), as the JS runtime's `violated`
    /// gives them (design §17.162; `sakura_wasm_host.js` `defectSpans`).
    fn spans(&self, site: u32) -> Vec<Span> {
        let Some(&at) = (site as usize)
            .checked_sub(1)
            .and_then(|i| self.sites.get(i))
        else {
            return vec![];
        };
        let span = |(s, a, e, _, _): (u32, u32, u32, u8, u32), role| {
            self.sources
                .get(s as usize)
                .filter(|uri| !uri.is_empty())
                .map(|uri| Span {
                    uri: uri.clone(),
                    start_byte: a,
                    end_byte: e,
                    role,
                })
        };
        if at.3 != 0 || at.4 == 0 {
            return span(at, "primary").into_iter().collect();
        }
        let clause = self.sites.iter().find(|s| s.3 == 1 && s.4 == at.4);
        span(at, "primary")
            .into_iter()
            .chain(clause.and_then(|&c| span(c, "definition")))
            .collect()
    }
}

/// The descriptor, moved to the program's threads.
#[derive(Clone, Copy)]
struct Desc(&'static SakuraModule);

// SAFETY: the descriptor and everything it points at are immutable statics of the program.
unsafe impl Send for Desc {}
// SAFETY: as `Send`: nothing behind it is ever written.
unsafe impl Sync for Desc {}

/// Compiled code behind the runtime's [`Module`]: codes below `n` are the program's, the rest
/// the runtime's [`CODES`] in the table's order.
struct Linked {
    codes: &'static [SakuraCode],
    image: Image,
}

impl Module for Linked {
    fn image(&self) -> &Image {
        &self.image
    }

    fn dispatch(&self, ctx: Ctx<'_>, code: u32, env: u32, frame: u32) -> u32 {
        let n = self.codes.len() as u32;
        let f = match code.checked_sub(n) {
            None => self.codes[code as usize],
            Some(k) => match CODES.get(k as usize) {
                Some((_, f)) => *f,
                None => CODES
                    .iter()
                    .find(|(name, _)| *name == "rt_code_unsupported")
                    .map(|(_, f)| *f)
                    .expect("rt_code_unsupported is a native code"),
            },
        };
        f(ctx, env, frame)
    }
}

/// A limit variable's value, `default` when unset.
fn limit<T: std::str::FromStr>(
    name: &str,
    value: Result<String, std::env::VarError>,
    default: T,
) -> Result<T, String> {
    match value {
        Ok(v) => v
            .parse()
            .map_err(|_| format!("{name} must be a non-negative integer")),
        Err(_) => Ok(default),
    }
}

/// The module's code and a copy of its static block, for one instance.
fn linked(m: &'static SakuraModule) -> Rc<Linked> {
    // SAFETY: the compiler emits `codes` as `n_codes` function pointers and `image` as
    // `image_len` bytes, immutable for the program's life.
    let (codes, bytes) = unsafe {
        (
            std::slice::from_raw_parts(m.codes, m.n_codes as usize),
            std::slice::from_raw_parts(m.image, m.image_len as usize),
        )
    };
    Rc::new(Linked {
        codes,
        image: Image {
            bytes: bytes.to_vec(),
            layout: m.layout,
            codes: m.code_table,
            effects: m.effects,
            ops: m.ops,
            natives: m.natives,
            schemas: m.schemas,
        },
    })
}

/// Starts each live worker on its own thread: a second instance of the module with the
/// parent's limits (its heap bound too, design §17.299 (13)), whose host has the worker's
/// grants as its permissions, running the worker's entry.
fn launcher(m: Desc, limits: Limits, table: Vec<WorkerFn>) -> Launcher {
    Arc::new(move |job: Job| {
        let table = table.clone();
        let spawned = std::thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(move || run_worker(m, limits, table, job));
        if let Err(e) = spawned {
            // The job went with the closure: the worker ends on `D090`.
            eprintln!("error: a worker's thread did not start: {e}");
        }
    })
}

fn run_worker(m: Desc, limits: Limits, table: Vec<WorkerFn>, job: Job) {
    let Job {
        worker,
        config,
        parent,
    } = job;
    let mut host = match NativeHost::new(config) {
        Ok(h) => h,
        Err(e) => return eprintln!("error: worker {}: {e}", worker.key),
    };
    host.attach(parent);
    if let Some(effect) = worker
        .effects
        .iter()
        .find(|e| UNAVAILABLE.contains(&e.as_str()))
    {
        eprintln!("`{effect}` is not available on target `native`");
        return host.report_end(&worker.key, &Outcome::Exit(EXIT_UNAVAILABLE));
    }
    host.set_workers(table.clone(), launcher(m, limits, table));
    let mut inst = match Instance::new(linked(m.0), host, limits) {
        Ok(i) => i,
        Err(d) => {
            return eprintln!(
                "error: worker {}: D{:03}: {}",
                worker.key, d.code, d.message
            );
        }
    };
    let o = match inst.start(worker.entry) {
        Ok(task) => {
            let o = run_task(&mut inst, task);
            inst.task_free(task);
            o
        }
        Err(d) => Outcome::Defect {
            code: d.code,
            site: 0,
            message: d.message.into(),
        },
    };
    inst.host().report_end(&worker.key, &o);
    inst.close().close();
}

fn run(m: &'static SakuraModule) -> Result<i32, String> {
    // SAFETY: the compiler emits `native` as `native_len` immutable bytes.
    let native = unsafe { std::slice::from_raw_parts(m.native, m.native_len as usize) };
    if let Some(why) = refuse(native) {
        return Err(format!("V8003: {why}"));
    }
    if m.n_natives as usize != CODES.len() {
        return Err(format!(
            "V8003: the program numbers {} runtime codes, this runtime has {}",
            m.n_natives,
            CODES.len()
        ));
    }
    // SAFETY: the compiler emits `roots` as `roots_len` immutable bytes.
    let roots = unsafe { std::slice::from_raw_parts(m.roots, m.roots_len as usize) };
    if let Some(effect) = unavailable(roots) {
        eprintln!("`{effect}` is not available on target `native`");
        return Ok(EXIT_UNAVAILABLE);
    }
    // SAFETY: the compiler emits `debug` and `workers` as `debug_len` and `workers_len`
    // immutable bytes.
    let (debug, workers) = unsafe {
        (
            std::slice::from_raw_parts(m.debug, m.debug_len as usize),
            std::slice::from_raw_parts(m.workers, m.workers_len as usize),
        )
    };
    let table = parse_table(workers).ok_or("the program's worker table is malformed")?;
    let module = linked(m);
    let config = Config::from_env()?;
    let defaults = Limits::default();
    let limits = Limits {
        max_heap: limit(
            "SAKURA_MAX_HEAP",
            std::env::var("SAKURA_MAX_HEAP"),
            defaults.max_heap,
        )?,
        max_requests: limit(
            "SAKURA_MAX_REQUESTS",
            std::env::var("SAKURA_MAX_REQUESTS"),
            defaults.max_requests,
        )?,
        max_copy_bytes: config.max_copy_bytes,
    };
    let mut host = NativeHost::new(config)?;
    host.set_workers(table.clone(), launcher(Desc(m), limits, table));
    let mut inst = Instance::new(module, host, limits)
        .map_err(|d| format!("D{:03}: {}", d.code, d.message))?;
    if m.entry == u32::MAX {
        return Err("the program has no main".into());
    }
    let task = inst
        .start(m.entry)
        .map_err(|d| format!("D{:03}: {}", d.code, d.message))?;
    let o = run_task(&mut inst, task);
    inst.task_free(task);
    let spans = match (&o, Debug::parse(debug)) {
        (Outcome::Defect { site, .. }, Some(d)) => d.spans(*site),
        _ => vec![],
    };
    let code = report_spans(inst.host(), &o, &spans);
    inst.host().flush_out();
    if let Some(path) = std::env::var_os("SAKURA_NATIVE_STATS")
        && let Err(e) = append_stats(&path, inst.stats())
    {
        eprintln!("warning: {e}");
    }
    inst.close().close();
    Ok(code)
}

/// One JSON line of the `stats()` counters (ABI §10) by name, as the WASM host's
/// `SAKURA_WASM_STATS` writes it, for benchmark records.
fn append_stats(path: &std::ffi::OsStr, s: sakura_rt::Stats) -> Result<(), String> {
    use std::io::Write;
    let fields = [
        ("requested", s.requested),
        ("live", s.live),
        ("peak", s.peak),
        ("live_objects", s.live_objects),
        ("committed", s.committed),
        ("free_list_bytes", s.free_list_bytes),
        ("frames", s.frames),
        ("boxes", s.boxes),
        ("dups", s.dups),
        ("drops", s.drops),
        ("restarts", s.restarts),
        ("reuse_attempts", s.reuse_attempts),
        ("reuses", s.reuses),
        ("borrow_candidates", s.borrow_candidates),
        ("borrows", s.borrows),
    ];
    let body: Vec<String> = fields.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{{{}}}", body.join(",")))
        .map_err(|e| format!("SAKURA_NATIVE_STATS: {e}"))
}

/// The C entry a native program's `main` calls (ABI §1b): runs `main` of module `m` on a
/// thread with a large stack and returns the process exit code.
///
/// # Safety
///
/// `m` is the descriptor `sakura_codegen_native` emits: a static that lives as long as the
/// program, whose `codes`, `image`, `debug`, `native`, `roots` and `workers` point at
/// `n_codes` functions, `image_len`, `debug_len`, `native_len`, `roots_len` and `workers_len`
/// bytes. The suffix is the descriptor layout's version: a layout change renames the entry, so
/// an object from another layout fails to link instead of misreading this one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sakura_native_main_v5(m: *const SakuraModule) -> i32 {
    // SAFETY: the caller's contract above.
    let m = Desc(unsafe { &*m });
    let code = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || {
            let m = m;
            run(m.0).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                70
            })
        })
        .map_err(|e| e.to_string())
        .and_then(|t| t.join().map_err(|_| "the program's thread panicked".into()));
    code.unwrap_or_else(|e: String| {
        eprintln!("error: {e}");
        70
    })
}
