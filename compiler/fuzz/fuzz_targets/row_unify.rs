#![no_main]
use libfuzzer_sys::fuzz_target;
use sakura_hir::{DefId, ModuleId};
use sakura_types::{Effect, Infer, Label, RowId, Tail, UnifyError};

fn label(n: u8) -> Label {
    Label {
        effect: Effect::Def(DefId {
            module: ModuleId(0),
            index: la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(n as u32)),
        }),
        args: vec![],
    }
}

fn pair(data: &[u8; 4]) -> (Infer, RowId, RowId) {
    let mut inf = Infer::new();
    let shared = Tail::Var(inf.fresh_row_var());
    let rigid = Tail::Param(inf.interner.new_row_param("e", false));
    let mut build = |mask: u8, tail: u8| {
        let labels = (0..8).filter(|n| mask & (1 << n) != 0).map(label).collect();
        let tail = match tail % 4 {
            0 => Tail::Closed,
            1 => shared,
            2 => Tail::Var(inf.fresh_row_var()),
            _ => rigid,
        };
        inf.make_row(labels, tail)
    };
    let a = build(data[0], data[1]);
    let b = build(data[2], data[3]);
    (inf, a, b)
}

fn labels(inf: &mut Infer, row: RowId) -> Vec<Label> {
    inf.flatten_row(row).labels
}

fuzz_target!(|data: [u8; 4]| {
    // §14.5: commutativity, reflexivity/idempotence, and occurs rejection. Each
    // direction gets fresh inference state, since failed unification can bind variables.
    let (mut inf, a, b) = pair(&data);
    let (mut rev, ra, rb) = pair(&data);
    let forward = inf.unify_rows(a, b).is_ok();
    assert_eq!(forward, rev.unify_rows(rb, ra).is_ok());
    if forward {
        inf.default_row(a);
        inf.default_row(b);
        assert_eq!(inf.flatten_row(a), inf.flatten_row(b));
        assert_eq!(labels(&mut inf, a), labels(&mut rev, ra));
    }
    let (mut inf, a, b) = pair(&data);
    assert!(inf.unify_rows(a, a).is_ok());
    let aa = inf.union_rows(a, a).unwrap();
    assert_eq!(inf.flatten_row(aa), inf.flatten_row(a));
    let (mut rev, ra, rb) = pair(&data);
    let union = inf.union_rows(a, b);
    let reversed = rev.union_rows(rb, ra);
    assert_eq!(union.is_ok(), reversed.is_ok());
    if let (Ok(u), Ok(v)) = (union, reversed) {
        assert_eq!(labels(&mut inf, u), labels(&mut rev, v));
    }
    let mut inf = Infer::new();
    let recursive = inf.fresh_row();
    let unit = inf.interner.unit();
    let f = inf.interner.func(vec![], unit, recursive);
    let mut effect = label(data[0]);
    effect.args.push(f);
    let contains_self = inf.closed_row(vec![effect]);
    assert!(matches!(
        inf.unify_rows(recursive, contains_self),
        Err(UnifyError::Tails { .. })
    ));
});
