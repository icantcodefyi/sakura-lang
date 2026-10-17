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
fn same_tail_with_different_labels_needs_both_in_the_tail() {
    // Labels are a set: `{E1 | r} ~ {E2 | r}` holds iff `r` contains E1 and E2.
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let a = row(&mut inf, &[1], r);
    let b = row(&mut inf, &[2], r);
    inf.unify_rows(a, b).unwrap();
    inf.default_row(a);
    assert_eq!(show_row(&mut inf, a), "{E1, E2}");
    assert_eq!(show_row(&mut inf, b), "{E1, E2}");
    // …unless a bound forbids it.
    let r = var_tail(&mut inf);
    let open = row(&mut inf, &[], r);
    let only1 = inf.closed_row(vec![label(1)]);
    inf.subsume_rows(open, only1).unwrap();
    let a = row(&mut inf, &[1], r);
    let b = row(&mut inf, &[2], r);
    assert!(inf.unify_rows(a, b).is_err());
}

#[test]
fn rigid_tails_must_match() {
    let mut inf = Infer::new();
    let e = Tail::Param(inf.interner.new_row_param("e", false));
    let f = Tail::Param(inf.interner.new_row_param("f", false));
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
    inf.default_row(flex);
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
fn closed_fail_sets_must_be_equal_in_unify() {
    let mut inf = Infer::new();
    let fa = fail(&mut inf, &[1]);
    let fb = fail(&mut inf, &[2]);
    let ra = inf.closed_row(vec![fa.clone()]);
    let ra2 = inf.closed_row(vec![fa]);
    let rb = inf.closed_row(vec![fb]);
    assert!(inf.unify_rows(ra, ra2).is_ok());
    assert!(inf.unify_rows(ra, rb).is_err());
}

#[test]
fn fail_lower_bounds_join() {
    // §5.2 rule 8: two callbacks sharing one open row join their tags, in either order.
    for order in [[1, 2], [2, 1]] {
        let mut inf = Infer::new();
        let open = inf.fresh_row();
        for t in order {
            let l = fail(&mut inf, &[t]);
            let r = inf.closed_row(vec![l]);
            inf.subsume_rows(r, open).unwrap();
        }
        inf.default_row(open);
        assert_eq!(show_row(&mut inf, open), "{Fail<E101 | E102>}");
    }
}

#[test]
fn fail_unify_through_variables_joins() {
    let mut inf = Infer::new();
    let (r, s) = (inf.fresh_row(), inf.fresh_row());
    let (fa, fb) = (fail(&mut inf, &[1]), fail(&mut inf, &[2]));
    let (ra, rb) = (inf.closed_row(vec![fa]), inf.closed_row(vec![fb]));
    inf.subsume_rows(ra, r).unwrap();
    inf.subsume_rows(rb, s).unwrap();
    inf.unify_rows(r, s).unwrap();
    inf.default_row(r);
    inf.default_row(s);
    assert_eq!(show_row(&mut inf, r), "{Fail<E101 | E102>}");
    assert_eq!(show_row(&mut inf, s), "{Fail<E101 | E102>}");
}

#[test]
fn fail_upper_bound_rejects_later_tags() {
    let mut inf = Infer::new();
    let open = inf.fresh_row();
    let fa = fail(&mut inf, &[1]);
    let only_a = inf.closed_row(vec![fa]);
    inf.subsume_rows(open, only_a).unwrap();
    let fb = fail(&mut inf, &[2]);
    let rb = inf.closed_row(vec![fb]);
    let (extra, _) = inf.subsume_rows(rb, open).unwrap_err();
    assert_eq!(extra.len(), 1);
    let shown = inf.render_label(&extra[0], &names);
    assert_eq!(shown, "Fail<E102>");
}

#[test]
fn hidden_failures_of_a_row_parameter() {
    // `{| e}` with a failure-carrying `e` is `{Fail<e.Fail> | e}`: it fits `! e`, not `{}`,
    // and passing it to an open row hands `e.Fail` over without showing it.
    let mut inf = Infer::new();
    let e = Tail::Param(inf.interner.new_row_param("e", true));
    let re = row(&mut inf, &[], e);
    let empty = inf.interner.empty_row();
    assert!(inf.subsume_rows(re, re).is_ok());
    assert_eq!(inf.subsume_rows(re, empty), Err((vec![], true)));
    let open = inf.fresh_row();
    inf.subsume_rows(re, open).unwrap();
    inf.default_row(open);
    assert_eq!(show_row(&mut inf, open), "e");
    assert!(inf.subsume_rows(open, re).is_ok());
}

#[test]
fn upper_bound_is_deferred() {
    // §5.2 rule 9: `{| r} ⊆ {E1, E2}` bounds `r` instead of closing it.
    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let open = row(&mut inf, &[], r);
    let bound = inf.closed_row(vec![label(1), label(2)]);
    inf.subsume_rows(open, bound).unwrap();
    let one = inf.closed_row(vec![label(1)]);
    inf.unify_rows(open, one).unwrap();
    inf.default_row(open);
    assert_eq!(show_row(&mut inf, open), "{E1}");

    let mut inf = Infer::new();
    let r = var_tail(&mut inf);
    let open = row(&mut inf, &[], r);
    let bound = inf.closed_row(vec![label(1), label(2)]);
    inf.subsume_rows(open, bound).unwrap();
    let three = inf.closed_row(vec![label(3)]);
    match inf.unify_rows(open, three) {
        Err(UnifyError::Rows { extra, .. }) => assert_eq!(extra, vec![label(3)]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn bounds_intersect() {
    for (want, ok) in [(2, false), (1, true)] {
        let mut inf = Infer::new();
        let r = var_tail(&mut inf);
        let open = row(&mut inf, &[], r);
        let b1 = inf.closed_row(vec![label(1), label(2)]);
        let b2 = inf.closed_row(vec![label(1), label(3)]);
        inf.subsume_rows(open, b1).unwrap();
        inf.subsume_rows(open, b2).unwrap();
        let w = inf.closed_row(vec![label(want)]);
        assert_eq!(inf.unify_rows(open, w).is_ok(), ok, "E{want}");
    }
}

#[test]
fn bound_flows_to_fresh_tails() {
    let mut inf = Infer::new();
    let (r, s) = (var_tail(&mut inf), var_tail(&mut inf));
    let (ro, so) = (row(&mut inf, &[], r), row(&mut inf, &[], s));
    let clock = inf.closed_row(vec![label(1)]);
    inf.subsume_rows(ro, clock).unwrap();
    inf.unify_rows(ro, so).unwrap();
    let db = inf.closed_row(vec![label(2)]);
    assert!(inf.unify_rows(so, db).is_err());
}

#[test]
fn rigid_bound() {
    let mut inf = Infer::new();
    let e = Tail::Param(inf.interner.new_row_param("e", false));
    let f = Tail::Param(inf.interner.new_row_param("f", false));
    for (tail, ok) in [(e, true), (f, false)] {
        let r = var_tail(&mut inf);
        let open = row(&mut inf, &[], r);
        let bound = row(&mut inf, &[1], e);
        inf.subsume_rows(open, bound).unwrap();
        let want = row(&mut inf, &[1], tail);
        assert_eq!(inf.unify_rows(open, want).is_ok(), ok);
    }
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
    let e = Tail::Param(inf.interner.new_row_param("e", false));
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
            // Bounded tails stay open until the body ends (§5.2 rule 9).
            env.inf.default_row(ra);
            env.inf.default_row(rb);
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

// ---- order independence (design §5.2 rule 9, §17.46) ----

/// A row in a constraint script: labels, a `Fail` part and a tail.
#[derive(Clone, Debug)]
struct ScriptRow {
    labels: BTreeSet<u32>,
    /// 0 no `Fail`, 1 `Fail<tags>`, 2 `Fail<ε>` with the script's shared error variable.
    fail: u8,
    tags: BTreeSet<u32>,
    /// 0 closed, 1 shared x, 2 shared y, 3 fresh, 4 rigid Fail-free e, 5 rigid carrying g.
    tail: u8,
}

#[derive(Clone, Debug)]
enum Step {
    Subsume(ScriptRow, ScriptRow),
    Unify(ScriptRow, ScriptRow),
    Union(ScriptRow, ScriptRow),
}

fn script_row() -> impl Strategy<Value = ScriptRow> {
    (
        prop::collection::btree_set(0u32..4, 0..3),
        0u8..3,
        prop::collection::btree_set(1u32..4, 1..3),
        0u8..6,
    )
        .prop_map(|(labels, fail, tags, tail)| ScriptRow {
            labels,
            fail,
            tags,
            tail,
        })
}

/// Steps inside the fragment §5.2 rule 9 makes order independent: everything except relating
/// two *different* flexible tails, which is solved by equality (rules 1 and 5; see the plan's
/// Deviations).
fn step() -> impl Strategy<Value = Step> {
    let flex = |t: u8| (1..=3).contains(&t);
    (0u8..3, script_row(), script_row())
        .prop_filter("two different flexible tails", move |(_, a, b)| {
            !(flex(a.tail) && flex(b.tail) && (a.tail != b.tail || a.tail == 3))
        })
        .prop_map(|(k, a, b)| match k {
            0 => Step::Subsume(a, b),
            1 => Step::Unify(a, b),
            _ => Step::Union(a, b),
        })
}

struct ScriptEnv {
    inf: Infer,
    x: Tail,
    y: Tail,
    e: Tail,
    g: Tail,
    eps: TypeId,
}

fn script_env() -> ScriptEnv {
    let mut inf = Infer::new();
    let x = Tail::Var(inf.fresh_row_var());
    let y = Tail::Var(inf.fresh_row_var());
    let e = Tail::Param(inf.interner.new_row_param("e", false));
    let g = Tail::Param(inf.interner.new_row_param("g", true));
    let eps = inf.fresh_err_var();
    ScriptEnv {
        inf,
        x,
        y,
        e,
        g,
        eps,
    }
}

fn build_script_row(env: &mut ScriptEnv, s: &ScriptRow) -> RowId {
    let tail = match s.tail {
        0 => Tail::Closed,
        1 => env.x,
        2 => env.y,
        3 => Tail::Var(env.inf.fresh_row_var()),
        4 => env.e,
        _ => env.g,
    };
    let mut labels: Vec<Label> = s.labels.iter().map(|i| label(*i)).collect();
    match s.fail {
        1 => {
            let tags: Vec<u32> = s.tags.iter().copied().collect();
            labels.push(fail(&mut env.inf, &tags));
        }
        2 => labels.push(Label {
            effect: Effect::Fail,
            args: vec![env.eps],
        }),
        _ => {}
    }
    env.inf.make_row(labels, tail)
}

/// Runs the steps in `order`; returns whether all succeeded and, if so, every row of the
/// script (and every union result) rendered after body-end defaulting.
fn run_script(steps: &[Step], order: &[usize]) -> (bool, Vec<String>) {
    let mut env = script_env();
    let rows: Vec<(RowId, RowId)> = steps
        .iter()
        .map(|s| {
            let (a, b) = match s {
                Step::Subsume(a, b) | Step::Unify(a, b) | Step::Union(a, b) => (a, b),
            };
            (build_script_row(&mut env, a), build_script_row(&mut env, b))
        })
        .collect();
    let mut unions = vec![None; steps.len()];
    let mut ok = true;
    for &i in order {
        let (a, b) = rows[i];
        ok &= match &steps[i] {
            Step::Subsume(..) => env.inf.subsume_rows(a, b).is_ok(),
            Step::Unify(..) => env.inf.unify_rows(a, b).is_ok(),
            Step::Union(..) => match env.inf.union_rows(a, b) {
                Ok(u) => {
                    unions[i] = Some(u);
                    true
                }
                Err(_) => false,
            },
        };
    }
    if !ok {
        return (false, Vec::new());
    }
    let mut all: Vec<RowId> = rows.iter().flat_map(|(a, b)| [*a, *b]).collect();
    all.extend(unions.into_iter().flatten());
    for r in &all {
        env.inf.default_row(*r);
    }
    let shown = all.iter().map(|r| show_row(&mut env.inf, *r)).collect();
    (true, shown)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    #[test]
    fn script_is_order_independent(
        (steps, order) in prop::collection::vec(step(), 2..=6).prop_flat_map(|steps| {
            let n = steps.len();
            (Just(steps), Just((0..n).collect::<Vec<usize>>()).prop_shuffle())
        })
    ) {
        let identity: Vec<usize> = (0..steps.len()).collect();
        let (ok1, rows1) = run_script(&steps, &identity);
        let (ok2, rows2) = run_script(&steps, &order);
        prop_assert_eq!(ok1, ok2);
        prop_assert_eq!(rows1, rows2);
    }

    #[test]
    fn fail_lub_is_commutative_and_idempotent(
        a in prop::collection::btree_set(1u32..6, 0..4),
        b in prop::collection::btree_set(1u32..6, 0..4),
    ) {
        let set = |xs: &BTreeSet<u32>| ErrSet {
            tags: xs.iter().map(|t| test_def(100 + t)).collect(),
            params: BTreeSet::new(),
        };
        let lub = |sets: &[&BTreeSet<u32>]| {
            let mut inf = Infer::new();
            let eps = inf.fresh_err_var();
            let Type::Var(v) = *inf.interner.ty(eps) else { unreachable!() };
            for s in sets {
                inf.err_lower(v, &set(s)).unwrap();
            }
            inf.err_lower_bound(eps)
        };
        let ab: BTreeSet<u32> = a.union(&b).copied().collect();
        prop_assert_eq!(lub(&[&a, &b]), lub(&[&b, &a]));
        prop_assert_eq!(lub(&[&a, &b]), lub(&[&ab]));
        prop_assert_eq!(lub(&[&a, &a]), lub(&[&a]));
    }
}
