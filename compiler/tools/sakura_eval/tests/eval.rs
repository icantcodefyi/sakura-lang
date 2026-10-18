//! Hand-built Core programs (validated with `sakura_core::validate`) run through the evaluator.

use sakura_core::build::*;
use sakura_core::*;
use sakura_eval::{Limits, Stop, run_main};

const CONSOLE: EffectId = EffectId(0);

fn module() -> Module {
    Module {
        effects: vec![EffectDef {
            name: "Console".into(),
            module: "std::console".into(),
            ops: vec![OpDef {
                name: "print".into(),
                params: vec![Ty::String],
                ret: Ty::Unit,
                fail: None,
            }],
        }],
        ..Module::default()
    }
}

fn console() -> Row {
    Row {
        effects: vec![CONSOLE],
        ..Row::default()
    }
}

fn def(
    name: &str,
    kind: FnKind,
    b: FnBuilder,
    params: Vec<Local>,
    ret: Ty,
    row: Row,
    body: Expr,
) -> FnDef {
    FnDef {
        name: name.into(),
        module: "main".into(),
        kind,
        captures: vec![],
        params,
        locals: b.locals,
        ret,
        row,
        row_vars: vec![],
        ty_params: 0,
        row_params: 0,
        pos: None,
        body,
        refined_params: vec![],
    }
}

fn top() -> FnKind {
    FnKind::Top { export: false }
}

/// `main` that prints `show(f())`.
fn print_result(m: &mut Module, f: DefId, ty: Ty) {
    let mut b = FnBuilder::new();
    let (v, s, r) = (
        b.local("v", ty.clone()),
        b.local("s", Ty::String),
        b.local("r", Ty::Unit),
    );
    let mut blk = Block::new();
    blk.push(v, Bind::Call(Atom::Global(f), vec![]));
    blk.push(s, Bind::Prim(PrimOp::Show(ty), vec![local(v)]));
    blk.push(r, Bind::Perform(CONSOLE, 0, vec![local(s)]));
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs.push(def(
        "main",
        FnKind::Top { export: true },
        b,
        vec![],
        Ty::Unit,
        console(),
        body,
    ));
    m.entry = Some(DefId(m.defs.len() as u32 - 1));
}

fn run(m: &Module) -> sakura_eval::Outcome {
    validate(m).unwrap_or_else(|e| panic!("invalid Core: {e:?}"));
    run_main(m, Limits::default())
}

#[test]
fn hello_world() {
    let mut m = module();
    // fn greet(name) { Console.print("hello, " + name) }
    let mut b = FnBuilder::new();
    let (name, s, r) = (
        b.local("name", Ty::String),
        b.local("s", Ty::String),
        b.local("r", Ty::Unit),
    );
    let mut blk = Block::new();
    blk.push(
        s,
        Bind::Prim(PrimOp::Concat, vec![str_("hello, "), local(name)]),
    );
    blk.push(r, Bind::Perform(CONSOLE, 0, vec![local(s)]));
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs.push(def(
        "greet",
        top(),
        b,
        vec![name],
        Ty::Unit,
        console(),
        body,
    ));
    let mut b = FnBuilder::new();
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(
        r,
        Bind::Call(Atom::Global(DefId(0)), vec![str_("Sakura Lang")]),
    );
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs.push(def(
        "main",
        FnKind::Top { export: true },
        b,
        vec![],
        Ty::Unit,
        console(),
        body,
    ));
    m.entry = Some(DefId(1));
    let out = run(&m);
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.stdout, "hello, Sakura Lang\n");
}

/// `fn countdown(n) { if n == 0 { Console.print("liftoff") } else { countdown(n - 1) } }`
fn countdown(m: &mut Module, from: i64) {
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let (c, r1, n1, r2) = (
        b.local("c", Ty::Bool),
        b.local("r", Ty::Unit),
        b.local("m", Ty::Int),
        b.local("r", Ty::Unit),
    );
    let mut then = Block::new();
    then.push(r1, Bind::Perform(CONSOLE, 0, vec![str_("liftoff")]));
    let mut els = Block::new();
    els.push(n1, Bind::Prim(PrimOp::IntSub, vec![local(n), int(1)]));
    els.push(r2, Bind::Call(Atom::Global(DefId(0)), vec![local(n1)]));
    let mut blk = Block::new();
    blk.push(
        c,
        Bind::Prim(PrimOp::Eq(CmpTy::Int), vec![local(n), int(0)]),
    );
    let body = blk.finish(Expr::If {
        cond: local(c),
        then: Box::new(then.finish(Expr::Ret(local(r1)))),
        els: Box::new(els.finish(Expr::Ret(local(r2)))),
    });
    m.defs.push(def(
        "countdown",
        top(),
        b,
        vec![n],
        Ty::Unit,
        console(),
        body,
    ));
    let mut b = FnBuilder::new();
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(r, Bind::Call(Atom::Global(DefId(0)), vec![int(from)]));
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs.push(def(
        "main",
        FnKind::Top { export: true },
        b,
        vec![],
        Ty::Unit,
        console(),
        body,
    ));
    m.entry = Some(DefId(1));
}

#[test]
fn million_deep_tail_recursion_runs_in_constant_stack() {
    let mut m = module();
    countdown(&mut m, 1_000_000);
    let out = run(&m);
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.stdout, "liftoff\n");
}

#[test]
fn step_limit_stops_runaway_programs() {
    let mut m = module();
    countdown(&mut m, 1_000_000);
    let out = run_main(
        &m,
        Limits {
            max_steps: 1000,
            ..Limits::default()
        },
    );
    assert_eq!(out.result, Err(Stop::Limit("step limit")));
}

/// `effect Counter { fn next() -> Int }` with `handler Counter { var n = 0; fn next() { n = n + 1; n } }`.
fn counter_module() -> (Module, DefId) {
    let mut m = module();
    let counter = EffectId(1);
    m.effects.push(EffectDef {
        name: "Counter".into(),
        module: "main".into(),
        ops: vec![OpDef {
            name: "next".into(),
            params: vec![],
            ret: Ty::Int,
            fail: None,
        }],
    });
    // clause next(inst)
    let mut b = FnBuilder::new();
    let inst = b.local("inst", Ty::Instance(counter));
    let (n, n1, u) = (
        b.local("n", Ty::Int),
        b.local("n1", Ty::Int),
        b.local("u", Ty::Unit),
    );
    let mut blk = Block::new();
    blk.push(n, Bind::StateGet(local(inst), 0));
    blk.push(n1, Bind::Prim(PrimOp::IntAdd, vec![local(n), int(1)]));
    blk.push(u, Bind::StateSet(local(inst), 0, local(n1)));
    let body = blk.finish(Expr::Ret(local(n1)));
    m.defs.push(def(
        "next",
        FnKind::Clause {
            effect: counter,
            op: 0,
        },
        b,
        vec![inst],
        Ty::Int,
        Row::empty(),
        body,
    ));
    // fn counter() -> Handler<Counter>
    let mut b = FnBuilder::new();
    let (c, h) = (b.local("c", Ty::Unit), b.local("h", Ty::Handler(counter)));
    let mut blk = Block::new();
    blk.push(c, Bind::Closure(DefId(0), vec![]));
    blk.push(
        h,
        Bind::MakeHandler {
            effect: counter,
            state: vec![int(0)],
            clauses: vec![local(c)],
        },
    );
    let body = blk.finish(Expr::Ret(local(h)));
    m.defs.push(def(
        "counter",
        top(),
        b,
        vec![],
        Ty::Handler(counter),
        Row::empty(),
        body,
    ));
    (m, DefId(1))
}

fn perform_next(b: &mut FnBuilder, blk: &mut Block) -> Local {
    let l = b.local("x", Ty::Int);
    blk.push(l, Bind::Perform(EffectId(1), 0, vec![]));
    l
}

#[test]
fn handler_state_advances_and_inner_handler_wins() {
    let (mut m, counter) = counter_module();
    // fn answer() { with counter() { next() + with counter() { next() + next() } } }  => 1 + (1 + 2) = 4
    let mut b = FnBuilder::new();
    let (h1, h2) = (
        b.local("h1", Ty::Handler(EffectId(1))),
        b.local("h2", Ty::Handler(EffectId(1))),
    );
    let mut blk = Block::new();
    blk.push(h1, Bind::Call(Atom::Global(counter), vec![]));
    blk.handle(local(h1));
    let a = perform_next(&mut b, &mut blk);
    blk.push(h2, Bind::Call(Atom::Global(counter), vec![]));
    let inner_sum = b.local("inner", Ty::Int);
    let mut inner = Block::new();
    let x = perform_next(&mut b, &mut inner);
    let y = perform_next(&mut b, &mut inner);
    let s = b.local("s", Ty::Int);
    inner.push(s, Bind::Prim(PrimOp::IntAdd, vec![local(x), local(y)]));
    let k = b.label();
    blk.continue_at(k, vec![inner_sum], {
        let mut h = Block::new();
        h.handle(local(h2));
        h.finish(inner.finish(Expr::Jump(k, vec![local(s)])))
    });
    let total = b.local("total", Ty::Int);
    blk.push(
        total,
        Bind::Prim(PrimOp::IntAdd, vec![local(a), local(inner_sum)]),
    );
    let body = blk.finish(Expr::Ret(local(total)));
    m.defs
        .push(def("answer", top(), b, vec![], Ty::Int, Row::empty(), body));
    let answer = DefId(m.defs.len() as u32 - 1);
    print_result(&mut m, answer, Ty::Int);
    let out = run(&m);
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.stdout, "4\n");
}

#[test]
fn try_catches_a_fail_from_a_callee() {
    let mut m = module();
    m.types.push(TypeDef {
        name: "ParseError".into(),
        module: "main".into(),
        params: 0,
        kind: TypeKind::Record {
            fields: vec![Field {
                name: "input".into(),
                ty: Ty::String,
            }],
            error: true,
        },
        builtin: None,
    });
    let pe = Ty::Adt(TypeId(0), vec![]);
    let fail_row = Row {
        fail: Some(Box::new(pe.clone())),
        ..Row::default()
    };
    // fn parse() -> Int ! {Fail<ParseError>} { fail ParseError { input: "x" } }
    let mut b = FnBuilder::new();
    let e = b.local("e", pe.clone());
    let mut blk = Block::new();
    blk.push(e, Bind::Alloc(Alloc::Record(TypeId(0)), vec![str_("x")]));
    let body = blk.finish(Expr::Fail(local(e)));
    m.defs
        .push(def("parse", top(), b, vec![], Ty::Int, fail_row, body));
    // fn caught() -> String { try { parse(); "ok" } catch err => show(err) }
    let mut b = FnBuilder::new();
    let (err, shown, r) = (
        b.local("err", pe.clone()),
        b.local("shown", Ty::String),
        b.local("r", Ty::Int),
    );
    let on_fail = b.label();
    let mut handler = Block::new();
    handler.push(shown, Bind::Prim(PrimOp::Show(pe), vec![local(err)]));
    let mut blk = Block::new();
    blk.join(on_fail, vec![err], handler.finish(Expr::Ret(local(shown))));
    blk.try_(on_fail);
    blk.push(r, Bind::Call(Atom::Global(DefId(0)), vec![]));
    let body = blk.finish(Expr::Ret(str_("ok")));
    m.defs.push(def(
        "caught",
        top(),
        b,
        vec![],
        Ty::String,
        Row::empty(),
        body,
    ));
    print_result(&mut m, DefId(1), Ty::String);
    let out = run(&m);
    assert_eq!(out.result, Ok(()));
    assert_eq!(out.stdout, "ParseError { input: \"x\" }\n");
}

#[test]
fn arithmetic_defects() {
    for (op, a, b, code) in [
        (PrimOp::IntDiv, 1, 0, "D002"),
        (PrimOp::IntRem, 1, 0, "D002"),
        (PrimOp::IntAdd, (1 << 53) - 1, 1, "D003"),
    ] {
        let mut m = module();
        let mut bld = FnBuilder::new();
        let r = bld.local("r", Ty::Int);
        let mut blk = Block::new();
        blk.push(r, Bind::Prim(op, vec![int(a), int(b)]));
        let body = blk.finish(Expr::Ret(local(r)));
        m.defs
            .push(def("f", top(), bld, vec![], Ty::Int, Row::empty(), body));
        print_result(&mut m, DefId(0), Ty::Int);
        validate(&m).unwrap();
        let out = run_main(&m, Limits::default());
        match out.result {
            Err(Stop::Defect { code: c, .. }) if c == code => {}
            other => panic!("{a} {b}: expected {code}, got {other:?}"),
        }
    }
}
