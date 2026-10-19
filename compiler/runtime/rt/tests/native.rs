//! The native profile `offset32-v1` (ABI §1b) at the runtime's level: instances own their heap
//! and host, keep the heap base fixed while it grows, and run hand-built code through the
//! context-first exports. Runs under Miri (`cargo +nightly miri test -p sakura_rt --features
//! native --test native`), where the heap reservation is a plain allocation.

#![cfg(feature = "native")]

mod common;

use std::rc::Rc;

use common::{CodeDesc, Known, Ty, codes_bytes, layout_bytes};
use sakura_rt::exec::step::{DEFECT, RETURN};
use sakura_rt::exports::{
    rt_alloc, rt_box_i64, rt_set_ret, rt_show_i64, rt_show_word, rt_vt_begin, rt_vt_end,
    rt_vt_push_word, rt_vt_result, rt_vt_take_word,
};
use sakura_rt::layout::WORD;
use sakura_rt::native::{
    CODES, Ctx, HOSTED, HandleId, Host, Hosted, Image, Instance, Limits, Module, Outcome,
    rt_hosted, sakura_abi_version,
};
use sakura_rt::{ABI_VERSION, tag};

/// The frame header's resume block (ABI §6.1).
const F_BLOCK: u32 = 12;

#[derive(Default)]
struct TestHost {
    lines: Vec<String>,
    retired: Vec<HandleId>,
    env: Vec<(String, String)>,
}

impl Host for TestHost {
    fn console_print(&mut self, line: &str) {
        self.lines.push(line.into());
    }
    fn console_read_line(&mut self) -> Option<String> {
        None
    }
    fn clock_now_ms(&mut self) -> i64 {
        0
    }
    fn clock_monotonic_ms(&mut self) -> i64 {
        0
    }
    fn clock_monotonic_us(&mut self) -> i64 {
        0
    }
    fn clock_sleep_ms(&mut self, _request: u32, _ms: i64) {}
    fn abort(&mut self, _request: u32) {}
    fn random_int(&mut self, lo: i64, _hi: i64) -> i64 {
        lo
    }
    fn random_float(&mut self) -> f64 {
        0.0
    }
    fn random_bytes(&mut self, n: usize) -> Vec<u8> {
        vec![0; n]
    }
    fn random_uuid(&mut self) -> String {
        "00000000-0000-4000-8000-000000000000".into()
    }
    fn log_write(&mut self, line: &str) {
        self.lines.push(line.into());
    }
    fn trace_enabled(&mut self) -> bool {
        false
    }
    fn trace_write(&mut self, _line: &str) {}
    fn retire_handle(&mut self, handle: HandleId) {
        self.retired.push(handle);
    }
    fn env_get(&mut self, key: &str) -> Hosted<Option<String>> {
        Ok(self
            .env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone()))
    }
}

/// Program codes, before the runtime's [`CODES`].
const MAIN: u32 = 0;
const GROW: u32 = 1;
const HOSTED_ENV: u32 = 2;
const N: u32 = 3;

/// The test program: an image built as `sakura_codegen_wasm` lays out its static block, and
/// three functions written as compiled code would be (one `match` on the resume block).
struct Program {
    image: Image,
}

fn align8(b: &mut Vec<u8>) -> u32 {
    while !b.len().is_multiple_of(8) {
        b.push(0);
    }
    b.len() as u32
}

fn program() -> Rc<Program> {
    let types = [
        Ty::Variant("std::option::Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("std::result::Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
    ];
    let known = Known {
        option: Some(0),
        result: Some(1),
        ..Known::default()
    };
    let mut codes: Vec<Option<CodeDesc>> = vec![
        Some(CodeDesc("main", &[WORD], &[])),
        Some(CodeDesc("grow", &[WORD], &[])),
        Some(CodeDesc("hosted_env", &[WORD], &[])),
    ];
    codes.extend(CODES.iter().map(|_| None));
    let mut bytes = Vec::new();
    let layout = align8(&mut bytes);
    bytes.extend(layout_bytes(&types, known));
    let codes_at = align8(&mut bytes);
    bytes.extend(codes_bytes(&codes));
    // No effects and no operations.
    let effects = align8(&mut bytes);
    bytes.extend(0u32.to_le_bytes());
    let ops = align8(&mut bytes);
    bytes.extend(0u32.to_le_bytes());
    // The ten native codes `init` receives: none is reached here, so all name the stub.
    let natives = align8(&mut bytes);
    for _ in 0..10 {
        bytes.extend(N.to_le_bytes());
    }
    // No type descriptors.
    let schemas = align8(&mut bytes);
    bytes.extend(0u32.to_le_bytes());
    Rc::new(Program {
        image: Image {
            bytes,
            layout,
            codes: codes_at,
            effects,
            ops,
            natives,
            schemas,
        },
    })
}

impl Module for Program {
    fn image(&self) -> &Image {
        &self.image
    }
    fn dispatch(&self, ctx: Ctx<'_>, code: u32, env: u32, frame: u32) -> u32 {
        match code {
            MAIN => main(ctx, frame),
            GROW => grow(ctx, frame),
            HOSTED_ENV => hosted_env(ctx, frame),
            _ => (CODES[(code - N) as usize].1)(ctx, env, frame),
        }
    }
}

/// Checks the ABI version `sakura_abi_version(ctx)` reports and boxes it.
fn main(ctx: Ctx<'_>, _frame: u32) -> u32 {
    assert_eq!(sakura_abi_version(ctx), ABI_VERSION);
    let b = rt_box_i64(ctx, i64::from(sakura_abi_version(ctx)));
    rt_set_ret(ctx, i64::from(b));
    RETURN
}

/// Allocates past the initial commit, keeping every object (a chain through slot 0 of a
/// 64 KiB block), and checks the heap base never moves and early stores survive growth.
fn grow(ctx: Ctx<'_>, frame: u32) -> u32 {
    let base = ctx.heap_base();
    let first = rt_alloc(ctx, u32::from(tag::BYTES), 1 << 16);
    assert_ne!(first, 0);
    ctx.store_u32(first + 8, 0);
    ctx.store_u64(first + 16, 0x5a5a_5a5a_5a5a_5a5a);
    let mut last = first;
    for _ in 0..256 {
        let b = rt_alloc(ctx, u32::from(tag::BYTES), 1 << 16);
        if b == 0 {
            // The heap limit: the defect is raised; the chain goes with the frame's task.
            return DEFECT;
        }
        ctx.store_u32(b + 8, 0);
        ctx.store_u32(last + 12, b);
        last = b;
    }
    assert_eq!(ctx.heap_base(), base, "the heap base moved");
    assert_eq!(ctx.load_u64(first + 16), 0x5a5a_5a5a_5a5a_5a5a);
    assert_eq!(ctx.load_u32(frame + F_BLOCK), 0);
    let r = rt_box_i64(ctx, 1);
    rt_set_ret(ctx, i64::from(r));
    RETURN
}

/// `env_get("42")` through the hosted glue, as N2's code calls it; slot 0 is set to 1 when its
/// answer shows as the test host's value.
fn hosted_env(ctx: Ctx<'_>, _frame: u32) -> u32 {
    let key = rt_show_i64(ctx, 42);
    assert_eq!(rt_vt_begin(ctx), 0);
    assert_eq!(rt_vt_push_word(ctx, key), 0);
    let args = rt_vt_end(ctx);
    let (len, at) = (ctx.load_u32(args + 8), args + 12);
    let id = HOSTED.iter().position(|h| *h == "env_get").unwrap() as u32;
    let answer = rt_hosted(ctx, id, at, len);
    sakura_rt::exports::drop(ctx, args);
    // `vt::top::OPTION`; status 2 has raised the host's defect.
    match rt_vt_result(ctx, answer, 4) {
        2 => return DEFECT,
        status => assert_eq!(status, 0),
    }
    let w = rt_vt_take_word(ctx);
    let shown = rt_show_word(ctx, w);
    let len = ctx.load_u32(shown + 8);
    let text: Vec<u8> = (0..len)
        .map(|i| ctx.load_u32(shown + 16 + i) as u8)
        .collect();
    assert_eq!(String::from_utf8(text).unwrap(), "Some(\"/home/sakura\")");
    sakura_rt::exports::drop(ctx, shown);
    rt_set_ret(ctx, i64::from(w));
    RETURN
}

fn run_code(inst: &mut Instance<TestHost>, code: u32) -> Outcome {
    let task = inst.start(code).unwrap();
    assert_ne!(inst.run(task), 4, "the task is still pending");
    let o = inst.outcome(task);
    inst.task_free(task);
    o
}

#[test]
fn two_instances_run_and_close_in_one_process() {
    let p = program();
    let mut a = Instance::new(p.clone(), TestHost::default(), Limits::default()).unwrap();
    let mut b = Instance::new(p, TestHost::default(), Limits::default()).unwrap();
    let ta = a.start(MAIN).unwrap();
    let tb = b.start(MAIN).unwrap();
    assert_eq!(a.run(ta), 0);
    assert_eq!(b.run(tb), 0);
    assert_eq!(a.outcome(ta), Outcome::Ok);
    assert_eq!(b.outcome(tb), Outcome::Ok);
    a.task_free(ta);
    b.task_free(tb);
    let base = a.stats().live_objects;
    assert_eq!(run_code(&mut a, MAIN), Outcome::Ok);
    assert_eq!(a.stats().live_objects, base);
    assert_eq!(a.exec_counts(), (0, 0));
    a.close();
    b.close();
}

#[test]
fn heap_base_is_fixed_across_growth() {
    // 32 MiB: the 256 blocks of 64 KiB need 16 MiB past the first commit.
    let limits = Limits {
        max_heap: 32 << 20,
        ..Limits::default()
    };
    let mut a = Instance::new(program(), TestHost::default(), limits).unwrap();
    let before = a.stats().committed;
    assert_eq!(run_code(&mut a, GROW), Outcome::Ok);
    assert!(a.stats().committed > before + (16 << 20));
}

#[test]
fn heap_limit_is_a_defect_not_a_crash() {
    let limits = Limits {
        max_heap: 8 << 20,
        ..Limits::default()
    };
    let mut a = Instance::new(program(), TestHost::default(), limits).unwrap();
    match run_code(&mut a, GROW) {
        Outcome::Defect { code, .. } => assert_eq!(code, 96),
        o => panic!("{o:?}"),
    }
}

#[test]
fn hosted_env_get_round_trips_through_vt1() {
    let host = TestHost {
        env: vec![("42".into(), "/home/sakura".into())],
        ..TestHost::default()
    };
    let mut a = Instance::new(program(), host, Limits::default()).unwrap();
    assert_eq!(run_code(&mut a, HOSTED_ENV), Outcome::Ok);
    assert_eq!(a.exec_counts(), (0, 0));
}

#[test]
fn an_answer_past_the_copy_budget_is_the_wasm_hosts_d091() {
    let host = TestHost {
        env: vec![("42".into(), "/home/sakura".into())],
        ..TestHost::default()
    };
    let limits = Limits {
        max_copy_bytes: 8,
        ..Limits::default()
    };
    let mut a = Instance::new(program(), host, limits).unwrap();
    match run_code(&mut a, HOSTED_ENV) {
        Outcome::Defect { code, message, .. } => {
            assert_eq!(code, 91);
            assert!(
                message.starts_with("response too large: ")
                    && message.ends_with(" bytes over SAKURA_MAX_COPY_BYTES"),
                "{message}"
            );
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn codes_follow_the_compilers_order() {
    let names: Vec<&str> = CODES.iter().map(|(n, _)| *n).collect();
    assert_eq!(names[0], "rt_code_callback_stub");
    let (code, cps): (Vec<&str>, Vec<&str>) =
        names[1..].iter().partition(|n| n.starts_with("rt_code_"));
    assert!(code.is_sorted() && cps.is_sorted());
    assert!(cps.iter().all(|n| n.starts_with("rt_k_")));
    assert_eq!(names.len(), 1 + code.len() + cps.len());
}
