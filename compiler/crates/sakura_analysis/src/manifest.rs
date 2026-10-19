//! `sakura.toml` (design §6.2, §9): package identity and the `[permissions]` table that caps the
//! root handlers `sakura run` installs. `[dependencies]` is reserved and refused (`V8005`, §17.232);
//! any other way the file fails to parse is `V8006` (§17.239).

use serde::{Deserialize, Serialize};

use crate::Analyzed;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub package: Package,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub verify: Verify,
}

/// `[verify]`: `sakura verify` settings (design §17.106).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verify {
    /// Z3's resource limit per query, when the default is too small for this project;
    /// `sakura verify --rlimit` overrides it.
    pub rlimit: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub edition: String,
}

/// Root authority granted to `main` (design §6.2). Everything absent is denied.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub fs: FsPermissions,
    /// `host:port` pairs for `Http`, `Serve` and `Sql.postgres`.
    #[serde(default)]
    pub net: Vec<String>,
    /// Environment variables `Env` may read.
    #[serde(default)]
    pub env: Vec<String>,
    /// Reserved (design §17.253 (3)): always empty, since [`Manifest::parse`] refuses any other
    /// list with `V8006`; no std effect starts processes.
    #[serde(default)]
    pub process: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsPermissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

/// Why `sakura.toml` is not a valid manifest (`V8006`, design §17.239): the parser's message and
/// the byte range it points at (the key, the value or the syntax error).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub message: String,
    pub span: std::ops::Range<usize>,
}

impl Manifest {
    /// Everything but `dependencies`, which [`dependencies_key`] finds for `V8005`.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let invalid = |e: toml::de::Error| ManifestError {
            message: e.message().trim_end().to_owned(),
            span: e.span().unwrap_or(0..0),
        };
        let mut table = toml::de::DeTable::parse(text).map_err(invalid)?;
        table.get_mut().remove("dependencies");
        let process = process_value(table.get_ref());
        let manifest =
            Manifest::deserialize(toml::de::Deserializer::from(table)).map_err(invalid)?;
        // `process` is reserved (design §17.253 (3)): `[]` grants nothing, as leaving it out
        // does, and any other list is `V8006` at the value until an RFC adds a `Process` effect.
        if !manifest.permissions.process.is_empty() {
            return Err(ManifestError {
                message: PROCESS_RESERVED.to_owned(),
                span: process.unwrap_or(0..0),
            });
        }
        Ok(manifest)
    }
}

/// `V8006`'s message for a non-empty `[permissions] process` (design §17.253 (3)).
pub const PROCESS_RESERVED: &str = "`process` grants nothing in this version: no std effect \
                                    starts processes; the key is reserved";

/// The byte range of `[permissions] process`'s value, in either the table or the dotted form.
fn process_value(table: &toml::de::DeTable) -> Option<std::ops::Range<usize>> {
    let (_, permissions) = table.iter().find(|(k, _)| k.get_ref() == "permissions")?;
    let toml::de::DeValue::Table(permissions) = permissions.get_ref() else {
        return None;
    };
    let (_, value) = permissions.iter().find(|(k, _)| k.get_ref() == "process")?;
    Some(value.span())
}

/// The byte range of the `dependencies` key, in any form (`[dependencies]`, empty or not, an
/// inline table, a dotted key), or `None` without one or when `text` is not TOML, whose parse
/// error then stands alone (`V8005`, design §17.232).
pub fn dependencies_key(text: &str) -> Option<std::ops::Range<usize>> {
    let table = toml::de::DeTable::parse(text).ok()?;
    let (key, _) = (table.get_ref().iter()).find(|(k, _)| k.get_ref() == "dependencies")?;
    Some(key.span())
}

/// `V8005`'s message (design §17.232).
pub fn dependencies_message() -> String {
    "Sakura Lang does not support packages yet: `[dependencies]` is reserved and refused; \
     packages arrive in a later minor release"
        .to_owned()
}

impl Permissions {
    /// One canonical line, for interface deltas and frozen-spec comparison.
    pub fn render(&self) -> String {
        format!(
            "fs.read={:?} fs.write={:?} net={:?} env={:?} process={:?}",
            self.fs.read, self.fs.write, self.net, self.env, self.process
        )
    }

    /// Why `main` may not start with root handlers for `effects` (std effect names from its
    /// row), or `None` if every declared effect has the authority it needs. `sakura run` exits 75
    /// on a denial (§12.1) instead of failing at first use.
    pub fn startup_denial(&self, effects: &[String]) -> Option<String> {
        for effect in effects {
            let missing = match effect.as_str() {
                "Fs" if self.fs.read.is_empty() => "fs.read",
                "Http" | "Serve" | "Sql" if self.net.is_empty() => "net",
                "Process" if self.process.is_empty() => "process",
                _ => continue,
            };
            return Some(format!(
                "`main` declares `{effect}`, but sakura.toml grants no `{missing}` permission"
            ));
        }
        None
    }
}

impl Analyzed {
    /// The user package's `sakura.toml`, when the snapshot has a valid one; an invalid one is
    /// `V8006` among the diagnostics (design §17.239).
    pub fn manifest(&self) -> Option<Manifest> {
        let file = self
            .snapshot
            .files()
            .iter()
            .find(|f| crate::disk::is_manifest(&f.uri))?;
        Manifest::parse(&file.text).ok()
    }

    /// The std effects in `main`'s declared row (`Console`, `Fs`, …), if there is a `pub fn
    /// main` in the user package.
    pub fn main_effects(&self) -> Option<Vec<String>> {
        let main = self
            .symbols()
            .iter()
            .find(|s| s.package != "std" && s.name == "main" && s.kind == "fn" && s.is_pub)?;
        let scheme = self.typed.defs.get(&main.def)?;
        let sakura_types::Type::Fn { row, .. } = self.typed.interner.ty(scheme.ty) else {
            return None;
        };
        let mut names: Vec<String> = self
            .typed
            .interner
            .row_data(*row)
            .labels
            .iter()
            .filter_map(|l| match l.effect {
                sakura_types::Effect::Def(d) => Some(self.program.def(d).name.clone()),
                sakura_types::Effect::Fail => None,
            })
            .collect();
        names.sort();
        Some(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_design_example() {
        let m = Manifest::parse(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\nfs.read  = [\"./data\"]\nnet = [\"localhost:5432\"]\nprocess = []\n",
        )
        .unwrap();
        assert_eq!(m.package.name, "app");
        assert_eq!(m.permissions.fs.read, ["./data"]);
        assert_eq!(m.permissions.startup_denial(&["Fs".into()]), None);
        let deny = m
            .permissions
            .startup_denial(&["Console".into(), "Process".into()]);
        assert!(deny.unwrap().contains("`process`"));
        assert!(Manifest::parse("[package]\nname = \"x\"\n[permisions]\n").is_err());
    }

    #[test]
    fn dependencies_are_refused_in_every_form() {
        for text in [
            "[package]\nname = \"x\"\n[dependencies]\n",
            "[package]\nname = \"x\"\n[dependencies]\nhttp = \"1.0\"\n",
            "dependencies = {}\n[package]\nname = \"x\"\n",
            "dependencies.http = \"1.0\"\n[package]\nname = \"x\"\n",
            "[package]\nname = \"x\"\n[dependencies.http]\nversion = \"1.0\"\n",
        ] {
            let span = dependencies_key(text).expect(text);
            assert_eq!(&text[span], "dependencies", "{text}");
            assert_eq!(Manifest::parse(text).unwrap().package.name, "x", "{text}");
        }
        assert_eq!(dependencies_key("[package]\nname = \"x\"\n"), None);
        assert_eq!(dependencies_key("[dependencies\n"), None);
    }

    /// Design §17.253 (3): `process = []` parses and grants what the absent key grants; any
    /// other list is `V8006` with the reserved message at the value, in every spelling.
    #[test]
    fn process_is_reserved() {
        let absent = Manifest::parse("[package]\nname = \"x\"\n").unwrap();
        let empty = Manifest::parse("[package]\nname = \"x\"\n[permissions]\nprocess = []\n");
        assert_eq!(empty.unwrap(), absent);
        for (text, at) in [
            (
                "[package]\nname = \"x\"\n[permissions]\nprocess = [\"git\"]\n",
                "[\"git\"]",
            ),
            (
                "permissions.process = [\"git\", \"ls\"]\n[package]\nname = \"x\"\n",
                "[\"git\", \"ls\"]",
            ),
            (
                "permissions = { process = [\"sh\"] }\n[package]\nname = \"x\"\n",
                "[\"sh\"]",
            ),
        ] {
            let e = Manifest::parse(text).unwrap_err();
            assert_eq!(e.message, PROCESS_RESERVED, "{text}");
            assert_eq!(&text[e.span.clone()], at, "{text}");
        }
        // A value of the wrong type stays the parser's own error.
        let e = Manifest::parse("[package]\nname = \"x\"\n[permissions]\nprocess = \"git\"\n");
        assert!(e.unwrap_err().message.contains("invalid type"));
    }

    /// Design §17.239: every failure is the parser's message at the parser's span.
    #[test]
    fn invalid_manifests_point_at_the_problem() {
        for (text, at, says) in [
            (
                "[package]\nname = \"x\"\n[bogus]\n",
                "bogus",
                "unknown field `bogus`",
            ),
            (
                "[package]\nname = \"x\"\n[targets]\njs = {}\n",
                "targets",
                "unknown field `targets`",
            ),
            (
                "[package]\nname = \"x\"\nauthor = \"a\"\n",
                "author",
                "unknown field `author`",
            ),
            ("[package]\nname = 3\n", "3", "invalid type"),
            (
                "[package]\nversion = \"1\"\n",
                "[package]",
                "missing field `name`",
            ),
        ] {
            let e = Manifest::parse(text).unwrap_err();
            assert_eq!(&text[e.span.clone()], at, "{text}");
            assert!(e.message.contains(says), "{text}: {}", e.message);
        }
        let e = Manifest::parse("[package]\nname = \"x\n").unwrap_err();
        assert!(!e.message.contains("TOML parse error"), "{}", e.message);
        assert_eq!(e.span.start, 19);
    }
}
