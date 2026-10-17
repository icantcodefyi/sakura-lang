//! End-to-end tests over hand-built Core: emit, run under node, compare stdout.

use std::path::PathBuf;
use std::process::Command;

use sakura_codegen_js::{Options, emit};
use sakura_core::build::*;
use sakura_core::*;

const CONSOLE: EffectId = EffectId(0);
const COUNTER: EffectId = EffectId(1);

fn base() -> Module {
    let op = |name: &str, params: Vec<Ty>, ret: Ty| OpDef {
        name: name.into(),
        params,
        ret,
        fail: None,
    };
    Module {
        effects: vec![
            EffectDef {
                name: "Console".into(),
                module: "std::console".into(),
                ops: vec![
                    op("print", vec![Ty::String], Ty::Unit),
                    op("read_line", vec![], Ty::Unit),
                ],
            },
            EffectDef {
                name: "Counter".into(),
                module: "main".into(),
                ops: vec![op("next", vec![], Ty::Int)],
            },
        ],
        ..Module::default()
    }
}

fn row(effects: &[EffectId]) -> Row {
    Row {
        effects: effects.to_vec(),
        ..Row::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn add_def(
    m: &mut Module,
    name: &str,
    kind: FnKind,
    b: FnBuilder,
    caps: Vec<Local>,
    params: Vec<Local>,
    ret: Ty,
    row: Row,
    body: Expr,
) -> DefId {
    m.defs.push(FnDef {
        name: name.into(),
        module: "main".into(),
        kind,
        captures: caps,
        params,
        locals: b.locals,
        ret,
        row,
        row_vars: vec![],
        pos: None,
        body,
    });
    DefId(m.defs.len() as u32 - 1)
}

fn top(
    m: &mut Module,
    name: &str,
    b: FnBuilder,
    params: Vec<Local>,
    ret: Ty,
    row: Row,
    body: Expr,
) -> DefId {
    add_def(
        m,
        name,
        FnKind::Top { export: false },
        b,
        vec![],
        params,
        ret,
        row,
        body,
    )
}

/// `$entry`: install the host console and call `main`.
fn entry(m: &mut Module, main: DefId) {
    let mut b = FnBuilder::new();
    let h = b.local("console", Ty::Handler(CONSOLE));
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(h, Bind::Call(builtin("console_host"), vec![]));
    blk.handle(local(h));
    blk.push(r, Bind::Call(Atom::Global(main), vec![]));
    let body = blk.finish(Expr::Ret(local(r)));
    let e = top(m, "$entry", b, vec![], Ty::Unit, Row::empty(), body);
    m.entry = Some(e);
}

/// `main() { Console.print(show(value_of(arg))) }`
fn print_main(m: &mut Module, f: DefId, arg: i64, ty: Ty) -> DefId {
    let mut b = FnBuilder::new();
    let v = b.local("v", ty.clone());
    let s = b.local("s", Ty::String);
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(v, Bind::Call(Atom::Global(f), vec![int(arg)]));
    blk.push(s, Bind::Prim(PrimOp::Show(ty), vec![local(v)]));
    blk.push(r, Bind::Perform(CONSOLE, 0, vec![local(s)]));
    let body = blk.finish(Expr::Ret(local(r)));
    top(m, "main", b, vec![], Ty::Unit, row(&[CONSOLE]), body)
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run(m: &Module, name: &str, direct: bool) -> Run {
    validate(m).unwrap_or_else(|e| panic!("invalid Core: {e:?}"));
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("handbuilt-{name}-{direct}"));
    let _ = std::fs::remove_dir_all(&dir);
    let opts = Options {
        direct_style: direct,
        ..Options::new(vec![])
    };
    for f in emit(m, &opts) {
        let path = dir.join(&f.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, f.text).unwrap();
    }
    let out = Command::new("node")
        .arg(dir.join("__main__.js"))
        .output()
        .expect("node runs");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn both(m: &Module, name: &str, expected: &str) {
    for direct in [false, true] {
        let r = run(m, name, direct);
        assert_eq!(
            (r.stdout.as_str(), r.code),
            (expected, 0),
            "direct={direct}\n{}",
            r.stderr
        );
    }
}

#[test]
fn hello_world() {
    let mut m = base();
    let mut b = FnBuilder::new();
    let name = b.local("name", Ty::String);
    let line = b.local("line", Ty::String);
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(
        line,
        Bind::Prim(PrimOp::Concat, vec![str_("hello, "), local(name)]),
    );
    blk.push(r, Bind::Perform(CONSOLE, 0, vec![local(line)]));
    let greet = top(
        &mut m,
        "greet",
        b,
        vec![name],
        Ty::Unit,
        row(&[CONSOLE]),
        blk.finish(Expr::Ret(local(r))),
    );
    let mut b = FnBuilder::new();
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(
        r,
        Bind::Call(Atom::Global(greet), vec![str_("Sakura Lang")]),
    );
    let main = top(
        &mut m,
        "main",
        b,
        vec![],
        Ty::Unit,
        row(&[CONSOLE]),
        blk.finish(Expr::Ret(local(r))),
    );
    entry(&mut m, main);
    both(&m, "hello", "hello, Sakura Lang\n");
    let js = emit(&m, &Options::new(vec![]))
        .into_iter()
        .find(|f| f.path == "main.js")
        .unwrap();
    insta::assert_snapshot!(js.text);
}

/// `fn sum(n) -> Int { if n == 0 { 0 } else { n + sum(n - 1) } }`, not a tail call.
#[test]
fn million_deep_recursion() {
    let mut m = base();
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let z = b.local("z", Ty::Bool);
    let n1 = b.local("n1", Ty::Int);
    let s = b.local("s", Ty::Int);
    let t = b.local("t", Ty::Int);
    let mut els = Block::new();
    els.push(n1, Bind::Prim(PrimOp::IntSub, vec![local(n), int(1)]));
    els.push(s, Bind::Call(Atom::Global(DefId(0)), vec![local(n1)]));
    els.push(t, Bind::Prim(PrimOp::IntAdd, vec![local(n), local(s)]));
    let mut blk = Block::new();
    blk.push(
        z,
        Bind::Prim(PrimOp::Eq(CmpTy::Int), vec![local(n), int(0)]),
    );
    let body = blk.finish(Expr::If {
        cond: local(z),
        then: Box::new(Expr::Ret(int(0))),
        els: Box::new(els.finish(Expr::Ret(local(t)))),
    });
    let sum = top(&mut m, "sum", b, vec![n], Ty::Int, Row::empty(), body);
    let main = print_main(&mut m, sum, 1_000_000, Ty::Int);
    entry(&mut m, main);
    both(&m, "sum", "500000500000\n");
}

/// A pure `while` loop: `var i = 0; var acc = 0.0; while i < n { acc = acc + 0.5; i = i + 1 }`.
#[test]
fn loops_run_in_constant_stack() {
    let mut m = base();
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let i = b.local("i", Ty::Int);
    let acc = b.local("acc", Ty::Float);
    let c = b.local("c", Ty::Bool);
    let i2 = b.local("i", Ty::Int);
    let acc2 = b.local("acc", Ty::Float);
    let lp = b.label();
    let mut step = Block::new();
    step.push(
        acc2,
        Bind::Prim(
            PrimOp::FloatAdd,
            vec![local(acc), Atom::Lit(Lit::Float(0.5))],
        ),
    );
    step.push(i2, Bind::Prim(PrimOp::IntAdd, vec![local(i), int(1)]));
    let mut head = Block::new();
    head.push(
        c,
        Bind::Prim(PrimOp::Lt(CmpTy::Int), vec![local(i), local(n)]),
    );
    let join = head.finish(Expr::If {
        cond: local(c),
        then: Box::new(step.finish(Expr::Jump(lp, vec![local(i2), local(acc2)]))),
        els: Box::new(Expr::Ret(local(acc))),
    });
    let body = Expr::LetJoin {
        label: lp,
        params: vec![i, acc],
        join: Box::new(join),
        body: Box::new(Expr::Jump(lp, vec![int(0), Atom::Lit(Lit::Float(0.0))])),
    };
    let f = top(&mut m, "halves", b, vec![n], Ty::Float, Row::empty(), body);
    let main = print_main(&mut m, f, 2_000_000, Ty::Float);
    entry(&mut m, main);
    both(&m, "loop", "1000000.0\n");
}

/// Counter handler with state; 1,000,000 operations; nested instances (design §15.3).
#[test]
fn handler_state_and_a_million_operations() {
    let mut m = base();
    // clause next(inst) { n = n + 1; n }
    let mut b = FnBuilder::new();
    let inst = b.local("inst", Ty::Instance(COUNTER));
    let n = b.local("n", Ty::Int);
    let n1 = b.local("n1", Ty::Int);
    let u = b.local("u", Ty::Unit);
    let mut blk = Block::new();
    blk.push(n, Bind::StateGet(local(inst), 0));
    blk.push(n1, Bind::Prim(PrimOp::IntAdd, vec![local(n), int(1)]));
    blk.push(u, Bind::StateSet(local(inst), 0, local(n1)));
    let clause = add_def(
        &mut m,
        "counter$next",
        FnKind::Clause {
            effect: COUNTER,
            op: 0,
        },
        b,
        vec![],
        vec![inst],
        Ty::Int,
        Row::empty(),
        blk.finish(Expr::Ret(local(n1))),
    );
    // fn counter(start) -> Handler<Counter>
    let mut b = FnBuilder::new();
    let start = b.local("start", Ty::Int);
    let c = b.local("next", Ty::Unit);
    let h = b.local("h", Ty::Handler(COUNTER));
    let mut blk = Block::new();
    blk.push(c, Bind::Closure(clause, vec![]));
    blk.push(
        h,
        Bind::MakeHandler {
            effect: COUNTER,
            state: vec![local(start)],
            clauses: vec![local(c)],
        },
    );
    let counter = top(
        &mut m,
        "counter",
        b,
        vec![start],
        Ty::Handler(COUNTER),
        Row::empty(),
        blk.finish(Expr::Ret(local(h))),
    );
    // fn spin(n) -> Int ! {Counter} { var last = 0; var i = 0; while i < n { last = Counter.next(); i = i + 1 }; last }
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let i = b.local("i", Ty::Int);
    let last = b.local("last", Ty::Int);
    let c = b.local("c", Ty::Bool);
    let v = b.local("v", Ty::Int);
    let i2 = b.local("i", Ty::Int);
    let lp = b.label();
    let mut step = Block::new();
    step.push(v, Bind::Perform(COUNTER, 0, vec![]));
    step.push(i2, Bind::Prim(PrimOp::IntAdd, vec![local(i), int(1)]));
    let mut head = Block::new();
    head.push(
        c,
        Bind::Prim(PrimOp::Lt(CmpTy::Int), vec![local(i), local(n)]),
    );
    let join = head.finish(Expr::If {
        cond: local(c),
        then: Box::new(step.finish(Expr::Jump(lp, vec![local(i2), local(v)]))),
        els: Box::new(Expr::Ret(local(last))),
    });
    let body = Expr::LetJoin {
        label: lp,
        params: vec![i, last],
        join: Box::new(join),
        body: Box::new(Expr::Jump(lp, vec![int(0), int(0)])),
    };
    let spin = top(&mut m, "spin", b, vec![n], Ty::Int, row(&[COUNTER]), body);
    // fn run(n) -> Int { with counter(0) { let a = spin(n); with counter(100) { Counter.next() + a } } }
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let h0 = b.local("h0", Ty::Handler(COUNTER));
    let a = b.local("a", Ty::Int);
    let h1 = b.local("h1", Ty::Handler(COUNTER));
    let x = b.local("x", Ty::Int);
    let r = b.local("r", Ty::Int);
    let mut blk = Block::new();
    blk.push(h0, Bind::Call(Atom::Global(counter), vec![int(0)]));
    blk.handle(local(h0));
    blk.push(a, Bind::Call(Atom::Global(spin), vec![local(n)]));
    blk.push(h1, Bind::Call(Atom::Global(counter), vec![int(100)]));
    blk.handle(local(h1));
    blk.push(x, Bind::Perform(COUNTER, 0, vec![]));
    blk.push(r, Bind::Prim(PrimOp::IntAdd, vec![local(x), local(a)]));
    let runf = top(
        &mut m,
        "run",
        b,
        vec![n],
        Ty::Int,
        Row::empty(),
        blk.finish(Expr::Ret(local(r))),
    );
    let main = print_main(&mut m, runf, 1_000_000, Ty::Int);
    entry(&mut m, main);
    both(&m, "counter", "1000101\n");
}

#[test]
fn defects_exit_70_with_a_json_record() {
    let mut m = base();
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let q = b.local("q", Ty::Int);
    let mut blk = Block::new();
    blk.push(q, Bind::Prim(PrimOp::IntDiv, vec![int(1), local(n)]));
    let f = top(
        &mut m,
        "inv",
        b,
        vec![n],
        Ty::Int,
        Row::empty(),
        blk.finish(Expr::Ret(local(q))),
    );
    let main = print_main(&mut m, f, 0, Ty::Int);
    entry(&mut m, main);
    let r = run(&m, "defect", true);
    assert_eq!(r.code, 70);
    assert_eq!(
        r.stderr.trim(),
        r#"{"schema_version":"1.0","kind":"defect","code":"D002","message":"division by zero"}"#
    );
}
