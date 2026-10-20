//! `crypto.rs` (native only) on the official vector tables of `compiler/tests/vectors` (the JS
//! runtime's `runtime.test.js` reads the same files), then once through the `Rt` builtins, which
//! must drop their arguments.
#![cfg(feature = "native")]

mod common;

use common::{Known, Ty, rt_with};
use sakura_rt::crypto::{
    aes_gcm, digest, ecdh, hkdf_derive, hmac, hmac_verify, pbkdf2_derive, public_of, public_valid,
    sign, sign_verify,
};
use sakura_rt::layout::WORD;

/// `std/crypto`'s `curve(alg)`.
const P256: i64 = 0;
const ED25519: i64 = 1;
const X25519: i64 = 2;

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

// The tier 1b tables. A failing check names the row, never a key or a signature.

#[test]
fn aes_gcm_nist_cavp() {
    let (mut sealed, mut refused) = (0, 0);
    for r in rows(include_str!("../../../tests/vectors/aes_gcm.txt")) {
        let (key, iv, pt, aad) = (unhex(r[2]), unhex(r[3]), unhex(r[4]), unhex(r[5]));
        let ct = [unhex(r[6]), unhex(r[7])].concat();
        let row = format!("{} {} iv {}", r[0], r[1], r[3]);
        if r[0] == "encrypt" {
            assert!(
                aes_gcm(&key, &iv, &aad, &pt, true) == Some(ct.clone()),
                "{row}"
            );
            assert!(aes_gcm(&key, &iv, &aad, &ct, false) == Some(pt), "{row}");
            sealed += 1;
        } else {
            assert!(aes_gcm(&key, &iv, &aad, &ct, false).is_none(), "{row}");
            refused += 1;
        }
    }
    assert_eq!((sealed, refused), (50, 50));
}

#[test]
fn ed25519_rfc8032() {
    let mut n = 0;
    for r in rows(include_str!("../../../tests/vectors/ed25519.txt")) {
        let (sk, pk, msg, sig) = (unhex(r[1]), unhex(r[2]), unhex(r[3]), unhex(r[4]));
        assert!(public_of(ED25519, &sk) == pk, "test {}", r[0]);
        assert!(sign(ED25519, &sk, &msg) == sig, "test {}", r[0]);
        assert!(sign_verify(ED25519, &pk, &sig, &msg), "test {}", r[0]);
        let mut flipped = sig.clone();
        flipped[63] ^= 1;
        assert!(!sign_verify(ED25519, &pk, &flipped, &msg), "test {}", r[0]);
        assert!(
            !sign_verify(ED25519, &pk, &sig[..63], &msg),
            "test {}",
            r[0]
        );
        n += 1;
    }
    assert_eq!(n, 5);
}

#[test]
fn x25519_rfc7748() {
    let mut n = 0;
    for r in rows(include_str!("../../../tests/vectors/x25519.txt")) {
        let (k, u, out) = (unhex(r[1]), unhex(r[2]), unhex(r[3]));
        assert!(ecdh(X25519, &k, &u) == out, "case {}", r[0]);
        if r[0].ends_with("-public") {
            assert!(public_of(X25519, &k) == out, "case {}", r[0]);
        }
        n += 1;
    }
    assert_eq!(n, 6);
    // A small-order point (here zero) gives the all-zero secret: no bytes, which `std/crypto`
    // answers as `ZeroSecret`.
    assert!(ecdh(X25519, &[7; 32], &[0; 32]).is_empty());
}

#[test]
fn ecdh_p256_rfc5903() {
    let r = rows(include_str!("../../../tests/vectors/ecdh_p256.txt"))
        .next()
        .unwrap();
    let (i, gi) = (unhex(r[0]), unhex(&format!("04{}{}", r[1], r[2])));
    let (rr, gr) = (unhex(r[3]), unhex(&format!("04{}{}", r[4], r[5])));
    assert!(public_of(P256, &i) == gi && public_of(P256, &rr) == gr);
    assert!(public_valid(P256, &gi) && public_valid(P256, &gr));
    assert!(ecdh(P256, &i, &gr) == unhex(r[6]));
    assert!(ecdh(P256, &rr, &gi) == unhex(r[6]));
    // Not keys: a zero scalar, the group order, a short scalar, a point off the curve.
    let order = unhex("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551");
    assert!(public_of(P256, &[0; 32]).is_empty() && public_of(P256, &order).is_empty());
    assert!(public_of(P256, &i[1..]).is_empty());
    let mut off = gi.clone();
    off[64] ^= 1;
    assert!(!public_valid(P256, &off) && !public_valid(P256, &gi[..33]));
}

#[test]
fn ecdsa_p256_rfc6979() {
    let mut n = 0;
    for r in rows(include_str!("../../../tests/vectors/ecdsa_p256.txt")) {
        let (x, msg) = (unhex(r[1]), unhex(r[4]));
        let public = unhex(&format!("04{}{}", r[2], r[3]));
        let sig = unhex(&format!("{}{}", r[6], r[7]));
        assert!(public_of(P256, &x) == public, "message {}", r[0]);
        // RFC 6979 signing is deterministic, so native's signature is the table's.
        assert!(sign(P256, &x, &msg) == sig, "message {}", r[0]);
        assert!(sign_verify(P256, &public, &sig, &msg), "message {}", r[0]);
        assert!(
            !sign_verify(P256, &public, &sig, b"other"),
            "message {}",
            r[0]
        );
        assert!(
            !sign_verify(P256, &public, &[0; 64], &msg),
            "message {}",
            r[0]
        );
        n += 1;
    }
    assert_eq!(n, 2);
}

#[test]
fn tier_1b_builtins_drop_their_arguments() {
    let types = [Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])])];
    let known = Known {
        option: Some(0),
        result: None,
        pair: None,
        int_pair: None,
    };
    let mut rt = rt_with(&types, known);
    let base = rt.stats().live_objects;
    let bytes = |rt: &mut sakura_rt::Rt, b: &[u8]| {
        let p = rt.new_bytes(b.len() as u32).unwrap();
        rt.heap
            .mem
            .bytes_mut(p + 12, b.len() as u32)
            .copy_from_slice(b);
        p
    };
    let (seed, msg) = ([9; 32], b"msg");
    let pk = public_of(ED25519, &seed);
    let p = bytes(&mut rt, &seed);
    let got = rt.crypto_public_of(ED25519, p).unwrap();
    assert!(rt.data_bytes(got) == pk.as_slice());
    rt.drop(got);
    let p = bytes(&mut rt, &pk);
    assert!(rt.crypto_public_valid(ED25519, p));
    let (p, m) = (bytes(&mut rt, &seed), bytes(&mut rt, msg));
    let sig = rt.crypto_sign(ED25519, p, m).unwrap();
    let (p, m) = (bytes(&mut rt, &pk), bytes(&mut rt, msg));
    assert!(rt.crypto_sign_verify(ED25519, p, sig, m));
    let x = public_of(X25519, &seed);
    let (p, q) = (bytes(&mut rt, &seed), bytes(&mut rt, &x));
    let shared = rt.crypto_ecdh(X25519, p, q).unwrap();
    assert_eq!(rt.data_bytes(shared).len(), 32);
    rt.drop(shared);
    let (key, nonce) = ([3; 16], [4; 12]);
    let args = [&key[..], &nonce, b"aad", b"plain"].map(|b| bytes(&mut rt, b));
    let ct = rt
        .crypto_aes_gcm_seal(args[0], args[1], args[2], args[3])
        .unwrap();
    let ct_copy = rt.data_bytes(ct).to_vec();
    let args = [&key[..], &nonce, b"aad"].map(|b| bytes(&mut rt, b));
    let pt = rt
        .crypto_aes_gcm_open(args[0], args[1], args[2], ct)
        .unwrap();
    assert_ne!(pt, rt.none());
    rt.drop(pt);
    // A wrong AAD fails the tag: `None`, and the arguments are still dropped.
    let args = [&key[..], &nonce, b"AAD", &ct_copy].map(|b| bytes(&mut rt, b));
    let pt = rt
        .crypto_aes_gcm_open(args[0], args[1], args[2], args[3])
        .unwrap();
    assert_eq!(pt, rt.none());
    assert_eq!(rt.stats().live_objects, base);
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
