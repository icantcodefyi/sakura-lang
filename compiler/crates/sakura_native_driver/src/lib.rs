//! The entry of a native Sakura program (plan M5 N2; ABI §1b). The object `sakura_codegen_native`
//! emits defines `main`, which calls [`sakura_native_main_v2`] with the module's descriptor; this
//! crate, built as a staticlib with `panic = "abort"`, brings the runtime and the native host.
//! It runs `main` as the Node host does: one instance on one thread, the outcome reported on
//! stderr and as the exit code.

#![cfg(feature = "native")]

use std::rc::Rc;

use sakura_host_native::{Config, NativeHost, Span, report_spans, run_task};
use sakura_rt::native::{CODES, Ctx, Image, Instance, Limits, Module, Outcome, SakuraCode};

/// The module descriptor the compiler emits (`sakura_codegen_native`, symbol `sakura$module`):
/// pointers are the target's, every other field a `u32`. The layout is a contract between the
/// compiler and this crate, which ship together.
#[repr(C)]
pub struct SakuraModule {
    /// `sakura_abi_version` the code was compiled against (+0).
    abi: u32,
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
    _reserved: u32,
}

const _: () = assert!(size_of::<SakuraModule>() == 72);

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

/// The descriptor, moved to the program's thread.
struct Desc(&'static SakuraModule);

// SAFETY: the descriptor and everything it points at are immutable statics of the program.
unsafe impl Send for Desc {}

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

fn run(m: &'static SakuraModule) -> Result<i32, String> {
    if m.abi != sakura_rt::ABI_VERSION {
        return Err(format!(
            "V8003: the program was compiled for runtime ABI {:#08x}, this runtime is {:#08x}",
            m.abi,
            sakura_rt::ABI_VERSION
        ));
    }
    if m.n_natives as usize != CODES.len() {
        return Err(format!(
            "V8003: the program numbers {} runtime codes, this runtime has {}",
            m.n_natives,
            CODES.len()
        ));
    }
    // SAFETY: the compiler emits `codes` as `n_codes` function pointers, `image` and `debug`
    // as `image_len` and `debug_len` bytes, all immutable for the program's life.
    let (codes, bytes, debug) = unsafe {
        (
            std::slice::from_raw_parts(m.codes, m.n_codes as usize),
            std::slice::from_raw_parts(m.image, m.image_len as usize),
            std::slice::from_raw_parts(m.debug, m.debug_len as usize),
        )
    };
    let module = Rc::new(Linked {
        codes,
        image: Image {
            bytes: bytes.to_vec(),
            layout: m.layout,
            codes: m.code_table,
            effects: m.effects,
            ops: m.ops,
            natives: m.natives,
        },
    });
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
    let host = NativeHost::new(config)?;
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
/// program, whose `codes`, `image` and `debug` point at `n_codes` functions, `image_len` and
/// `debug_len` bytes. The suffix is the descriptor layout's version: a layout change renames
/// the entry, so an object from another layout fails to link instead of misreading this one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sakura_native_main_v2(m: *const SakuraModule) -> i32 {
    // SAFETY: the caller's contract above.
    let m = Desc(unsafe { &*m });
    let code = std::thread::Builder::new()
        .stack_size(1 << 28)
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
