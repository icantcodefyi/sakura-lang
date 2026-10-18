//! Proof cache keys and authenticated records (design §17.86, §17.98; research/09 §8).
//!
//! The key is the domain-separated digest `sakura-proof-v1` of the normalised VC (every query's
//! SMT text, which contains the inlined definitions and the assumed summaries), the function's
//! own Core body without spans, the bodies of inlined callees, the profile, the encoder version,
//! the solver identity and options, and the trust policy. A summary contributes only its
//! contract (already in the VC), so a caller that used only a callee's contract keeps its key when
//! the callee's body changes.
//!
//! A persisted record removes a check only if its HMAC-SHA256 verifies under the per-installation
//! key `$XDG_STATE_HOME/sakura/proof.key` (mode 0600, outside the repository and `target/`). Any
//! other record is a lookup hint: the solver reruns. An authenticated record also keeps the
//! solver's answers, so `sakura verify` rebuilds its obligation record from them instead of
//! solving again ([`Cache::reuse`]).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::{ContractId, Def, DefId, Local, Module};
use crate::record::{Answer, Model, Reason, ReasonKind, Record, SolverInfo, Status, decide, hex};
use crate::smt;
use crate::vc::{self, DefVc, Kind, Obligation, Profile};

pub const DOMAIN: &str = "sakura-proof-v1";
/// Bumped whenever the encoding of a VC changes meaning.
pub const ENCODER: &str = "sakura_verify/vc-1";
const MAX_RECORD: u64 = 64 * 1024;

/// A def's Core with every span removed: moving code does not change a proof.
fn body_digest(d: &Def) -> String {
    fn strip(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                m.remove("pos");
                m.values_mut().for_each(strip);
            }
            serde_json::Value::Array(xs) => xs.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut v = serde_json::to_value(d).expect("def serialises");
    strip(&mut v);
    hex(&Sha256::digest(v.to_string().as_bytes()))
}

/// The cache key of one obligation.
pub fn key(m: &Module, vc: &DefVc, o: &Obligation, solver: &SolverInfo, trust: &str) -> String {
    let mut h = Sha256::new();
    let mut field = |s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    field(DOMAIN);
    field(ENCODER);
    field(vc.profile.name());
    field(&o.clause);
    for q in smt::queries(o) {
        field(&smt::script(vc, o, q));
    }
    field(&body_digest(m.def(vc.def)));
    for dep in &vc.inlined {
        match m.defs.iter().find(|d| &d.symbol_id() == dep) {
            Some(d) => field(&format!("{dep}={}", body_digest(d))),
            None => field(dep),
        }
    }
    field(&serde_json::to_string(solver).expect("solver info serialises"));
    field(trust);
    hex(&h.finalize())
}

/// HMAC-SHA256 (RFC 2104).
pub fn hmac(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let pad = |b: u8| k.map(|x| x ^ b);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

fn equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The per-installation MAC key.
pub struct ProofKey([u8; 32]);

impl ProofKey {
    /// `$XDG_STATE_HOME/sakura/proof.key`, else `$HOME/.local/state/sakura/proof.key`.
    pub fn default_path() -> Option<PathBuf> {
        let state = std::env::var_os("XDG_STATE_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        Some(state.join("sakura").join("proof.key"))
    }

    /// Loads the key, creating it (32 bytes from the OS, mode 0600) on first use. Refuses a key
    /// inside any of `forbidden` (the repository, `target/`) or readable by other users.
    #[cfg(unix)]
    pub fn load_or_create(path: &Path, forbidden: &[&Path]) -> Result<ProofKey, String> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        let dir = path.parent().ok_or("key path has no directory")?;
        // Resolve the nearest existing ancestor before creating anything.
        let existing = dir
            .ancestors()
            .find(|a| a.exists())
            .ok_or("no existing ancestor")?;
        let canon = existing
            .canonicalize()
            .map_err(|e| e.to_string())?
            .join(dir.strip_prefix(existing).unwrap_or(Path::new("")));
        for root in forbidden {
            if root.canonicalize().is_ok_and(|r| canon.starts_with(r)) {
                return Err(format!(
                    "{} is inside {}; the proof key must live outside it",
                    path.display(),
                    root.display()
                ));
            }
        }
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        match fs::read(path) {
            Ok(bytes) => {
                let mode = fs::metadata(path)
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .mode();
                if mode & 0o077 != 0 {
                    return Err(format!(
                        "{} is accessible to other users (mode {:o})",
                        path.display(),
                        mode & 0o777
                    ));
                }
                let key: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| format!("{} is not a 32-byte key", path.display()))?;
                Ok(ProofKey(key))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut key = [0u8; 32];
                fs::File::open("/dev/urandom")
                    .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut key))
                    .map_err(|e| format!("/dev/urandom: {e}"))?;
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                f.write_all(&key).map_err(|e| e.to_string())?;
                Ok(ProofKey(key))
            }
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    #[cfg(not(unix))]
    pub fn load_or_create(_: &Path, _: &[&Path]) -> Result<ProofKey, String> {
        Err("no protected key storage on this platform; proofs are rerun".into())
    }

    #[cfg(test)]
    pub(crate) fn from_bytes(b: [u8; 32]) -> Self {
        ProofKey(b)
    }
}

/// One answer of a `proved` or `disproved` verdict, in the order [`decide`] asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Unsat,
    Sat(Model),
}

/// A persisted verdict. `mac` covers every other field in this order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub status: Status,
    pub solver: SolverInfo,
    /// The solver every `unsat` was re-asked of, if any.
    pub cross_check: Option<SolverInfo>,
    /// sha256 of the record the verdict came from, before any replay and without its span.
    pub transcript: String,
    pub dependencies: Vec<String>,
    pub replies: Vec<Reply>,
    pub mac: String,
}

/// Moving code changes a record's span but not its key, so the span is left out.
fn transcript(r: &Record) -> String {
    let r = Record {
        span: None,
        ..r.clone()
    };
    hex(&Sha256::digest(
        serde_json::to_string(&r).expect("record").as_bytes(),
    ))
}

impl Entry {
    /// The entry for record `r` as [`decide`] returned it from `answers` (before any replay).
    pub fn new(key: String, r: &Record, answers: &[Answer], k: Option<&ProofKey>) -> Option<Entry> {
        let solver = r.solver.clone()?;
        if !matches!(r.status, Status::Proved | Status::Disproved) {
            // `unknown` is history, never a reusable result (§17.86).
            return None;
        }
        let replies = answers
            .iter()
            .map(|a| match a {
                Answer::Unsat => Some(Reply::Unsat),
                Answer::Sat(m) => Some(Reply::Sat(m.clone())),
                Answer::Unknown(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let mut dependencies: Vec<String> = r.summaries.iter().chain(&r.inlined).cloned().collect();
        dependencies.sort();
        let mut e = Entry {
            key,
            status: r.status,
            solver,
            cross_check: r.cross_check.clone(),
            transcript: transcript(r),
            dependencies,
            replies,
            mac: String::new(),
        };
        e.mac = k.map_or(String::new(), |k| hex(&hmac(&k.0, &e.signed())));
        Some(e)
    }

    fn signed(&self) -> Vec<u8> {
        let mut e = self.clone();
        e.mac.clear();
        serde_json::to_vec(&e).expect("entry serialises")
    }

    fn authentic(&self, k: &ProofKey) -> bool {
        equal(
            hex(&hmac(&k.0, &self.signed())).as_bytes(),
            self.mac.as_bytes(),
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Lookup {
    /// The MAC verifies: a `proved` entry may remove its check.
    Authenticated(Entry),
    /// Unsigned, foreign or tampered: use only to order work, then rerun the solver.
    Hint(Entry),
    Missing,
}

pub struct Cache {
    pub dir: PathBuf,
    pub key: Option<ProofKey>,
}

impl Cache {
    fn file(&self, key: &str) -> Option<PathBuf> {
        (key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| self.dir.join(format!("{key}.json")))
    }

    /// Writes atomically (temporary file, then rename) with mode 0600.
    pub fn put(&self, e: &Entry) -> std::io::Result<()> {
        let path = self
            .file(&e.key)
            .ok_or_else(|| std::io::Error::other("malformed key"))?;
        fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{}.tmp", e.key));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        opts.open(&tmp)?
            .write_all(&serde_json::to_vec(e).expect("entry"))?;
        fs::rename(tmp, path)
    }

    pub fn get(&self, key: &str) -> Lookup {
        let Some(path) = self.file(key) else {
            return Lookup::Missing;
        };
        let Ok(meta) = fs::metadata(&path) else {
            return Lookup::Missing;
        };
        if meta.len() > MAX_RECORD {
            return Lookup::Missing;
        }
        let Some(e) = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Entry>(&b).ok())
        else {
            return Lookup::Missing;
        };
        if e.key != key {
            return Lookup::Hint(e);
        }
        match &self.key {
            Some(k) if e.authentic(k) => Lookup::Authenticated(e),
            _ => Lookup::Hint(e),
        }
    }

    /// The record of `o` rebuilt from the authenticated entry under `key` without a solver: the
    /// entry's replies given to [`decide`] again. Only an entry obtained with `solver` and the same
    /// cross-check solver (or none) counts, and only when the rebuilt record has the entry's
    /// status and transcript and uses every reply. `None` (missing, unsigned, tampered, foreign
    /// or stale) means: solve it.
    pub fn reuse(
        &self,
        key: &str,
        vc: &DefVc,
        o: &Obligation,
        solver: &SolverInfo,
        cross: Option<&SolverInfo>,
    ) -> Option<Record> {
        let Lookup::Authenticated(e) = self.get(key) else {
            return None;
        };
        if e.solver != *solver || e.cross_check.as_ref() != cross {
            return None;
        }
        let mut replies = e.replies.iter();
        let mut r = decide(vc, o, Some(solver.clone()), |_, _| match replies.next() {
            Some(Reply::Unsat) => Answer::Unsat,
            Some(Reply::Sat(m)) => Answer::Sat(m.clone()),
            None => Answer::Unknown(Reason::new(ReasonKind::SolverUnknown, "no cached reply")),
        });
        r.cross_check = cross.cloned();
        (replies.next().is_none() && r.status == e.status && transcript(&r) == e.transcript)
            .then_some(r)
    }

    /// The one question the removal hook (V3) may ask.
    pub fn proves(&self, key: &str) -> bool {
        matches!(self.get(key), Lookup::Authenticated(e) if e.status == Status::Proved)
    }

    /// The runtime checks `sakura build` may drop on `profile` (§7.5, §17.84, §17.98), keyed by
    /// (def, local the `Check` binds), each with whether it may: a function's own `ensures` and
    /// `where` checks may go when [`Cache::proves`] every obligation of the check under the key
    /// recomputed from `m` now, so an edit to the function's body, a callee's contract or an
    /// inlined definition keeps it. Entry checks blamed on a caller (`requires`, a refined
    /// parameter through `T.from`) are not listed, because one proved call site says nothing about
    /// another, and neither is checked arithmetic, which Core has no unchecked form of.
    pub fn check_sites(
        &self,
        m: &Module,
        profile: Profile,
        solver: &SolverInfo,
        trust: &str,
    ) -> BTreeMap<(DefId, Local), CheckSite> {
        let mut sites: BTreeMap<(DefId, Local), CheckSite> = BTreeMap::new();
        for vc in vc::generate(m, profile) {
            for o in &vc.obligations {
                let (Some(contract), true, None) = (
                    o.contract,
                    matches!(o.kind, Kind::Ensures | Kind::Where),
                    o.callee,
                ) else {
                    continue;
                };
                let Some(local) = o.site.rsplit_once('%').and_then(|(_, l)| l.parse().ok()) else {
                    continue;
                };
                let proved = self.proves(&key(m, &vc, o, solver, trust));
                sites
                    .entry((vc.def, local))
                    .or_insert(CheckSite {
                        contract,
                        proved: true,
                    })
                    .proved &= proved;
            }
        }
        sites
    }
}

/// A runtime check [`Cache::check_sites`] considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckSite {
    pub contract: ContractId,
    /// Every obligation of the check has an authenticated `proved` record.
    pub proved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        assert_eq!(
            hex(&hmac(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_hashes_long_keys_first() {
        // RFC 4231 case 6: a 131-byte key.
        assert_eq!(
            hex(&hmac(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn tampering_turns_a_record_into_a_hint() {
        let dir = std::env::temp_dir().join(format!("sakura-verify-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = Cache {
            dir: dir.clone(),
            key: Some(ProofKey::from_bytes([7; 32])),
        };
        let k = "a".repeat(64);
        let mut e = Entry {
            key: k.clone(),
            status: Status::Proved,
            solver: SolverInfo {
                name: "z3".into(),
                version: "5.3.1".into(),
                timeout_ms: 2000,
                rlimit: 10_000_000,
                deadline_ms: 5000,
            },
            cross_check: None,
            transcript: "t".into(),
            dependencies: vec![],
            replies: vec![],
            mac: String::new(),
        };
        e.mac = hex(&hmac(&[7; 32], &e.signed()));
        cache.put(&e).unwrap();
        assert!(cache.proves(&k));
        // Someone flips the verdict and recomputes nothing: only a hint.
        let mut forged = e.clone();
        forged.status = Status::Disproved;
        cache.put(&forged).unwrap();
        assert!(matches!(cache.get(&k), Lookup::Hint(_)));
        // The same record under another installation's key is a hint too.
        cache.put(&e).unwrap();
        let other = Cache {
            dir: dir.clone(),
            key: Some(ProofKey::from_bytes([8; 32])),
        };
        assert!(!other.proves(&k));
        let unkeyed = Cache {
            dir: dir.clone(),
            key: None,
        };
        assert!(!unkeyed.proves(&k));
        assert_eq!(cache.get("../../etc/passwd"), Lookup::Missing);
        fs::write(dir.join(format!("{k}.json")), "{not json").unwrap();
        assert_eq!(cache.get(&k), Lookup::Missing);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn key_is_created_private_and_refused_inside_the_repo() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("sakura-verify-key-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let path = base.join("state/sakura/proof.key");
        let a = ProofKey::load_or_create(&path, &[]).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let b = ProofKey::load_or_create(&path, &[]).unwrap();
        assert_eq!(a.0, b.0, "the key is stable across loads");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            ProofKey::load_or_create(&path, &[]).is_err(),
            "a world-readable key is refused"
        );
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let inside = repo.join("target/proof.key");
        assert!(ProofKey::load_or_create(&inside, &[&repo]).is_err());
        assert!(
            !repo.join("target").exists(),
            "nothing is created inside the repository"
        );
        let _ = fs::remove_dir_all(&base);
    }
}
