//! Design §17.310 (2g): Core's `opt::owned::BORROWS` (what JS's in-place facts and `W8001` take
//! as only read) against N4a's `borrow::reads` (what the runtime only reads), position by
//! position. Core may claim a position only where `reads` does, since a Core borrow that the
//! runtime keeps is the `map_get_or` class of bug; and for a builtin Core lists, it claims every
//! position `reads` has. Builtins only `reads` lists are left to it: Core treating an argument
//! as consumed is the conservative reading. Each difference is named below with its reason
//! (recorded in `compiler/REQUESTS.md`, 2026-10-22, the `BORROWS` audit).

use sakura_core::builtins::BUILTINS;
use sakura_core::opt::owned::{BORROWS, borrows_arg};
use sakura_lir::borrow::reads;

/// `(builtin, argument, why Core and `reads` differ there)`.
const DIFFER: &[(&str, usize, &str)] = &[
    ("list_get", 1, "an `Int` index, no reference"),
    ("list_get_expect", 1, "an `Int` index, no reference"),
    ("list_at", 1, "an `Int` index, no reference"),
    ("list_builder_at", 1, "an `Int` index, no reference"),
];

#[test]
fn core_borrows_agree_with_reads() {
    let mut got = vec![];
    for name in BORROWS {
        let b = BUILTINS.iter().find(|b| b.name == *name);
        let b = b.unwrap_or_else(|| panic!("BORROWS names `{name}`, no builtin"));
        for i in 0..b.arity {
            if borrows_arg(name, i) != reads(name, i) {
                got.push((*name, i));
            }
        }
    }
    let named: Vec<_> = DIFFER.iter().map(|&(n, i, _)| (n, i)).collect();
    assert_eq!(
        got, named,
        "a difference not named in DIFFER, or a named one gone"
    );
}
