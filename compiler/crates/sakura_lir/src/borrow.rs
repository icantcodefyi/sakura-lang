//! Which call arguments are only read (design §17.176 (1), §17.247 (1)): the marking behind
//! [`Inst::DupCandidate`], for every managed type, and the borrowed convention that removes the
//! pair (plan M5 N4a, design §17.176 (4)).
//!
//! A builtin only reads argument `i` when the runtime retains nothing of the argument itself
//! (it may return a duplicated child), never writes it in place and drops it once ([`reads`],
//! audited against `runtime/rt`). A direct def *never consumes* parameter `i` when no value
//! carrying it is consumed on any path ([`never_consumes`]): a *carrier* is the parameter or a
//! block parameter every incoming edge passes a carrier to (a greatest fixpoint, so a loop's
//! back edge keeps it), and each use of a carrier is a borrow (`Proj`, `Prim`, `Tag`, …), an
//! argument a builtin only reads, an argument of a direct def that never consumes it, or an
//! edge argument to a carrier. A return, a `fail`, an allocation, a closure, a handler, a
//! `perform`, a closure call or a callback builtin consumes it. A projection is a new reference
//! (the field is duplicated), so a def returning a field keeps nothing of the parameter.
//! Read before ownership inserts `Dup`/`Drop`.
//!
//! **The borrowed convention.** At a marked site the owner stays live after the call, so
//! ownership lends it instead ([`Inst::Lend`]: a `ref` the call reads while the caller holds the
//! owner) when the callee takes that argument borrowed: a builtin the runtime has a borrowed
//! entry for ([`lends`]: `rt_bb_<name>`, which drops none of the arguments its mask names; never
//! a host call), or a parameter of a direct def that [`borrowable`] keeps: never consumed, and
//! no carrier of it crosses a `perform`, a closure call, a callback builtin or a `with`/`try`
//! region. Such a call goes to the def's *borrowed variant* ([`variants`]): a copy of its body
//! whose lent parameters and their carriers are `ref`, so the variant neither counts nor drops
//! them. A variant is made per set of lent parameters that a site passes, so an argument that
//! is not live after the call is still passed owned: no call gains a `Drop` after it and no
//! tail call is lost. Any other marked site keeps its `dup.candidate`. [`strip`] undoes it all.

use std::collections::{BTreeMap, HashSet};

use crate::lir::*;

/// Builtin `name` only reads its argument `i`.
pub fn reads(name: &str, i: usize) -> bool {
    READS
        .iter()
        .any(|(n, args)| *n == name && args.contains(&i))
}

/// Per builtin, the arguments it only reads. A map or set key's string may have its hash cached
/// in its header (`string_hash`), which a shared key gets as well. Not here, with the reason:
/// a `msg` (`*_expect`: a defect keeps it), `map_get_or`'s default (returned), `string_trim`,
/// `string_slice`, `list_slice`, `bytes_slice` and `show` (may return the argument), every
/// owned update and `list_reuse` (write or return it), and `bytes_concat` (its overflow path
/// does not drop its list).
const READS: &[(&str, &[usize])] = &[
    ("list_len", &[0]),
    ("list_at", &[0]),
    ("list_get", &[0]),
    ("list_get_expect", &[0]),
    ("list_builder_at", &[0]),
    ("map_len", &[0]),
    ("map_get", &[0, 1]),
    ("map_get_expect", &[0, 1]),
    ("map_get_or", &[0, 1]),
    ("map_contains", &[0, 1]),
    ("map_entries", &[0]),
    ("map_remove", &[1]),
    ("set_len", &[0]),
    ("set_contains", &[0, 1]),
    ("set_to_list", &[0]),
    ("option_expect", &[0]),
    ("string_len", &[0]),
    ("string_to_upper", &[0]),
    ("string_to_lower", &[0]),
    ("string_contains", &[0, 1]),
    ("string_starts_with", &[0, 1]),
    ("string_ends_with", &[0, 1]),
    ("string_chars", &[0]),
    ("string_split", &[0, 1]),
    ("string_replace", &[0, 1, 2]),
    ("string_parse_int", &[0]),
    ("string_parse_float", &[0]),
    ("string_to_bytes", &[0]),
    ("string_from_bytes", &[0]),
    ("json_encode", &[0, 1]),
    ("json_parse", &[0]),
    ("bytes_len", &[0]),
    ("bytes_get", &[0]),
    ("bytes_from_list", &[0]),
    ("bytes_to_base64", &[0]),
    ("bytes_from_base64", &[0]),
    ("bytes_from_base64_lenient", &[0]),
    ("compress_crc32", &[1]),
    ("compress_adler32", &[1]),
    ("compress_deflate", &[0]),
    ("compress_inflate", &[0]),
    ("compress_inflate_used", &[0]),
    ("compress_inflate_done", &[0]),
    ("crypto_aes_gcm_open", &[0, 1, 2, 3]),
    ("crypto_aes_gcm_seal", &[0, 1, 2, 3]),
    ("crypto_cipher", &[1, 2, 3, 4]),
    ("crypto_digest", &[1]),
    ("crypto_ecdh", &[1, 2]),
    ("crypto_hkdf", &[1, 2, 3]),
    ("crypto_hmac", &[1, 2]),
    ("crypto_hmac_verify", &[1, 2, 3]),
    ("crypto_pbkdf2", &[1, 2]),
    ("crypto_public_of", &[1]),
    ("crypto_public_valid", &[1]),
    ("crypto_sign", &[1, 2]),
    ("crypto_sign_verify", &[1, 2, 3]),
];

/// Builtin `name` takes its argument `i` borrowed (`rt_bb_<name>`, ABI 1.18.0): it only reads it
/// and is no host call (a hosted builtin's arguments cross to the host, which design §17.176 (4)
/// keeps owned).
pub fn lends(name: &str, i: usize) -> bool {
    use crate::image::{HOSTED, WASM_HOSTED};
    reads(name, i) && !HOSTED.iter().chain(WASM_HOSTED).any(|(n, _)| *n == name)
}

/// Per function, per parameter: the def never consumes it. Every entry of a function `direct`
/// rejects is false.
pub fn never_consumes(funcs: &[Func], direct: &dyn Fn(FuncId) -> bool) -> Vec<Vec<bool>> {
    let mut nc: Vec<Vec<bool>> = (funcs.iter().enumerate())
        .map(|(i, f)| {
            let ok = direct(FuncId(i as u32));
            f.params()
                .iter()
                .map(|&p| ok && f.repr(p).is_managed())
                .collect()
        })
        .collect();
    // From "nothing consumes" down: each pass finds the consumptions one call further away.
    let mut changed = true;
    while changed {
        changed = false;
        for (fi, f) in funcs.iter().enumerate() {
            for i in 0..nc[fi].len() {
                if nc[fi][i] && consumes(f, f.params()[i], &nc, reads) {
                    nc[fi][i] = false;
                    changed = true;
                }
            }
        }
    }
    nc
}

/// Per function, per parameter: the def may take it borrowed (module docs). A subset of
/// [`never_consumes`]'s, from it down.
pub fn borrowable(funcs: &[Func], never: &[Vec<bool>]) -> Vec<Vec<bool>> {
    let mut ok = never.to_vec();
    let mut changed = true;
    while changed {
        changed = false;
        for (fi, f) in funcs.iter().enumerate() {
            for i in 0..ok[fi].len() {
                if ok[fi][i] && (consumes(f, f.params()[i], &ok, lends) || held(f, f.params()[i])) {
                    ok[fi][i] = false;
                    changed = true;
                }
            }
        }
    }
    ok
}

/// A carrier of `p` is held where a borrow may not be: across a `perform`, a closure call or a
/// callback builtin, or inside a `with`/`try` region.
fn held(f: &Func, p: Var) -> bool {
    let c = carriers(f, p);
    f.blocks.iter().any(|b| {
        let here = b.params.iter().any(|q| c.contains(q));
        here && (!b.regions.is_empty()
            || (b.insts.iter()).any(|i| {
                matches!(
                    i,
                    Inst::HandlePush { .. } | Inst::Bound { .. } | Inst::TryPush { .. }
                )
            })
            || matches!(&b.term,
                Term::Perform { cont, .. }
                | Term::Call {
                    callee: Callee::Closure(_) | Callee::Builtin(_),
                    cont,
                    ..
                } if cont.args.iter().any(|a| c.contains(a))))
    })
}

fn consumes(f: &Func, p: Var, nc: &[Vec<bool>], only: fn(&str, usize) -> bool) -> bool {
    let c = carriers(f, p);
    if !c.contains(&p) {
        return true;
    }
    let kept = |g: &FuncId, j: usize| nc.get(g.index()).and_then(|a| a.get(j)) != Some(&true);
    for b in &f.blocks {
        for inst in &b.insts {
            for (k, (v, u)) in inst.uses().into_iter().enumerate() {
                if u == Use::Consume && c.contains(&v) {
                    match inst {
                        Inst::Builtin { name, .. } if only(name, k) => {}
                        _ => return true,
                    }
                }
            }
        }
        let taken = match &b.term {
            Term::Call {
                callee: Callee::Direct(g),
                args,
                ..
            } => (args.iter().enumerate()).any(|(j, a)| c.contains(a) && kept(g, j)),
            t => (t.uses().iter()).any(|(v, u)| *u == Use::Consume && c.contains(v)),
        };
        if taken {
            return true;
        }
        for (e, skip) in b.term.edges() {
            let params = &f.block(e.block).params;
            for (j, a) in e.args.iter().enumerate() {
                if c.contains(a) && !params.get(skip + j).is_some_and(|q| c.contains(q)) {
                    return true;
                }
            }
        }
    }
    false
}

/// `p` and every block parameter all of whose incoming edges pass a carrier (none: a catch
/// entry's, filled by a `fail`).
pub(crate) fn carriers(f: &Func, p: Var) -> HashSet<Var> {
    let mut entered = HashSet::new();
    for b in &f.blocks {
        for (e, _) in b.term.edges() {
            entered.extend(f.block(e.block).params.iter().copied());
        }
    }
    let mut c: HashSet<Var> = (f.blocks.iter().skip(1))
        .flat_map(|b| b.params.iter().copied())
        .filter(|q| entered.contains(q))
        .collect();
    c.insert(p);
    let mut changed = true;
    while changed {
        changed = false;
        for b in &f.blocks {
            for (e, skip) in b.term.edges() {
                for (j, q) in f.block(e.block).params.iter().enumerate() {
                    let passed = j.checked_sub(skip).and_then(|j| e.args.get(j));
                    if c.contains(q) && !passed.is_some_and(|a| c.contains(a)) {
                        c.remove(q);
                        changed = true;
                    }
                }
            }
        }
    }
    c
}

/// Makes the borrowed variants the calls of `funcs` need (module docs) and points each call
/// passing a `ref` at one. `pre` is every function before ownership, and `own` runs ownership
/// (and reuse) over a new variant. A variant of def `g` lending parameters `L` is `pre[g]` with
/// those parameters and their carriers `ref`; its own calls may need more. Returns each variant
/// with its def, or `Err(c)` when direct component `c` ([`components`]) would need more than
/// `bound[c]` (plan M5 N4b; the caller then lends nothing to `c`).
pub fn variants(
    funcs: &mut Vec<Func>,
    pre: &[Func],
    (comp, bound): (&[usize], &[usize]),
    own: &mut dyn FnMut(&mut Func) -> Result<(), String>,
) -> Result<Result<Vec<(FuncId, FuncId)>, usize>, String> {
    let mut made: BTreeMap<(FuncId, Vec<bool>), FuncId> = BTreeMap::new();
    let mut count = vec![0; bound.len()];
    let mut out = Vec::new();
    let mut next = 0;
    while next < funcs.len() {
        for b in 0..funcs[next].blocks.len() {
            let f = &funcs[next];
            let Term::Call {
                callee: Callee::Direct(g),
                args,
                ..
            } = &f.blocks[b].term
            else {
                continue;
            };
            let lent: Vec<bool> = args.iter().map(|&a| f.repr(a) == Repr::Ref).collect();
            if !lent.contains(&true) {
                continue;
            }
            let key = (*g, lent);
            let v = match made.get(&key) {
                Some(&v) => v,
                None => {
                    let c = comp[key.0.index()];
                    count[c] += 1;
                    if count[c] > bound[c] {
                        return Ok(Err(c));
                    }
                    let mut t = pre[key.0.index()].clone();
                    let params = t.params().to_vec();
                    for (&p, _) in params.iter().zip(&key.1).filter(|(_, l)| **l) {
                        for c in carriers(&t, p) {
                            t.vars[c.index()].repr = Repr::Ref;
                        }
                    }
                    // The def's own name: a contract's defect message names it.
                    if let FuncKind::Top { export } = &mut t.kind {
                        *export = false;
                    }
                    own(&mut t)?;
                    let v = FuncId(funcs.len() as u32);
                    funcs.push(t);
                    out.push((v, key.0));
                    made.insert(key, v);
                    v
                }
            };
            if let Term::Call { callee, .. } = &mut funcs[next].blocks[b].term {
                *callee = Callee::Direct(v);
            }
        }
        next += 1;
    }
    Ok(Ok(out))
}

/// Each function's strongly connected component of the direct call graph, and how many there
/// are: [`variants`]' unit of bound (plan M5 N4b).
pub fn components(funcs: &[Func]) -> (Vec<usize>, usize) {
    let succ: Vec<Vec<usize>> = funcs
        .iter()
        .map(|f| {
            (f.blocks.iter())
                .filter_map(|b| match &b.term {
                    Term::Call {
                        callee: Callee::Direct(g),
                        ..
                    } => Some(g.index()),
                    _ => None,
                })
                .collect()
        })
        .collect();
    crate::split::components(&succ)
}

/// `m` as lowered without the borrowed convention: each [`Inst::Lend`] its `dup.candidate`
/// again, each call of a borrowed variant a call of its def, and the variants gone. The
/// `m5-stress` set runs with and without borrowing (plan M5 §4).
pub fn strip(m: &mut Module) {
    let of: BTreeMap<FuncId, FuncId> = m.borrowed.iter().copied().collect();
    if let Some(&(first, _)) = m.borrowed.first() {
        m.funcs.truncate(first.index());
        m.direct.truncate(first.index());
    }
    m.borrowed.clear();
    for f in &mut m.funcs {
        for b in &mut f.blocks {
            let mut lent: BTreeMap<Var, Var> = BTreeMap::new();
            for i in &mut b.insts {
                if let Inst::Lend { dst, src } = *i {
                    lent.insert(dst, src);
                    *i = Inst::DupCandidate(src);
                } else if let Inst::Builtin { args, .. } = i {
                    args.iter_mut().for_each(|a| *a = *lent.get(a).unwrap_or(a));
                }
            }
            if let Term::Call { callee, args, .. } = &mut b.term {
                args.iter_mut().for_each(|a| *a = *lent.get(a).unwrap_or(a));
                if let Callee::Direct(g) = callee
                    && let Some(&d) = of.get(g)
                {
                    *g = d;
                }
            }
        }
    }
}
