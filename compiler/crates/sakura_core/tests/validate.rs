//! Validator and printer tests over hand-built Core.

use sakura_core::build::*;
use sakura_core::*;

fn console() -> EffectDef {
    EffectDef {
        name: "Console".into(),
        module: "std::effects".into(),
        ops: vec![OpDef {
            name: "print".into(),
            params: vec![Ty::String],
            ret: Ty::Unit,
            fail: None,
        }],
    }
}

fn top(name: &str, b: FnBuilder, params: Vec<Local>, ret: Ty, row: Row, body: Expr) -> FnDef {
    FnDef {
        name: name.into(),
        module: "main".into(),
        kind: FnKind::Top { export: false },
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

/// `fn greet(name) { Console.print("hello, {name}") }` and `main() { greet("Sakura Lang") }`.
fn hello() -> Module {
    let mut m = Module {
        effects: vec![console()],
        ..Module::default()
    };
    let row = Row {
        effects: vec![EffectId(0)],
        ..Row::default()
    };

    let mut b = FnBuilder::new();
    let name = b.local("name", Ty::String);
    let shown = b.local("t", Ty::String);
    let line = b.local("line", Ty::String);
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(
        shown,
        Bind::Prim(PrimOp::Show(Ty::String), vec![local(name)]),
    );
    blk.push(
        line,
        Bind::Prim(PrimOp::Concat, vec![str_("hello, "), local(shown)]),
    );
    blk.push(r, Bind::Perform(EffectId(0), 0, vec![local(line)]));
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs
        .push(top("greet", b, vec![name], Ty::Unit, row.clone(), body));

    let mut b = FnBuilder::new();
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(
        r,
        Bind::Call(Atom::Global(DefId(0)), vec![str_("Sakura Lang")]),
    );
    let body = blk.finish(Expr::Ret(local(r)));
    m.defs.push(top("main", b, vec![], Ty::Unit, row, body));
    m.entry = Some(DefId(1));
    m
}

#[test]
fn hello_world_is_valid() {
    let m = hello();
    assert_eq!(validate(&m), Ok(()));
    insta::assert_snapshot!(m.to_string());
}

/// A loop: `fn count(n) { var i = 0; while i < n { i = i + 1 }; i }`.
#[test]
fn loop_with_join_point_is_valid() {
    let mut b = FnBuilder::new();
    let n = b.local("n", Ty::Int);
    let i = b.local("i", Ty::Int);
    let c = b.local("c", Ty::Bool);
    let i2 = b.local("i", Ty::Int);
    let lp = b.label();
    let mut inner = Block::new();
    inner.push(
        c,
        Bind::Prim(PrimOp::Lt(CmpTy::Int), vec![local(i), local(n)]),
    );
    let mut step = Block::new();
    step.push(i2, Bind::Prim(PrimOp::IntAdd, vec![local(i), int(1)]));
    let join = inner.finish(Expr::If {
        cond: local(c),
        then: Box::new(step.finish(Expr::Jump(lp, vec![local(i2)]))),
        els: Box::new(Expr::Ret(local(i))),
    });
    let body = Expr::LetJoin {
        label: lp,
        params: vec![i],
        join: Box::new(join),
        body: Box::new(Expr::Jump(lp, vec![int(0)])),
    };
    let m = Module {
        defs: vec![top("count", b, vec![n], Ty::Int, Row::empty(), body)],
        ..Module::default()
    };
    assert_eq!(validate(&m), Ok(()));
    insta::assert_snapshot!(m.to_string());
}

fn errors(m: &Module) -> Vec<String> {
    validate(m)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn rejects_scope_arity_and_projection_errors() {
    let mut m = hello();
    // Wrong perform arity and an out-of-scope local.
    let mut b = FnBuilder::new();
    let x = b.local("x", Ty::Unit);
    let ghost = b.local("ghost", Ty::Int);
    let mut blk = Block::new();
    blk.push(x, Bind::Perform(EffectId(0), 0, vec![]));
    let body = blk.finish(Expr::Ret(local(ghost)));
    m.defs
        .push(top("bad", b, vec![], Ty::Unit, Row::empty(), body));
    // Payload projection without a tag test, and a non-exhaustive switch.
    m.types.push(TypeDef {
        name: "Option".into(),
        module: "std::core".into(),
        params: 1,
        kind: TypeKind::Variant {
            ctors: vec![
                Ctor {
                    name: "None".into(),
                    fields: vec![],
                },
                Ctor {
                    name: "Some".into(),
                    fields: vec![Ty::Var(0)],
                },
            ],
        },
        builtin: Some("Option".into()),
    });
    let opt = Ty::Adt(TypeId(0), vec![Ty::Int]);
    let mut b = FnBuilder::new();
    let o = b.local("o", opt);
    let v = b.local("v", Ty::Int);
    let mut blk = Block::new();
    blk.push(v, Bind::Proj(local(o), Proj::Payload(TypeId(0), 1, 0)));
    let body = blk.finish(Expr::Switch {
        scrut: local(o),
        on: SwitchOn::Variant(TypeId(0)),
        arms: vec![(1, Expr::Ret(local(v)))],
        default: None,
    });
    m.defs
        .push(top("unwrap", b, vec![o], Ty::Int, Row::empty(), body));
    // Jump to an undefined label; call with the wrong arity.
    let mut b = FnBuilder::new();
    let r = b.local("r", Ty::Unit);
    let mut blk = Block::new();
    blk.push(r, Bind::Call(Atom::Global(DefId(0)), vec![]));
    let body = blk.finish(Expr::Jump(Label(7), vec![]));
    m.defs
        .push(top("jumpy", b, vec![], Ty::Unit, Row::empty(), body));

    insta::assert_debug_snapshot!(errors(&m));
}

#[test]
fn rejects_double_binding() {
    let mut b = FnBuilder::new();
    let x = b.local("x", Ty::Int);
    let mut blk = Block::new();
    blk.push(x, Bind::Atom(int(1)));
    blk.push(x, Bind::Atom(int(2)));
    let body = blk.finish(Expr::Ret(local(x)));
    let m = Module {
        defs: vec![top("twice", b, vec![], Ty::Int, Row::empty(), body)],
        ..Module::default()
    };
    assert_eq!(errors(&m), vec!["def #0: local %0 is bound twice"]);
}

/// Design §17.194: type and row parameters are numbered per def, below its counts; an
/// interner index (`Var(184)`, tail `56`) is rejected.
#[test]
fn rejects_parameters_beyond_the_defs_numbering() {
    let def = |var: u32, tail: u32| {
        let f = Ty::Fn(Box::new(FnTy {
            params: vec![Ty::Var(var)],
            ret: Ty::Var(var),
            row: Row {
                tail: Some(tail),
                ..Row::default()
            },
        }));
        let mut b = FnBuilder::new();
        let x = b.local("x", Ty::Var(var));
        let g = b.local("f", f);
        let r = b.local("r", Ty::Var(var));
        let mut blk = Block::new();
        blk.push(r, Bind::Call(local(g), vec![local(x)]));
        let body = blk.finish(Expr::Ret(local(r)));
        let row = Row {
            tail: Some(tail),
            ..Row::default()
        };
        let mut d = top("apply", b, vec![x, g], Ty::Var(var), row, body);
        d.row_vars = vec![tail];
        d.ty_params = 1;
        d.row_params = 1;
        Module {
            defs: vec![d],
            ..Module::default()
        }
    };
    assert_eq!(errors(&def(0, 0)), Vec::<String>::new());
    let errs = errors(&def(184, 56));
    assert!(
        errs.contains(
            &"def #0: type variable T184 is not below the def's 1 type parameters".into()
        ),
        "{errs:?}"
    );
    assert!(
        errs.contains(&"def #0: row variable r56 is not below the def's 1 row parameters".into()),
        "{errs:?}"
    );
}
