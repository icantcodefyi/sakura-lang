//! `sakura.toml` (design §6.2, §9): package identity and the `[permissions]` table that caps the
//! root handlers `sakura run` installs. `[dependencies]` is reserved and refused (`V8005`, §17.232).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Analyzed;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub package: Package,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub targets: BTreeMap<String, toml::Value>,
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
    /// Programs `Process` may start.
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

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        if dependencies_key(text).is_some() {
            return Err(dependencies_message());
        }
        toml::from_str(text).map_err(|e| e.to_string().trim_end().to_owned())
    }
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
    format!(
        "this project declares [dependencies]; Sakura {} compiles one package over std, and \
         packages arrive in a later minor release",
        env!("CARGO_PKG_VERSION")
    )
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
    /// The user package's `sakura.toml`, when the snapshot has one.
    pub fn manifest(&self) -> Option<Result<Manifest, String>> {
        let file = self
            .snapshot
            .files()
            .iter()
            .find(|f| crate::disk::is_manifest(&f.uri))?;
        Some(Manifest::parse(&file.text))
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
            assert_eq!(Manifest::parse(text), Err(dependencies_message()), "{text}");
        }
        assert_eq!(dependencies_key("[package]\nname = \"x\"\n"), None);
        assert_eq!(dependencies_key("[dependencies\n"), None);
        let malformed = Manifest::parse("[package]\nname = \"x\n[dependencies]\n").unwrap_err();
        assert!(malformed.starts_with("TOML parse error"), "{malformed}");
    }
}
