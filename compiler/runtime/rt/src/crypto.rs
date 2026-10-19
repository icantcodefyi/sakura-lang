//! `std/crypto`'s primitives on native (RFC 0002 §2.7): SHA-2, HMAC, HKDF and PBKDF2 from the
//! RustCrypto crates, no primitive of our own. Each matches the JS runtime's `node:crypto`
//! function of the same name; `tests/vectors/` and the semantics fixtures hold the two equal.
//! `std/crypto` checks every argument (lengths, iterations) before it calls these, so they do not
//! fail. Arguments are taken owned and results returned owned (ABI.md P2). Native only: the
//! wasm32 object has no dependencies, and WASM reaches the host's implementation instead.

use hmac::{EagerHash, Hmac, KeyInit, Mac};
use sha2::{Sha256, Sha384, Sha512};

use crate::{Res, Rt};

/// `std/crypto`'s `code(digest)`: 0 SHA-256, 1 SHA-384, 2 SHA-512.
const SHA384: i64 = 1;
const SHA512: i64 = 2;

/// Calls `$f::<H>($args)` with `H` the hash `std/crypto` numbered `$alg`.
macro_rules! by_hash {
    ($alg:expr, $f:ident($($a:expr),*)) => {
        match $alg {
            SHA384 => $f::<Sha384>($($a),*),
            SHA512 => $f::<Sha512>($($a),*),
            _ => $f::<Sha256>($($a),*),
        }
    };
}

fn hash<H: EagerHash>(data: &[u8]) -> Vec<u8> {
    H::digest(data).to_vec()
}

fn keyed<H: EagerHash>(key: &[u8]) -> Hmac<H> {
    Hmac::<H>::new_from_slice(key).expect("HMAC takes a key of any length")
}

fn mac<H: EagerHash>(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = keyed::<H>(key);
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// `Mac::verify_slice` compares in constant time and refuses a tag of another length.
fn mac_verify<H: EagerHash>(key: &[u8], sig: &[u8], data: &[u8]) -> bool {
    let mut m = keyed::<H>(key);
    m.update(data);
    m.verify_slice(sig).is_ok()
}

fn hkdf<H: EagerHash>(ikm: &[u8], salt: &[u8], info: &[u8], okm: &mut [u8]) {
    hkdf::Hkdf::<H>::new(Some(salt), ikm)
        .expand(info, okm)
        .expect("std/crypto bounds the length by 255 hash lengths");
}

fn pbkdf2<H: EagerHash>(password: &[u8], salt: &[u8], rounds: u32, out: &mut [u8]) {
    pbkdf2::pbkdf2_hmac::<H>(password, salt, rounds, out);
}

/// The digest of `data` under hash `alg` (`crypto.subtle.digest`).
pub fn digest(alg: i64, data: &[u8]) -> Vec<u8> {
    by_hash!(alg, hash(data))
}

/// HMAC (RFC 2104) of `data` under `key` and hash `alg`.
pub fn hmac(alg: i64, key: &[u8], data: &[u8]) -> Vec<u8> {
    by_hash!(alg, mac(key, data))
}

/// Whether `sig` is the HMAC of `data`, compared in constant time.
pub fn hmac_verify(alg: i64, key: &[u8], sig: &[u8], data: &[u8]) -> bool {
    by_hash!(alg, mac_verify(key, sig, data))
}

/// HKDF (RFC 5869) into `okm`, at most 255 hash lengths.
pub fn hkdf_derive(alg: i64, ikm: &[u8], salt: &[u8], info: &[u8], okm: &mut [u8]) {
    by_hash!(alg, hkdf(ikm, salt, info, okm))
}

/// PBKDF2 with HMAC (RFC 8018 §5.2) into `out`, after `rounds` iterations.
pub fn pbkdf2_derive(alg: i64, password: &[u8], salt: &[u8], rounds: u32, out: &mut [u8]) {
    by_hash!(alg, pbkdf2(password, salt, rounds, out))
}

impl Rt {
    /// The bytes of each argument, copied out, and every argument dropped.
    fn take_bytes<const N: usize>(&mut self, args: [u32; N]) -> [Vec<u8>; N] {
        let out = args.map(|b| self.data_bytes(b).to_vec());
        for b in args {
            self.drop(b);
        }
        out
    }

    fn bytes_from(&mut self, b: &[u8]) -> Res<u32> {
        let len = u32::try_from(b.len()).map_err(|_| crate::Defect::HEAP_EXHAUSTED)?;
        let p = self.new_bytes(len)?;
        self.heap.mem.bytes_mut(p + 12, len).copy_from_slice(b);
        Ok(p)
    }

    pub fn crypto_digest(&mut self, alg: i64, data: u32) -> Res<u32> {
        let [data] = self.take_bytes([data]);
        self.bytes_from(&digest(alg, &data))
    }

    pub fn crypto_hmac(&mut self, alg: i64, key: u32, data: u32) -> Res<u32> {
        let [key, data] = self.take_bytes([key, data]);
        self.bytes_from(&hmac(alg, &key, &data))
    }

    pub fn crypto_hmac_verify(&mut self, alg: i64, key: u32, sig: u32, data: u32) -> bool {
        let [key, sig, data] = self.take_bytes([key, sig, data]);
        hmac_verify(alg, &key, &sig, &data)
    }

    /// The derived bytes are written in place in a `Bytes` the heap budget allows, so an
    /// absurd length is `D096`, never a host allocation.
    fn derived(&mut self, len: i64, f: impl FnOnce(&mut [u8])) -> Res<u32> {
        let len = u32::try_from(len).map_err(|_| crate::Defect::HEAP_EXHAUSTED)?;
        let p = self.new_bytes(len)?;
        f(self.heap.mem.bytes_mut(p + 12, len));
        Ok(p)
    }

    pub fn crypto_hkdf(&mut self, alg: i64, ikm: u32, salt: u32, info: u32, len: i64) -> Res<u32> {
        let [ikm, salt, info] = self.take_bytes([ikm, salt, info]);
        self.derived(len, |out| hkdf_derive(alg, &ikm, &salt, &info, out))
    }

    pub fn crypto_pbkdf2(
        &mut self,
        alg: i64,
        password: u32,
        salt: u32,
        rounds: i64,
        len: i64,
    ) -> Res<u32> {
        let [password, salt] = self.take_bytes([password, salt]);
        self.derived(len, |out| {
            pbkdf2_derive(alg, &password, &salt, rounds as u32, out)
        })
    }
}
