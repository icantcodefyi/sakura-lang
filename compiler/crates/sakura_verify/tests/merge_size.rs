//! Fuzz run 39564548413 (`vc`, a 13 s timeout on `tests/semantics/crypto_aes_cbc_vectors.sak`):
//! a merge copied each path's guard into the disjunction, the pending implications and every
//! merged value, and the merge of an enclosing inlined callee copied that again once per path, so
//! a few `std/crypto` calls built path conditions of tens of thousands of nodes. A compound guard
//! is now a named definition, so the VC no longer grows exponentially in the inlining depth.

use sakura_analysis::{Analysis, BuildOptions, Snapshot};
use sakura_verify::{core, smt, vc};

/// The fuzz input reduced by hand: two `encrypt` and two `decrypt` calls.
const PROGRAM: &str = r#"
import std/crypto
import std/crypto.{AesLength, Algorithm, Usage}

pub fn main() -> Unit ! {Console} {
  let alg = Algorithm.AesCbc(AesLength.Aes128)
  let k = with Random.seeded(4) { crypto.generate_key(alg, [Usage.Encrypt, Usage.Decrypt], false) }
  let iv = Bytes.from_list([])
  match crypto.encrypt(k, iv, iv, iv) {
    Ok(sealed) => {
      Console.print("{crypto.decrypt(k, iv, iv, sealed)}")
      Console.print("{crypto.decrypt(k, iv, iv, iv)}")
    },
    Err(e) => Console.print("{e}"),
  }
  Console.print("{crypto.encrypt(k, iv, iv, iv)}")
}
"#;

fn script_bytes(program: &str) -> usize {
    let snap = Snapshot::with_std([("file:///vc/src/main.sak".into(), program.into())]);
    let opts = BuildOptions {
        all_fns: true,
        ..Default::default()
    };
    let (m, diags) = Analysis::lower_core(&snap, &opts);
    let m = core::convert::convert(&m.unwrap_or_else(|| panic!("{diags:?}")));
    let mut n = 0;
    for profile in [vc::Profile::Js, vc::Profile::I64] {
        for def in vc::generate(&m, profile) {
            for o in &def.obligations {
                for q in smt::queries(o) {
                    n += smt::script(&def, o, q).len();
                }
            }
        }
    }
    n
}

#[test]
fn nested_inlined_merges_stay_small() {
    // 2.3 MB with named guards, 26 MB before.
    let n = script_bytes(PROGRAM);
    assert!(n < 5_000_000, "{n} bytes of SMT");
}
