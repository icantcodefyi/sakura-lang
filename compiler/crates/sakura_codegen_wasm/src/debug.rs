//! The `sakura.debug` custom section (ABI §12), version `SKRDBG01`.
//!
//! The sources (URI per `Pos::file`) and the LIR's site table, which the host reads to give a
//! defect record the spans JS gives it (design §17.162); the function table (each function's
//! site is 0: frames carry no spans yet, so there are no block spans). The module hash and the
//! sources' content hashes are zero (not computed): the section travels inside the module it
//! describes. The names section carries the function names.

use sakura_lir::lir::{Module, SiteRole};

pub fn section(m: &Module, sources: &[String]) -> Vec<u8> {
    let mut out = b"SKRDBG01".to_vec();
    let u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    u32(&mut out, crate::ABI_VERSION);
    out.extend_from_slice(&[0; 32]);
    u32(&mut out, sources.len() as u32);
    for uri in sources {
        u32(&mut out, uri.len() as u32);
        out.extend_from_slice(uri.as_bytes());
        out.extend_from_slice(&[0; 32]);
    }
    u32(&mut out, m.sites.len() as u32);
    for s in &m.sites {
        u32(&mut out, s.file);
        u32(&mut out, s.start);
        u32(&mut out, s.end);
        out.push(match s.role {
            SiteRole::Primary => 0,
            SiteRole::Definition => 1,
        });
        u32(&mut out, s.contract);
    }
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
