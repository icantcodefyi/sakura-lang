//! Which call arguments are only read (design §17.176 (1), §17.247 (1)): the marking behind
//! [`Inst::DupCandidate`], for every managed type.
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

use std::collections::HashSet;

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
                if nc[fi][i] && consumes(f, f.params()[i], &nc) {
                    nc[fi][i] = false;
                    changed = true;
                }
            }
        }
    }
    nc
}

fn consumes(f: &Func, p: Var, nc: &[Vec<bool>]) -> bool {
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
                        Inst::Builtin { name, .. } if reads(name, k) => {}
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
fn carriers(f: &Func, p: Var) -> HashSet<Var> {
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
