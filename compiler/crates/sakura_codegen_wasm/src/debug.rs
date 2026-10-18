//! The `sakura.debug` custom section (ABI §12), version `SKRDBG01`.
//!
//! W1 writes the header and the function table; the LIR carries no source spans yet, so there
//! are no sources, sites or block spans, and the module hash is zero (not computed), which the
//! host reads as "ignore the section". The names section carries the function names.

use sakura_lir::lir::Module;

pub fn section(m: &Module) -> Vec<u8> {
    let mut out = b"SKRDBG01".to_vec();
    let u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    u32(&mut out, crate::ABI_VERSION);
    out.extend_from_slice(&[0; 32]);
    u32(&mut out, 0); // sources
    u32(&mut out, 0); // sites
    // Functions: code, offset of the name in the trailing pool, site (none).
    let mut pool = vec![];
    u32(&mut out, m.funcs.len() as u32);
    for (i, f) in m.funcs.iter().enumerate() {
        u32(&mut out, i as u32);
        u32(&mut out, pool.len() as u32);
        u32(&mut out, 0);
        let name = format!("{}::{}", f.module, f.name);
        u32(&mut pool, name.len() as u32);
        pool.extend_from_slice(name.as_bytes());
    }
    u32(&mut out, 0); // blocks
    out.extend(pool);
    out
}
