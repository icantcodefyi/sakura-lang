//! Negative cases for the LIR validator: each rule rejects a lowered corpus program after one
//! targeted mutation (plan §3.1 M4-LIR: "a validator that checks balanced ownership on every
//! path"; ABI §3.4 layout agreement, §4.3 niche projection, §7 ids not pointers).

mod common;

use common::{big_stack, named, sigs};
use sakura_lir::lir::*;
use sakura_lir::{lower, validate};

fn lowered(name: &str) -> Module {
    let p = named(&[name])
        .pop()
        .unwrap_or_else(|| panic!("no corpus program {name}"));
    lower::lower(&p.core, &sigs()).unwrap()
}

fn errors(m: &Module) -> Vec<String> {
    validate::validate(m, validate::Options::default())
        .iter()
        .map(|e| e.to_string())
        .collect()
}

/// The first instruction (function, block, index) matching `pred`.
fn find(m: &Module, pred: impl Fn(&Inst) -> bool) -> (usize, usize, usize) {
    for (fi, f) in m.funcs.iter().enumerate() {
        for (bi, b) in f.blocks.iter().enumerate() {
            if let Some(k) = b.insts.iter().position(&pred) {
                return (fi, bi, k);
            }
        }
    }
    panic!("no matching instruction");
}

fn rejected(m: &Module, needle: &str) {
    let es = errors(m);
    assert!(
        es.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}, got {es:?}"
    );
}

#[test]
fn deleting_any_dup_or_drop_is_rejected() {
    big_stack(|| {
        let sigs = sigs();
        let mut checked = 0;
        for p in named(&[
            "closures",
            "effects_and_handlers",
            "handler_callback_context",
            "typed_errors",
            "records",
            "tuples",
            "loops",
            "diff_0001",
        ]) {
            let m = lower::lower(&p.core, &sigs).unwrap();
            assert!(errors(&m).is_empty(), "{}", p.name);
            for (fi, f) in m.funcs.iter().enumerate() {
                for (bi, b) in f.blocks.iter().enumerate() {
                    for (k, i) in b.insts.iter().enumerate() {
                        if !matches!(i, Inst::Dup(_) | Inst::Drop(_)) {
                            continue;
                        }
                        let mut bad = m.clone();
                        bad.funcs[fi].blocks[bi].insts.remove(k);
                        assert!(
                            !errors(&bad).is_empty(),
                            "{}: removing {i:?} from {}#{fi} b{bi} still validates",
                            p.name,
                            f.name
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 100, "only {checked} mutations");
    });
}

#[test]
fn a_reference_dropped_twice_is_rejected() {
    let mut m = lowered("closures");
    let (fi, bi, k) = find(&m, |i| matches!(i, Inst::Drop(_)));
    let d = m.funcs[fi].blocks[bi].insts[k].clone();
    m.funcs[fi].blocks[bi].insts.insert(k, d);
    rejected(&m, "used after it was consumed or dropped");
}

#[test]
fn a_reference_left_live_at_a_terminator_is_rejected() {
    let mut m = lowered("closures");
    let (fi, bi, k) = find(&m, |i| matches!(i, Inst::Drop(_)));
    m.funcs[fi].blocks[bi].insts[k] = Inst::Dup(match m.funcs[fi].blocks[bi].insts[k] {
        Inst::Drop(v) => v,
        _ => unreachable!(),
    });
    rejected(&m, "unbalanced ownership");
}

#[test]
fn a_value_from_another_block_is_out_of_scope() {
    let mut m = lowered("variants_and_match");
    // Use an entry parameter of some function inside its second block.
    let fi = m
        .funcs
        .iter()
        .position(|f| {
            f.blocks.len() > 1 && !f.params().is_empty() && f.repr(f.params()[0]).is_managed()
        })
        .unwrap();
    let p = m.funcs[fi].params()[0];
    m.funcs[fi].blocks[1].insts.insert(0, Inst::Dup(p));
    rejected(&m, "is not in scope in this block");
}

#[test]
fn an_alloc_disagreeing_with_the_layout_is_rejected() {
    let mut m = lowered("records");
    let (fi, bi, k) = find(&m, |i| {
        matches!(
            i,
            Inst::Alloc {
                shape: Shape::Record(_),
                ..
            }
        )
    });
    if let Inst::Alloc { args, .. } = &mut m.funcs[fi].blocks[bi].insts[k] {
        args.pop();
    }
    rejected(&m, "slots");
}

#[test]
fn a_projection_with_the_wrong_representation_is_rejected() {
    let mut m = lowered("records");
    let (fi, bi, k) = find(&m, |i| {
        matches!(
            i,
            Inst::Proj {
                shape: Shape::Record(_),
                ..
            }
        )
    });
    let Inst::Proj { dst, .. } = m.funcs[fi].blocks[bi].insts[k] else {
        unreachable!()
    };
    let r = m.funcs[fi].vars[dst.index()].repr;
    m.funcs[fi].vars[dst.index()].repr = if r == Repr::F64 { Repr::I64 } else { Repr::F64 };
    assert!(!errors(&m).is_empty());
}

#[test]
fn an_activation_id_never_enters_an_ordinary_value() {
    let mut m = lowered("handler_state");
    let fi = m
        .funcs
        .iter()
        .position(|f| matches!(f.kind, FuncKind::Clause { .. }))
        .unwrap();
    let id = m.funcs[fi].params()[1];
    assert_eq!(m.funcs[fi].repr(id), Repr::Id);
    let dst = Var(m.funcs[fi].vars.len() as u32);
    m.funcs[fi].vars.push(VarDecl {
        name: "leak".into(),
        repr: Repr::Ptr,
    });
    m.funcs[fi].blocks[0].insts.insert(
        0,
        Inst::Alloc {
            dst,
            shape: Shape::List,
            args: vec![id],
            reuse: None,
        },
    );
    m.funcs[fi].blocks[0].insts.insert(1, Inst::Drop(dst));
    rejected(&m, "ids, never pointers");
}

#[test]
fn leaving_a_with_region_without_closing_it_is_rejected() {
    let mut m = lowered("handler_state");
    let (fi, bi, k) = find(&m, |i| matches!(i, Inst::HandlePop));
    m.funcs[fi].blocks[bi].insts.remove(k);
    assert!(!errors(&m).is_empty());
}

#[test]
fn a_raw_option_payload_projection_is_rejected_under_niche_layout() {
    let m = lowered("generics");
    let opt = m.layouts.builtin("Option").unwrap();
    let uses_payload = m.funcs.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i, Inst::Proj { shape: Shape::Variant(l, 1), .. } if *l == opt))
        })
    });
    assert!(uses_payload, "the program projects an Option payload");
    assert!(errors(&m).is_empty());
    let niche = validate::validate(&m, validate::Options { niche: true });
    assert!(
        niche.iter().any(|e| e.message.contains("niche layout")),
        "{niche:?}"
    );
}

#[test]
fn a_call_with_the_wrong_convention_is_rejected() {
    let mut m = lowered("closures");
    // Point a direct call at a lambda: lambdas are only called through closures.
    let lambda = m
        .funcs
        .iter()
        .position(|f| matches!(f.kind, FuncKind::Lambda))
        .unwrap();
    'out: for f in &mut m.funcs {
        for b in &mut f.blocks {
            if let Term::Call {
                callee: callee @ Callee::Direct(_),
                ..
            } = &mut b.term
            {
                *callee = Callee::Direct(FuncId(lambda as u32));
                break 'out;
            }
        }
    }
    rejected(&m, "is called through closures only");
}

#[test]
fn handler_state_is_checked_against_its_row() {
    // §17.108 E6 / plan §3.6 L1a: `Counter`'s state is an `i64` slot of its handler row.
    let m = lowered("handler_state");
    let (fi, bi, k) = find(&m, |i| matches!(i, Inst::StateGet { .. }));
    let Inst::StateGet { dst, row, .. } = m.funcs[fi].blocks[bi].insts[k] else {
        unreachable!()
    };
    assert_eq!(
        m.funcs[fi].repr(dst),
        Repr::I64,
        "Int state is read unboxed"
    );
    assert_eq!(
        m.layouts.entry(row).kind,
        sakura_lir::layout::EntryKind::Tuple
    );

    let mut bad = m.clone();
    bad.funcs[fi].vars[dst.index()].repr = Repr::F64;
    rejected(&bad, "expected i64");

    let mut bad = m.clone();
    let (fi, bi, k) = find(&bad, |i| matches!(i, Inst::StateSet { .. }));
    if let Inst::StateSet { index, .. } = &mut bad.funcs[fi].blocks[bi].insts[k] {
        *index = 99;
    }
    rejected(&bad, "handler state slot 99 out of range");

    let mut bad = m.clone();
    let (fi, bi, k) = find(&bad, |i| matches!(i, Inst::MakeHandler { .. }));
    let option = bad.layouts.builtin("Option").unwrap();
    if let Inst::MakeHandler { row, .. } = &mut bad.funcs[fi].blocks[bi].insts[k] {
        *row = option;
    }
    rejected(&bad, "is not a state shape row");
}

#[test]
fn a_clause_follows_its_operations_signature() {
    let mut m = lowered("handler_state");
    let fi = m
        .funcs
        .iter()
        .position(|f| matches!(f.kind, FuncKind::Clause { .. }))
        .unwrap();
    m.funcs[fi].ret = Repr::F64;
    rejected(&m, "its operation");
}

#[test]
fn reuse_tokens_are_linear_and_cross_only_direct_calls() {
    big_stack(|| {
        // Plan M5 N3: `reuse_constructor` lowers with constructor-site reuse.
        let m = lowered("reuse_constructor");
        assert!(errors(&m).is_empty(), "{:?}", errors(&m));
        let (fi, bi, k) = find(&m, |i| matches!(i, Inst::Alloc { reuse: Some(_), .. }));
        let Inst::Alloc { reuse: Some(t), .. } = m.funcs[fi].blocks[bi].insts[k] else {
            unreachable!()
        };

        // Never consumed.
        let mut bad = m.clone();
        if let Inst::Alloc { reuse, .. } = &mut bad.funcs[fi].blocks[bi].insts[k] {
            *reuse = None;
        }
        rejected(&bad, "outlives its block");

        // Read as an ordinary operand.
        let mut bad = m.clone();
        bad.funcs[fi].blocks[bi].insts.insert(k, Inst::Dup(t));
        rejected(&bad, "is an operand");

        // Given to an allocation of another constructor.
        let mut bad = m.clone();
        if let Inst::Alloc { shape, .. } = &mut bad.funcs[fi].blocks[bi].insts[k] {
            *shape = match &*shape {
                Shape::Variant(l, c) => Shape::Variant(*l, *c + 1),
                Shape::Record(l) => Shape::Tuple(*l),
                other => other.clone(),
            };
        }
        rejected(&bad, "is not a reuse token of this block for this shape");

        // `ins`'s recursive arm carries its token across the call of `ins` to the `Alloc` in
        // the call's continuation; across a call that may suspend it is rejected.
        let (fi, bi) = m
            .funcs
            .iter()
            .enumerate()
            .find_map(|(fi, f)| {
                f.blocks
                    .iter()
                    .position(|b| {
                        matches!(&b.term, Term::Call { cont, .. }
                        if f.blocks[cont.block.index()].insts.iter().any(|i| {
                            matches!(i, Inst::Alloc { reuse: Some(t), .. }
                                if f.blocks[cont.block.index()].params.contains(t))
                        }))
                    })
                    .map(|bi| (fi, bi))
            })
            .expect("a token across a call");
        assert_eq!(m.funcs[fi].name, "ins");
        // `main` performs `Console.print`, so it is not direct.
        let main = m.funcs.iter().position(|f| f.name == "main").unwrap();
        let mut bad = m.clone();
        if let Term::Call { callee, .. } = &mut bad.funcs[fi].blocks[bi].term {
            *callee = Callee::Direct(FuncId(main as u32));
        }
        rejected(&bad, "crosses a call that may suspend");
    });
}

/// Plan M5 §4: the `m5-stress` set runs with reuse stripped, so every corpus program must still
/// validate without it, with no reset or reusing allocation left.
#[test]
fn every_corpus_program_validates_with_reuse_stripped() {
    big_stack(|| {
        for p in common::programs() {
            let mut m = lower::lower(&p.core, &sigs()).unwrap();
            for f in &mut m.funcs {
                sakura_lir::reuse::strip(f);
            }
            assert_eq!(errors(&m), Vec::<String>::new(), "{}", p.name);
            let left = m
                .funcs
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| {
                    matches!(i, Inst::Builtin { name, .. } if name == sakura_lir::reuse::RESET)
                        || matches!(i, Inst::Alloc { reuse: Some(_), .. })
                });
            assert!(!left, "{}: reuse left after strip", p.name);
        }
    });
}
