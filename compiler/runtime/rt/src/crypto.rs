//! `std/crypto`'s primitives on native (RFC 0002 §2.7): SHA-2, HMAC, HKDF, PBKDF2, AES-GCM,
//! ECDSA and ECDH over P-256, Ed25519 and X25519 from the RustCrypto and dalek crates, no
//! primitive of our own. Each matches the JS runtime's `node:crypto`
//! function of the same name; `tests/vectors/` and the semantics fixtures hold the two equal.
//! `std/crypto` checks every argument (lengths, iterations) before it calls these, so they do not
//! fail. Arguments are taken owned and results returned owned (ABI.md P2). Native only: the
//! wasm32 object has no dependencies, and WASM reaches the host's implementation instead.
//!
//! Design §17.288 (7): every copy the `Rt` builtins make (each argument's bytes, each result
//! before it is copied into the heap) is a `Zeroizing<Vec<u8>>`, zeroed before it is freed.

use aes_gcm::aead::{self, Aead, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm};
use hmac::{EagerHash, Hmac, KeyInit, Mac};
// One `signature` crate serves p256 and ed25519-dalek.
use p256::ecdsa::signature::{Signer as _, Verifier as _};
use p256::elliptic_curve::sec1::ToSec1Point as _;
use sha2::{Sha256, Sha384, Sha512};
use zeroize::Zeroizing;

use crate::{Res, Rt};

/// `std/crypto`'s `code(digest)`: 0 SHA-256, 1 SHA-384, 2 SHA-512.
const SHA384: i64 = 1;
const SHA512: i64 = 2;

/// `std/crypto`'s `curve(alg)`: 0 P-256 (ECDSA and ECDH), 1 Ed25519, 2 X25519. A private key
/// is the scalar or seed (32 bytes), a public key the point: SEC1 uncompressed for P-256 (65
/// bytes), RFC 8032's and RFC 7748's encodings for the other two (32 bytes).
const ED25519: i64 = 1;
const X25519: i64 = 2;

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

fn seed(b: &[u8]) -> Option<Zeroizing<[u8; 32]>> {
    b.try_into().ok().map(Zeroizing::new)
}

/// `SecretKey::from_slice` left-pads a short scalar, so the length is checked first.
fn p256_secret(private: &[u8]) -> Option<p256::SecretKey> {
    (private.len() == 32)
        .then(|| p256::SecretKey::from_slice(private).ok())
        .flatten()
}

/// The uncompressed form only (`std/crypto` checks the length before it calls).
fn p256_public(public: &[u8]) -> Option<p256::PublicKey> {
    (public.len() == 65)
        .then(|| p256::PublicKey::from_sec1_bytes(public).ok())
        .flatten()
}

/// The public key of `private`, or no bytes when it is not a key of the curve (a P-256 scalar
/// of zero or at least the group order).
pub fn public_of(curve: i64, private: &[u8]) -> Vec<u8> {
    match curve {
        ED25519 => seed(private).map(|s| {
            let k = ed25519_dalek::SigningKey::from_bytes(&s);
            k.verifying_key().to_bytes().to_vec()
        }),
        X25519 => seed(private).map(|s| {
            let k = x25519_dalek::StaticSecret::from(*s);
            x25519_dalek::PublicKey::from(&k).to_bytes().to_vec()
        }),
        _ => p256_secret(private).map(|k| k.public_key().to_sec1_point(false).as_bytes().to_vec()),
    }
    .unwrap_or_default()
}

/// Whether `public` is a key of the curve: on P-256 an uncompressed SEC1 point on the curve;
/// Ed25519 and X25519 take any 32 bytes, as Web Crypto imports them.
pub fn public_valid(curve: i64, public: &[u8]) -> bool {
    match curve {
        ED25519 | X25519 => public.len() == 32,
        _ => p256_public(public).is_some(),
    }
}

/// The signature of `data`: ECDSA P-256 with SHA-256, deterministic (RFC 6979), as `r || s`
/// (IEEE P1363, Web Crypto's encoding); Ed25519 (RFC 8032). `std/crypto` passes only a key
/// `public_of` accepted, so these do not fail.
pub fn sign(curve: i64, private: &[u8], data: &[u8]) -> Vec<u8> {
    match curve {
        ED25519 => {
            let s = seed(private).expect("an Ed25519 seed");
            ed25519_dalek::SigningKey::from_bytes(&s)
                .sign(data)
                .to_bytes()
                .to_vec()
        }
        _ => {
            let k = p256_secret(private).expect("a P-256 scalar");
            let sig: p256::ecdsa::Signature = p256::ecdsa::SigningKey::from(k).sign(data);
            sig.to_bytes().to_vec()
        }
    }
}

/// Whether `sig` is a signature of `data` under `public`. A malformed key or signature (a
/// P-256 `r` or `s` of zero or at least the order, an Ed25519 point that does not decode) is
/// `false`. Ed25519 is RFC 8032's cofactorless check, the one OpenSSL makes.
pub fn sign_verify(curve: i64, public: &[u8], sig: &[u8], data: &[u8]) -> bool {
    match curve {
        ED25519 => {
            let key = seed(public).and_then(|p| ed25519_dalek::VerifyingKey::from_bytes(&p).ok());
            let sig = ed25519_dalek::Signature::from_slice(sig).ok();
            key.zip(sig)
                .is_some_and(|(k, s)| k.verify(data, &s).is_ok())
        }
        _ => {
            let key = p256_public(public).map(p256::ecdsa::VerifyingKey::from);
            let sig = p256::ecdsa::Signature::from_slice(sig).ok();
            key.zip(sig)
                .is_some_and(|(k, s)| k.verify(data, &s).is_ok())
        }
    }
}

/// The shared secret of `private` and `public`: P-256's x-coordinate (32 bytes) or X25519's
/// output (32 bytes). A small-order X25519 `public` gives the all-zero value, which is no bytes
/// here (checked in constant time), as Node refuses it.
pub fn ecdh(curve: i64, private: &[u8], public: &[u8]) -> Vec<u8> {
    match curve {
        X25519 => {
            let k = x25519_dalek::StaticSecret::from(*seed(private).expect("an X25519 scalar"));
            let p = x25519_dalek::PublicKey::from(*seed(public).expect("an X25519 point"));
            let shared = k.diffie_hellman(&p);
            if shared.was_contributory() {
                shared.to_bytes().to_vec()
            } else {
                vec![]
            }
        }
        _ => {
            let k = p256_secret(private).expect("a P-256 scalar");
            let p = p256_public(public).expect("a P-256 point");
            p256::ecdh::diffie_hellman(k.to_nonzero_scalar(), p.as_affine())
                .raw_secret_bytes()
                .to_vec()
        }
    }
}

fn gcm<C: KeyInit + Aead>(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    msg: &[u8],
    seal: bool,
) -> Option<Vec<u8>> {
    let c = C::new_from_slice(key).ok()?;
    let nonce = aead::Nonce::<C>::try_from(nonce).ok()?;
    let payload = Payload { msg, aad };
    if seal {
        c.encrypt(&nonce, payload).ok()
    } else {
        c.decrypt(&nonce, payload).ok()
    }
}

/// AES-GCM (NIST SP 800-38D) with a 96-bit nonce and a 128-bit tag under a 16- or 32-byte key:
/// sealing answers ciphertext `||` tag, opening the plaintext or `None` when the tag does not
/// verify.
pub fn aes_gcm(key: &[u8], nonce: &[u8], aad: &[u8], msg: &[u8], seal: bool) -> Option<Vec<u8>> {
    if key.len() == 16 {
        gcm::<Aes128Gcm>(key, nonce, aad, msg, seal)
    } else {
        gcm::<Aes256Gcm>(key, nonce, aad, msg, seal)
    }
}

impl Rt {
    /// The bytes of each argument, copied out, and every argument dropped. The copies are zeroed
    /// when they are freed.
    fn take_bytes<const N: usize>(&mut self, args: [u32; N]) -> [Zeroizing<Vec<u8>>; N] {
        let out = args.map(|b| Zeroizing::new(self.data_bytes(b).to_vec()));
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
        self.bytes_from(&Zeroizing::new(digest(alg, &data)))
    }

    pub fn crypto_hmac(&mut self, alg: i64, key: u32, data: u32) -> Res<u32> {
        let [key, data] = self.take_bytes([key, data]);
        self.bytes_from(&Zeroizing::new(hmac(alg, &key, &data)))
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

    pub fn crypto_public_of(&mut self, curve: i64, private: u32) -> Res<u32> {
        let [private] = self.take_bytes([private]);
        self.bytes_from(&Zeroizing::new(public_of(curve, &private)))
    }

    pub fn crypto_public_valid(&mut self, curve: i64, public: u32) -> bool {
        let [public] = self.take_bytes([public]);
        public_valid(curve, &public)
    }

    pub fn crypto_sign(&mut self, curve: i64, private: u32, data: u32) -> Res<u32> {
        let [private, data] = self.take_bytes([private, data]);
        self.bytes_from(&Zeroizing::new(sign(curve, &private, &data)))
    }

    pub fn crypto_sign_verify(&mut self, curve: i64, public: u32, sig: u32, data: u32) -> bool {
        let [public, sig, data] = self.take_bytes([public, sig, data]);
        sign_verify(curve, &public, &sig, &data)
    }

    pub fn crypto_ecdh(&mut self, curve: i64, private: u32, public: u32) -> Res<u32> {
        let [private, public] = self.take_bytes([private, public]);
        self.bytes_from(&Zeroizing::new(ecdh(curve, &private, &public)))
    }

    pub fn crypto_aes_gcm_seal(&mut self, key: u32, nonce: u32, aad: u32, pt: u32) -> Res<u32> {
        let [key, nonce, aad, pt] = self.take_bytes([key, nonce, aad, pt]);
        let ct = aes_gcm(&key, &nonce, &aad, &pt, true).map(Zeroizing::new);
        self.bytes_from(&ct.expect("std/crypto checks the key and the nonce"))
    }

    /// `Some(plaintext)`, or `None` when the tag does not verify.
    pub fn crypto_aes_gcm_open(&mut self, key: u32, nonce: u32, aad: u32, ct: u32) -> Res<u32> {
        let [key, nonce, aad, ct] = self.take_bytes([key, nonce, aad, ct]);
        match aes_gcm(&key, &nonce, &aad, &ct, false).map(Zeroizing::new) {
            Some(pt) => {
                let b = self.bytes_from(&pt)?;
                self.some(b)
            }
            None => Ok(self.none()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Design §17.288 (7): a freed copy cannot be read safely, so the type is the check: every
    /// argument copy is a `Zeroizing` wrapper, which zeroes its bytes on drop.
    #[test]
    fn take_bytes_returns_zeroizing_copies() {
        type Copies = [Zeroizing<Vec<u8>>; 1];
        let take: fn(&mut Rt, [u32; 1]) -> Copies = Rt::take_bytes::<1>;
        let mut rt = Rt::new(1 << 20).unwrap();
        let b = rt.bytes_from(b"key").unwrap();
        let [copy] = take(&mut rt, [b]);
        assert_eq!(copy.as_slice(), b"key");
    }
}
