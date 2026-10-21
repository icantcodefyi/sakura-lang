//! Constructor-site reuse (plan M5 N3, design §17.176 (3); Perceus `reset`/`reuse`).
//!
//! After [`crate::lower::ownership`], a block that drops an object whose shape one of its
//! `Proj`s names (the arm of a `match` that took a constructor apart) and later allocates an
//! object of that very shape (the same layout row and constructor, so the same class, slot
//! kinds and child count) turns the `Drop` into [`RESET`] and hands its token to that `Alloc`.
//! At run time `reuse_reset` tests the object: when no other reference reaches it, it releases
//! the object's children and returns the emptied object as the token (`reuses` counted), and
//! the `Alloc` writes the new constructor into its storage; otherwise it drops it as the `Drop`
//! did and returns `Unit`, and the `Alloc` allocates. Every `reuse_reset` is a `reuse_attempts`.
//!
//! The token is an ordinary owned word, so the ownership count and every landing pad already
//! free it on each exit; it never leaves its block (the validator), and a block has no call,
//! `perform` or suspension before its terminator, so no token is held across one. A pair split
//! by a call (the recursive case of a tree insertion) is not reused: the token would ride a
//! frame. ponytail: in-block pairs only; tokens across direct calls when a frame slot kind for
//! them exists.

use std::collections::{HashMap, HashSet};

use crate::lir::*;

/// The builtin a reused `Drop` becomes: `(ptr) -> word`.
pub const RESET: &str = "reuse_reset";

/// Pairs each block's `Drop`s with later `Alloc`s of the same constructor (module docs);
/// whether any pair was made.
pub fn reuse(f: &mut Func) -> bool {
    let mut any = false;
    for b in 0..f.blocks.len() {
        let mut shapes: HashMap<Var, Shape> = HashMap::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut taken: HashSet<usize> = HashSet::new();
        let insts = &f.blocks[b].insts;
        for (k, i) in insts.iter().enumerate() {
            match i {
                Inst::Proj { src, shape, .. } => {
                    shapes.insert(*src, shape.clone());
                }
                Inst::Drop(x) => {
                    let Some(shape) = shapes.get(x) else {
                        continue;
                    };
                    let alloc = (k + 1..insts.len()).find(|j| {
                        !taken.contains(j)
                            && matches!(&insts[*j], Inst::Alloc { shape: s, reuse: None, .. }
                                if s == shape && matches!(s, Shape::Record(_) | Shape::Variant(..)))
                    });
                    if let Some(j) = alloc {
                        taken.insert(j);
                        pairs.push((k, j));
                    }
                }
                _ => {}
            }
        }
        for (k, j) in pairs {
            let token = Var(f.vars.len() as u32);
            f.vars.push(VarDecl {
                name: "token".into(),
                repr: Repr::Word,
            });
            let insts = &mut f.blocks[b].insts;
            let Inst::Drop(x) = insts[k] else {
                unreachable!("paired a drop")
            };
            insts[k] = Inst::Builtin {
                dst: token,
                name: RESET.into(),
                args: vec![x],
            };
            if let Inst::Alloc { reuse, .. } = &mut insts[j] {
                *reuse = Some(token);
            }
            any = true;
        }
    }
    any
}
