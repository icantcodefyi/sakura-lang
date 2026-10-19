//! `crypto.rs` (native only) on the official vector tables of `compiler/tests/vectors` (the JS
//! runtime's `runtime.test.js` reads the same files), then once through the `Rt` builtins, which
//! must drop their arguments.
#![cfg(feature = "native")]

mod common;

use common::{Known, rt_with};
use sakura_rt::crypto::{digest, hkdf_derive, hmac, hmac_verify, pbkdf2_derive};

fn rows(table: &str) -> impl Iterator<Item = Vec<&str>> {
    table
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `std/crypto`'s `code(digest)`.
fn code(alg: &str) -> i64 {
    match alg {
        "SHA-256" => 0,
        "SHA-384" => 1,
        "SHA-512" => 2,
        _ => panic!("unknown hash {alg}"),
    }
}

#[test]
fn sha2_nist_cavp_short_messages() {
    let mut n = 0;
    for r in rows(include_str!("../../../tests/vectors/sha2.txt")) {
        assert_eq!(
            hex(&digest(code(r[0]), &unhex(r[1]))),
            r[2],
            "{} {}",
            r[0],
            r[1]
        );
        n += 1;
    }
    assert_eq!(n, 65 + 129 + 129);
}

#[test]
fn hmac_rfc4231_and_constant_time_verify() {
    for r in rows(include_str!("../../../tests/vectors/hmac.txt")) {
        let (alg, key, data) = (code(r[1]), unhex(r[2]), unhex(r[3]));
        let bits: usize = r[5].parse().unwrap();
        let tag = hmac(alg, &key, &data);
        assert_eq!(&hex(&tag)[..bits / 4], r[4], "case {} {}", r[0], r[1]);
        assert!(hmac_verify(alg, &key, &tag, &data));
        assert_eq!(
            hmac_verify(alg, &key, &tag[..bits / 8], &data),
            bits == tag.len() * 8
        );
        let mut flipped = tag.clone();
        flipped[0] ^= 1;
        assert!(!hmac_verify(alg, &key, &flipped, &data));
    }
}

#[test]
fn hkdf_rfc5869_and_pbkdf2_rfc7914() {
    for r in rows(include_str!("../../../tests/vectors/hkdf.txt")) {
        let mut okm = vec![0; r[5].parse().unwrap()];
        hkdf_derive(
            code(r[1]),
            &unhex(r[2]),
            &unhex(r[3]),
            &unhex(r[4]),
            &mut okm,
        );
        assert_eq!(hex(&okm), r[6], "case {}", r[0]);
    }
    for r in rows(include_str!("../../../tests/vectors/pbkdf2.txt")) {
        let mut dk = vec![0; r[4].parse().unwrap()];
        let rounds = r[3].parse().unwrap();
        pbkdf2_derive(
            code(r[0]),
            r[1].as_bytes(),
            r[2].as_bytes(),
            rounds,
            &mut dk,
        );
        assert_eq!(hex(&dk), r[5], "{} {}", r[1], r[3]);
    }
}

#[test]
fn builtins_drop_their_arguments() {
    let known = Known {
        option: None,
        result: None,
        pair: None,
        int_pair: None,
    };
    let mut rt = rt_with(&[], known);
    let base = rt.stats().live_objects;
    let bytes = |rt: &mut sakura_rt::Rt, b: &[u8]| {
        let p = rt.new_bytes(b.len() as u32).unwrap();
        rt.heap
            .mem
            .bytes_mut(p + 12, b.len() as u32)
            .copy_from_slice(b);
        p
    };
    let d = bytes(&mut rt, b"abc");
    let d = rt.crypto_digest(0, d).unwrap();
    assert_eq!(
        hex(rt.data_bytes(d)),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    rt.drop(d);
    let (k, m) = (bytes(&mut rt, b"Jefe"), bytes(&mut rt, b"msg"));
    let tag = rt.crypto_hmac(0, k, m).unwrap();
    let want = hmac(0, b"Jefe", b"msg");
    assert_eq!(rt.data_bytes(tag), want.as_slice());
    let (k, m) = (bytes(&mut rt, b"Jefe"), bytes(&mut rt, b"msg"));
    assert!(rt.crypto_hmac_verify(0, k, tag, m));
    let (ikm, salt, info) = (
        bytes(&mut rt, b"k"),
        bytes(&mut rt, b""),
        bytes(&mut rt, b""),
    );
    let okm = rt.crypto_hkdf(1, ikm, salt, info, 48).unwrap();
    assert_eq!(rt.data_bytes(okm).len(), 48);
    rt.drop(okm);
    let (pw, salt) = (bytes(&mut rt, b"pw"), bytes(&mut rt, b"salt"));
    let dk = rt.crypto_pbkdf2(2, pw, salt, 2, 20).unwrap();
    let mut want = [0; 20];
    pbkdf2_derive(2, b"pw", b"salt", 2, &mut want);
    assert_eq!(rt.data_bytes(dk), &want[..]);
    rt.drop(dk);
    // A length past the heap budget is `D096`, and the arguments are still dropped.
    let (pw, salt) = (bytes(&mut rt, b"pw"), bytes(&mut rt, b"salt"));
    assert_eq!(
        rt.crypto_pbkdf2(0, pw, salt, 1, 1 << 33),
        Err(sakura_rt::Defect::HEAP_EXHAUSTED)
    );
    assert_eq!(rt.stats().live_objects, base);
}
