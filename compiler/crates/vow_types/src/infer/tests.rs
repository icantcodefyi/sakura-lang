use std::collections::BTreeSet;

use proptest::prelude::*;

use super::*;
use crate::ty::test_def;

fn names(d: DefId) -> String {
    format!("E{}", d.index.into_raw().into_u32())
}

fn label(i: u32) -> Label {
    Label {
        effect: Effect::Def(test_def(i)),
        args: Vec::new(),
    }
}

fn fail(inf: &mut Infer, tags: &[u32]) -> Label {
    let e = inf
        .interner
        .error_union(tags.iter().map(|t| test_def(100 + t)).collect());
    Label {
        effect: Effect::Fail,
        args: vec![e],
    }
}

fn row(inf: &mut Infer, labels: &[u32], tail: Tail) -> RowId {
    let labels = labels.iter().map(|i| label(*i)).collect();
    inf.make_row(labels, tail)
}

fn var_tail(inf: &mut Infer) -> Tail {
    Tail::Var(inf.fresh_row_var())
}

fn show_row(inf: &mut Infer, r: RowId) -> String {
    inf.render_row(r, &names)
}

#[test]
fn unify_primitives_and_vars() {
    let mut inf = Infer::new();
    let int = inf.interner.prim(TyCon::Int);
    let string = inf.interner.prim(TyCon::String);
    assert!(inf.unify(int, int).is_ok());
    assert_eq!(
        inf.unify(int, string),
        Err(UnifyError::Types {
            actual: int,
            expected: string
        })
    );
    let a = inf.fresh_var();
    let list_a = inf.interner.con(TyCon::List, vec![a]);
    let list_int = inf.interner.con(TyCon::List, vec![int]);
    assert!(inf.unify(list_a, list_int).is_ok());
    assert_eq!(inf.zonk(a), int);
}

#[test]
fn occurs_check_rejects_infinite_types() {
    let mut inf = Infer::new();
    let a = inf.fresh_var();
    let list_a = inf.interner.con(TyCon::List, vec![a]);
    assert!(matches!(
        inf.unify(a, list_a),
        Err(UnifyError::Occurs { .. })
    ));
}

#[test]
fn error_type_absorbs_mismatches() {
    let mut inf = Infer::new();
    let err = inf.interner.error();
    let int = inf.interner.prim(TyCon::Int);
    assert!(inf.unify(err, int).is_ok());
    assert!(inf.unify(int, err).is_ok());
}

#[test]
fn function_arity_mismatch() {
    let mut inf = Infer::new();
    let int = inf.interner.prim(TyCon::Int);
    let pure = inf.interner.empty_row();
    let f1 = inf.interner.func(vec![int], int, pure);
    let f2 = inf.interner.func(vec![int, int], int, pure);
    assert!(inf.unify(f1, f2).is_err());
}

#[test]
fn rows_with_distinct_tails_meet_in_a_fresh_tail() {
    // §5.2 rule 1: {A | r} ~ {B | s} gives r := {B | t}, s := {A | t}.
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let s = var_tail(&mut inf);
    let a = row(&mut inf, &[1], r);
    let b = row(&mut inf, &[2], s);
    inf.unify_rows(a, b).unwrap();
    let fa = inf.flatten_row(a);
    let fb = inf.flatten_row(b);
    assert_eq!(fa, fb);
    assert_eq!(fa.labels.len(), 2);
    assert!(matches!(fa.tail, Tail::Var(_)));
}

#[test]
fn open_row_against_empty_reports_the_label() {
    // §5.2 rule 2.
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let a = row(&mut inf, &[1], r);
    let empty = inf.interner.empty_row();
    match inf.unify_rows(a, empty) {
        Err(UnifyError::Rows { extra, .. }) => assert_eq!(extra, vec![label(1)]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn same_tail_with_different_labels_fails() {
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let a = row(&mut inf, &[1], r);
    let b = row(&mut inf, &[2], r);
    assert!(inf.unify_rows(a, b).is_err());
}

#[test]
fn rigid_tails_must_match() {
    let mut inf = Infer::new();
    let e = Tail::Param(inf.interner.new_row_param("e"));
    let f = Tail::Param(inf.interner.new_row_param("f"));
    let a = row(&mut inf, &[], e);
    let b = row(&mut inf, &[], f);
    assert!(inf.unify_rows(a, a).is_ok());
    assert!(matches!(
        inf.unify_rows(a, b),
        Err(UnifyError::Tails { .. })
    ));
    let r = var_tail(&mut inf);
    let flex = row(&mut inf, &[], r);
    let rigid = row(&mut inf, &[3], e);
    inf.unify_rows(flex, rigid).unwrap();
    assert_eq!(show_row(&mut inf, flex), "{E3 | e}");
}

#[test]
fn same_label_unifies_arguments() {
    // §5.2 rule 3: State<Int> ~ State<?a> binds ?a; State<Int> ~ State<String> fails.
    let mut inf = Infer::new();
    let int = inf.interner.prim(TyCon::Int);
    let string = inf.interner.prim(TyCon::String);
    let a = inf.fresh_var();
    let st = |t| Label {
        effect: Effect::Def(test_def(7)),
        args: vec![t],
    };
    let r1 = inf.closed_row(vec![st(int)]);
    let r2 = inf.closed_row(vec![st(a)]);
    let r3 = inf.closed_row(vec![st(string)]);
    inf.unify_rows(r1, r2).unwrap();
    assert_eq!(inf.zonk(a), int);
    assert!(inf.unify_rows(r1, r3).is_err());
}

#[test]
fn row_occurs_check() {
    // r ~ {E<fn() -> Unit ! r>} would be infinite.
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let tail_row = row(&mut inf, &[], r);
    let unit = inf.interner.unit();
    let f = inf.interner.func(vec![], unit, tail_row);
    let lab = Label {
        effect: Effect::Def(test_def(5)),
        args: vec![f],
    };
    let big = inf.closed_row(vec![lab]);
    assert!(matches!(
        inf.unify_rows(tail_row, big),
        Err(UnifyError::Tails { .. })
    ));
}

#[test]
fn union_joins_fail_and_unifies_tails() {
    let mut inf = Infer::new();
    let fa = fail(&mut inf, &[1]);
    let fb = fail(&mut inf, &[2]);
    let r1 = inf.closed_row(vec![label(1), fa]);
    let r2 = inf.closed_row(vec![fb]);
    let u = inf.union_rows(r1, r2).unwrap();
    assert_eq!(show_row(&mut inf, u), "{E1, Fail<E101 | E102>}");
    let t = var_tail(&mut inf);
    let open = row(&mut inf, &[2], t);
    let u2 = inf.union_rows(u, open).unwrap();
    assert!(matches!(inf.flatten_row(u2).tail, Tail::Var(_)));
}

#[test]
fn closed_rows_widen_into_flexible_rows() {
    // §5.2 rule 4: fn() -> Int ! {Clock} passed where fn() -> Int ! ?e is expected.
    let mut inf = Infer::new();
    let clock = inf.closed_row(vec![label(1)]);
    let e = var_tail(&mut inf);
    let flex = row(&mut inf, &[], e);
    inf.subsume_rows(clock, flex).unwrap();
    inf.default_row(flex);
    assert_eq!(show_row(&mut inf, flex), "{E1}");
}

#[test]
fn subset_against_closed_rows() {
    let mut inf = Infer::new();
    let small = inf.closed_row(vec![label(1)]);
    let big = inf.closed_row(vec![label(1), label(2)]);
    assert!(inf.subsume_rows(small, big).is_ok());
    assert_eq!(inf.subsume_rows(big, small), Err((vec![label(2)], false)));
}

#[test]
fn fail_subsumption_is_union_inclusion() {
    let mut inf = Infer::new();
    let fa = fail(&mut inf, &[1]);
    let fab = fail(&mut inf, &[1, 2]);
    let ra = inf.closed_row(vec![fa.clone()]);
    let rab = inf.closed_row(vec![fab.clone()]);
    assert!(inf.subsume_rows(ra, rab).is_ok());
    assert!(inf.subsume_rows(rab, ra).is_err());
    // Unification (synthesis mode) requires equal unions.
    assert!(inf.unify_rows(ra, rab).is_err());
}

#[test]
fn never_and_refinements_subsume() {
    let mut inf = Infer::new();
    let never = inf.interner.never();
    let int = inf.interner.prim(TyCon::Int);
    let string = inf.interner.prim(TyCon::String);
    assert!(inf.subsume(never, int).is_ok());
    assert!(inf.unify(never, int).is_err());
    let email = inf.interner.con(TyCon::Adt(test_def(42)), vec![]);
    inf.refined_bases.insert(test_def(42), string);
    assert!(inf.subsume(email, string).is_ok());
    assert!(inf.subsume(string, email).is_err());
    let lists = (
        inf.interner.con(TyCon::List, vec![email]),
        inf.interner.con(TyCon::List, vec![string]),
    );
    assert!(inf.subsume(lists.0, lists.1).is_ok());
}

#[test]
fn generalize_only_inner_level_vars() {
    let mut inf = Infer::new();
    let outer = inf.fresh_var();
    inf.enter_level();
    let a = inf.fresh_var();
    let pure = inf.interner.empty_row();
    let tuple = inf.interner.intern(Type::Tuple(vec![a, outer]));
    let f = inf.interner.func(vec![a], tuple, pure);
    inf.exit_level();
    let s = inf.generalize(f);
    assert_eq!(s.params.len(), 1);
    let (i1, _) = inf.instantiate(&s);
    let (i2, _) = inf.instantiate(&s);
    assert_ne!(i1, i2);
    assert_eq!(inf.render(f, &names), "fn(?T1) -> (?T1, ?T0)");
    assert_eq!(inf.render(s.ty, &names), "fn(A) -> (A, ?T0)");
}

#[test]
fn unifying_with_outer_var_lowers_level() {
    // A var unified with an outer-level var must not be generalized.
    let mut inf = Infer::new();
    let outer = inf.fresh_var();
    inf.enter_level();
    let inner = inf.fresh_var();
    inf.unify(inner, outer).unwrap();
    let list = inf.interner.con(TyCon::List, vec![inner]);
    inf.exit_level();
    assert!(inf.generalize(list).params.is_empty());
}

#[test]
fn row_vars_generalize() {
    let mut inf = Infer::new();
    inf.enter_level();
    let r = var_tail(&mut inf);
    let tail_row = row(&mut inf, &[1], r);
    let unit = inf.interner.unit();
    let f = inf.interner.func(vec![], unit, tail_row);
    inf.exit_level();
    let s = inf.generalize(f);
    assert_eq!(s.row_params.len(), 1);
    assert_eq!(inf.render(s.ty, &names), "fn() -> Unit ! {E1 | e1}");
}

#[test]
fn snapshot_rolls_back_bindings() {
    let mut inf = Infer::new();
    let a = inf.fresh_var();
    let int = inf.interner.prim(TyCon::Int);
    let snap = inf.snapshot();
    inf.unify(a, int).unwrap();
    inf.rollback(snap);
    assert!(inf.is_unbound_var(a));
}

// ---- row laws (design §14.5) ----

#[derive(Clone, Debug)]
struct RowSpec {
    labels: BTreeSet<u32>,
    /// 0 closed, 1 shared var x, 2 shared var y, 3 own fresh var, 4 rigid e.
    tail: u8,
}

fn row_spec() -> impl Strategy<Value = RowSpec> {
    (prop::collection::btree_set(0u32..5, 0..4), 0u8..5)
        .prop_map(|(labels, tail)| RowSpec { labels, tail })
}

struct Env {
    inf: Infer,
    x: Tail,
    y: Tail,
    e: Tail,
}

fn env() -> Env {
    let mut inf = Infer::new();
    let x = Tail::Var(inf.fresh_row_var());
    let y = Tail::Var(inf.fresh_row_var());
    let e = Tail::Param(inf.interner.new_row_param("e"));
    Env { inf, x, y, e }
}

fn build(env: &mut Env, spec: &RowSpec) -> RowId {
    let tail = match spec.tail {
        0 => Tail::Closed,
        1 => env.x,
        2 => env.y,
        3 => Tail::Var(env.inf.fresh_row_var()),
        _ => env.e,
    };
    let labels: Vec<u32> = spec.labels.iter().copied().collect();
    row(&mut env.inf, &labels, tail)
}

fn label_set(inf: &mut Infer, r: RowId) -> (BTreeSet<Effect>, Tail) {
    let f = inf.flatten_row(r);
    (f.labels.iter().map(|l| l.effect).collect(), f.tail)
}

proptest! {
    #[test]
    fn unify_is_commutative(a in row_spec(), b in row_spec()) {
        let mut e1 = env();
        let (ra, rb) = (build(&mut e1, &a), build(&mut e1, &b));
        let ok1 = e1.inf.unify_rows(ra, rb).is_ok();
        let mut e2 = env();
        let (ra2, rb2) = (build(&mut e2, &a), build(&mut e2, &b));
        let ok2 = e2.inf.unify_rows(rb2, ra2).is_ok();
        prop_assert_eq!(ok1, ok2);
        if ok1 {
            prop_assert_eq!(label_set(&mut e1.inf, ra).0, label_set(&mut e2.inf, ra2).0);
        }
    }

    #[test]
    fn unify_solution_equates_rows(a in row_spec(), b in row_spec()) {
        let mut env = env();
        let (ra, rb) = (build(&mut env, &a), build(&mut env, &b));
        if env.inf.unify_rows(ra, rb).is_ok() {
            prop_assert_eq!(label_set(&mut env.inf, ra), label_set(&mut env.inf, rb));
        }
    }

    #[test]
    fn unify_with_self_succeeds(a in row_spec()) {
        let mut env = env();
        let ra = build(&mut env, &a);
        prop_assert!(env.inf.unify_rows(ra, ra).is_ok());
    }

    #[test]
    fn union_is_idempotent_and_commutative(a in row_spec(), b in row_spec()) {
        let mut env = env();
        let ra = build(&mut env, &a);
        let aa = env.inf.union_rows(ra, ra).unwrap();
        prop_assert_eq!(label_set(&mut env.inf, aa), label_set(&mut env.inf, ra));

        let mut e1 = env_pair(&a, &b);
        let mut e2 = env_pair(&a, &b);
        let u1 = e1.0.inf.union_rows(e1.1, e1.2);
        let u2 = e2.0.inf.union_rows(e2.2, e2.1);
        prop_assert_eq!(u1.is_ok(), u2.is_ok());
        if let (Ok(u1), Ok(u2)) = (u1, u2) {
            prop_assert_eq!(label_set(&mut e1.0.inf, u1).0, label_set(&mut e2.0.inf, u2).0);
        }
    }

    #[test]
    fn subset_is_reflexive_and_union_contains_parts(a in row_spec(), b in row_spec()) {
        let (mut env, ra, rb) = env_pair(&a, &b);
        prop_assert!(env.inf.subsume_rows(ra, ra).is_ok());
        if let Ok(u) = env.inf.union_rows(ra, rb) {
            let (la, _) = label_set(&mut env.inf, ra);
            let (lu, _) = label_set(&mut env.inf, u);
            prop_assert!(la.is_subset(&lu));
        }
    }

    #[test]
    fn occurs_check_always_rejects(labels in prop::collection::btree_set(0u32..5, 0..4)) {
        let mut env = env();
        let x = env.x;
        let tail_row = row(&mut env.inf, &[], x);
        let unit = env.inf.interner.unit();
        let f = env.inf.interner.func(vec![], unit, tail_row);
        let mut ls: Vec<Label> = labels.iter().map(|i| label(*i)).collect();
        ls.push(Label { effect: Effect::Def(test_def(99)), args: vec![f] });
        let big = env.inf.make_row(ls, Tail::Closed);
        prop_assert!(env.inf.unify_rows(tail_row, big).is_err());
        prop_assert!(env.inf.unify_rows(big, tail_row).is_err());
    }
}

fn env_pair(a: &RowSpec, b: &RowSpec) -> (Env, RowId, RowId) {
    let mut env = env();
    let ra = build(&mut env, a);
    let rb = build(&mut env, b);
    (env, ra, rb)
}
