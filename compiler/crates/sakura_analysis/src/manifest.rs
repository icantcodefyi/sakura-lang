//! `sakura.toml` (design §6.2, §9): package identity and the `[permissions]` table that caps the
//! root handlers `sakura run` installs, and (RFC 0003, experimental) `[dependencies]`, whose
//! entries [`dependencies`] reads (`V8030`; `V8005` for a form this release does not support), and
//! `[packages]`. Any other way the file fails to parse is `V8006` (§17.239).

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
    #[serde(default, skip_serializing_if = "Packages::is_unset")]
    pub packages: Packages,
    /// The valid entries of `[dependencies]` ([`dependencies`] reports the others).
    #[serde(skip)]
    pub dependencies: Vec<Dependency>,
}

/// `[packages]` (RFC 0003 §1.3): the consumer's package settings. P1 reads none of them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packages {
    pub registry: Option<String>,
    pub vendor: Option<String>,
    pub min_age: Option<String>,
}

impl Packages {
    fn is_unset(&self) -> bool {
        *self == Packages::default()
    }
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
    /// The minimum compiler and std minor, `MAJOR.MINOR` (RFC 0003 §1.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sakura: Option<String>,
    /// The effect ceiling: the std effects the package's `pub fn`s may require (`V3030`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// An SPDX expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// One valid `[dependencies]` entry (RFC 0003 §1.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub source: DependencySource,
    /// The ceiling the consumer accepts for this dependency (`V8032`); `[]` when absent.
    pub effects: Vec<String>,
    /// Whether the dependency may acquire authority (`V6020`).
    pub trusted: bool,
    /// The entry in the manifest, from its key to the end of its value.
    pub span: std::ops::Range<usize>,
    /// The entry's value is a string or an inline table, so a fix can rewrite the entry whole.
    pub inline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencySource {
    /// `>= floor` within the floor's major (its minor at major 0).
    Floor(Version),
    /// A directory relative to the manifest's, `/`-separated and without `.` or inner `..`.
    Path(String),
    Git {
        url: String,
        rev: String,
    },
}

impl Default for DependencySource {
    fn default() -> Self {
        DependencySource::Path(String::new())
    }
}

/// `MAJOR.MINOR.PATCH`, without pre-release or build metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.').map(|p| {
            let digits = !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
            (digits && (p == "0" || !p.starts_with('0')))
                .then(|| p.parse().ok())
                .flatten()
        });
        let v = Version {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
        };
        parts.next().is_none().then_some(v)
    }

    /// Whether `self` satisfies the floor `floor`: at least it, in the same major (the same minor
    /// at major 0), as RFC 0003 §4.1.
    pub fn satisfies(self, floor: Version) -> bool {
        let line = |v: Version| (v.major, if v.major == 0 { v.minor } else { 0 });
        self >= floor && line(self) == line(floor)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A `[dependencies]` entry that is not valid (`V8030`), or whose form this release does not
/// support (`unsupported`, `V8005`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyError {
    pub unsupported: bool,
    pub message: String,
    pub span: std::ops::Range<usize>,
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
    /// Everything; `dependencies` holds the valid entries of `[dependencies]`, which
    /// [`dependencies`] checks.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let invalid = |e: toml::de::Error| ManifestError {
            message: e.message().trim_end().to_owned(),
            span: e.span().unwrap_or(0..0),
        };
        let mut table = toml::de::DeTable::parse(text).map_err(invalid)?;
        table.get_mut().remove("dependencies");
        let at = |path: &[&str]| value_span(table.get_ref(), path).unwrap_or(0..0);
        let (process, sakura, min_age, registry, keywords) = (
            at(&["permissions", "process"]),
            at(&["package", "sakura"]),
            at(&["packages", "min_age"]),
            at(&["packages", "registry"]),
            at(&["package", "keywords"]),
        );
        let mut manifest =
            Manifest::deserialize(toml::de::Deserializer::from(table)).map_err(invalid)?;
        let refuse = |message: &str, span| {
            Err(ManifestError {
                message: message.to_owned(),
                span,
            })
        };
        // `process` is reserved (design §17.253 (3)): `[]` grants nothing, as leaving it out
        // does, and any other list is `V8006` at the value until an RFC adds a `Process` effect.
        if !manifest.permissions.process.is_empty() {
            return refuse(PROCESS_RESERVED, process);
        }
        let minor = |s: &str| {
            let mut parts = s.split('.');
            let number = |p: Option<&str>| {
                p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            };
            number(parts.next()) && number(parts.next()) && parts.next().is_none()
        };
        if manifest
            .package
            .sakura
            .as_deref()
            .is_some_and(|s| !minor(s))
        {
            return refuse(
                "`sakura` must be a `MAJOR.MINOR` version, e.g. \"1.2\"",
                sakura,
            );
        }
        if manifest.package.keywords.len() > 8 {
            return refuse("`keywords` lists at most 8 words", keywords);
        }
        let days = |s: &str| {
            s.strip_suffix('d')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        if manifest
            .packages
            .min_age
            .as_deref()
            .is_some_and(|s| !days(s))
        {
            return refuse(
                "`min_age` must be a whole number of days, e.g. \"3d\"",
                min_age,
            );
        }
        let url = |s: &str| s.starts_with("https://") || s.starts_with("file://");
        if manifest
            .packages
            .registry
            .as_deref()
            .is_some_and(|s| !url(s))
        {
            return refuse(
                "`registry` must be an `https://` or `file://` URL",
                registry,
            );
        }
        manifest.dependencies = dependencies(text).0;
        Ok(manifest)
    }
}

/// The entries of `[dependencies]` in `text`, valid ones and errors, in source order; both empty
/// without the table or when `text` is not TOML, whose parse error then stands alone (`V8006`).
pub fn dependencies(text: &str) -> (Vec<Dependency>, Vec<DependencyError>) {
    let (mut deps, mut errors) = (Vec::new(), Vec::new());
    let Ok(table) = toml::de::DeTable::parse(text) else {
        return (deps, errors);
    };
    let Some((key, value)) = (table.get_ref().iter()).find(|(k, _)| k.get_ref() == "dependencies")
    else {
        return (deps, errors);
    };
    let toml::de::DeValue::Table(entries) = value.get_ref() else {
        errors.push(DependencyError {
            unsupported: false,
            message: "`dependencies` must be a table of `name = ...` entries".to_owned(),
            span: key.span().start..value.span().end,
        });
        return (deps, errors);
    };
    let mut entries: Vec<_> = entries.iter().collect();
    entries.sort_by_key(|(k, _)| k.span().start);
    for (key, value) in entries {
        match dependency(text, key.get_ref(), key.span(), value) {
            Ok(d) => deps.push(d),
            Err(e) => errors.push(e),
        }
    }
    (deps, errors)
}

fn dependency(
    text: &str,
    name: &str,
    key: std::ops::Range<usize>,
    value: &toml::Spanned<toml::de::DeValue<'_>>,
) -> Result<Dependency, DependencyError> {
    use toml::de::DeValue;
    let span = key.start..value.span().end.max(key.end);
    let error = |message: String| DependencyError {
        unsupported: false,
        message,
        span: span.clone(),
    };
    let named = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !named {
        return Err(error(format!(
            "`{name}` is not a package name: lowercase letters, digits and `_`, starting with a \
             letter"
        )));
    }
    if crate::RESERVED_PACKAGES.contains(&name) {
        return Err(error(format!(
            "`{name}` is reserved and cannot name a dependency"
        )));
    }
    let floor = |text: &str| {
        Version::parse(text).ok_or_else(|| {
            error(format!(
                "`{text}` is not a version floor: write `MAJOR.MINOR.PATCH`, e.g. \"1.2.0\" (no \
                 `*`, ranges, pre-release or build metadata)"
            ))
        })
    };
    let mut dep = Dependency {
        name: name.to_owned(),
        span: span.clone(),
        ..Dependency::default()
    };
    let fields = match value.get_ref() {
        DeValue::String(s) => {
            dep.source = DependencySource::Floor(floor(s)?);
            dep.inline = true;
            return Ok(dep);
        }
        DeValue::Table(t) => {
            dep.inline = text[value.span()].starts_with('{');
            t
        }
        _ => {
            return Err(error(format!(
                "`{name}` must be a version floor string or a table"
            )));
        }
    };
    let (mut version, mut path, mut git, mut rev) = (None, None, None, None);
    for (k, v) in fields.iter() {
        let string = || match v.get_ref() {
            DeValue::String(s) => Ok(s.to_string()),
            _ => Err(error(format!("`{name}.{}` must be a string", k.get_ref()))),
        };
        match k.get_ref().as_ref() {
            "version" => version = Some(floor(&string()?)?),
            "path" => path = Some(string()?),
            "git" => git = Some(string()?),
            "rev" => rev = Some(string()?),
            "trusted" => match v.get_ref() {
                DeValue::Boolean(b) => dep.trusted = *b,
                _ => return Err(error(format!("`{name}.trusted` must be `true` or `false`"))),
            },
            "effects" => {
                let DeValue::Array(items) = v.get_ref() else {
                    return Err(error(format!(
                        "`{name}.effects` must be a list of effect names"
                    )));
                };
                for item in items {
                    match item.get_ref() {
                        DeValue::String(s) => dep.effects.push(s.to_string()),
                        _ => {
                            return Err(error(format!(
                                "`{name}.effects` must be a list of effect names"
                            )));
                        }
                    }
                }
            }
            other => {
                return Err(error(format!(
                    "unknown key `{other}` in dependency `{name}`; expected `version`, `path`, \
                     `git`, `rev`, `effects` or `trusted`"
                )));
            }
        }
    }
    dep.source = match (version, path, git) {
        (Some(v), None, None) => DependencySource::Floor(v),
        (None, Some(p), None) => DependencySource::Path(relative_path(&p).ok_or_else(|| {
            error(format!(
                "`{name}.path` must be a relative directory other than this package's, not `{p}`"
            ))
        })?),
        (None, None, Some(url)) => {
            let rev = rev.ok_or_else(|| {
                error(format!(
                    "`{name}` needs `rev`, the full 40-hex commit hash, beside `git`"
                ))
            })?;
            let hex = rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit());
            if !hex {
                return Err(error(format!(
                    "`{name}.rev` must be a full 40-hex commit hash, not `{rev}`"
                )));
            }
            if !url.starts_with("https://") {
                return Err(error(format!("`{name}.git` must be an `https://` URL")));
            }
            return Err(DependencyError {
                unsupported: true,
                message: format!(
                    "`git` dependencies are not supported in this version: `{name}` (RFC 0003; \
                     use a `path` dependency on a checkout of the commit)"
                ),
                span,
            });
        }
        (None, None, None) => {
            return Err(error(format!(
                "dependency `{name}` needs `version`, `path` or `git`"
            )));
        }
        _ => {
            return Err(error(format!(
                "dependency `{name}` names more than one of `version`, `path` and `git`"
            )));
        }
    };
    if rev.is_some() {
        return Err(error(format!("`{name}.rev` goes with `git` only")));
    }
    Ok(dep)
}

/// `p` as a `/`-separated relative directory with `.` segments removed and inner `..` folded
/// (`./a/../b/` is `b`), or `None` for an absolute path or one naming the manifest's own
/// directory.
pub fn relative_path(p: &str) -> Option<String> {
    let p = p.replace('\\', "/");
    let drive = p.as_bytes().get(1) == Some(&b':');
    if p.starts_with('/') || drive {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/').filter(|s| !s.is_empty() && *s != ".") {
        match (seg, out.last()) {
            ("..", Some(&last)) if last != ".." => _ = out.pop(),
            _ => out.push(seg),
        }
    }
    (!out.is_empty()).then(|| out.join("/"))
}

/// `V8006`'s message for a non-empty `[permissions] process` (design §17.253 (3)).
pub const PROCESS_RESERVED: &str = "`process` grants nothing in this version: no std effect \
                                    starts processes; the key is reserved";

/// The byte range of the value at `path` (`["permissions", "process"]`), in either the table or
/// the dotted form.
fn value_span(table: &toml::de::DeTable, path: &[&str]) -> Option<std::ops::Range<usize>> {
    let (first, rest) = path.split_first()?;
    let (_, value) = table.iter().find(|(k, _)| k.get_ref() == *first)?;
    match (rest, value.get_ref()) {
        ([], _) => Some(value.span()),
        (rest, toml::de::DeValue::Table(t)) => value_span(t, rest),
        _ => None,
    }
}

/// The byte range of the `dependencies` key, in any form (`[dependencies]`, empty or not, an
/// inline table, a dotted key), or `None` without one or when `text` is not TOML.
pub fn dependencies_key(text: &str) -> Option<std::ops::Range<usize>> {
    let table = toml::de::DeTable::parse(text).ok()?;
    let (key, _) = (table.get_ref().iter()).find(|(k, _)| k.get_ref() == "dependencies")?;
    Some(key.span())
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
                "Http" | "Serve" | "Sql" | "WebSocket" if self.net.is_empty() => "net",
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
    /// The root package's `sakura.toml`, when the snapshot has a valid one; an invalid one is
    /// `V8006` among the diagnostics (design §17.239).
    pub fn manifest(&self) -> Option<Manifest> {
        let file = crate::root_manifest(&self.snapshot)?;
        Manifest::parse(&self.snapshot.file(file).text).ok()
    }

    /// The packages of the build, the root first (RFC 0003): one, the root, for a program
    /// without dependencies.
    pub fn packages(&self) -> Vec<crate::BuildPackage> {
        let root = (self.program.packages.get(1)).map_or("main", |p| p.name.as_str());
        crate::packages::layout(&self.snapshot).packages(root)
    }

    /// The snapshot's `sakura.lock`, when it has one that parses.
    pub fn lock(&self) -> Option<crate::Lock> {
        crate::packages::layout(&self.snapshot).lock().cloned()
    }

    /// The std effects in `main`'s declared row (`Console`, `Fs`, …), if there is a `pub fn
    /// main` in the root package.
    pub fn main_effects(&self) -> Option<Vec<String>> {
        let root = self.program.packages.get(1)?;
        let main = self
            .symbols()
            .iter()
            .find(|s| s.package == root.name && s.name == "main" && s.kind == "fn" && s.is_pub)?;
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

    /// `sakura run` passes this serialization as `SAKURA_PERMISSIONS`: every key, always, so no
    /// driver-run program depends on how a host reads a missing one (design §17.304).
    #[test]
    fn the_driver_table_carries_every_key() {
        assert_eq!(
            serde_json::to_string(&Permissions::default()).unwrap(),
            r#"{"fs":{"read":[],"write":[]},"net":[],"env":[],"process":[]}"#
        );
    }

    /// RFC 0003 §1.2: every supported form parses, in source order, with its span.
    #[test]
    fn dependency_entries_parse() {
        let text = "[package]\nname = \"app\"\n[dependencies]\nzeta = \"1.2.0\"\nutil = { path = \"./libs/../util/\", effects = [\"Clock\"], trusted = true }\n\n[dependencies.old]\nversion = \"0.4.1\"\n";
        let (deps, errors) = dependencies(text);
        assert_eq!(errors, []);
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["zeta", "util", "old"]);
        let v = |major, minor, patch| Version {
            major,
            minor,
            patch,
        };
        assert_eq!(deps[0].source, DependencySource::Floor(v(1, 2, 0)));
        assert_eq!(&text[deps[0].span.clone()], "zeta = \"1.2.0\"");
        assert!(deps[0].inline && deps[1].inline && !deps[2].inline);
        assert_eq!(deps[1].source, DependencySource::Path("util".into()));
        assert_eq!(
            (deps[1].effects.as_slice(), deps[1].trusted),
            (&["Clock".to_owned()][..], true)
        );
        assert!(text[deps[1].span.clone()].starts_with("util = {"));
        assert!(text[deps[1].span.clone()].ends_with('}'));
        assert_eq!(deps[2].source, DependencySource::Floor(v(0, 4, 1)));
        assert_eq!(Manifest::parse(text).unwrap().dependencies, deps);
        // An empty or absent table is a program over std.
        for text in [
            "[package]\nname = \"x\"\n[dependencies]\n",
            "dependencies = {}\n[package]\nname = \"x\"\n",
            "[package]\nname = \"x\"\n",
        ] {
            assert_eq!(dependencies(text), (vec![], vec![]), "{text}");
        }
        assert_eq!(dependencies("[dependencies\n"), (vec![], vec![]));
        assert_eq!(
            dependencies_key("[package]\nname = \"x\"\n[dependencies]\n"),
            Some(22..34)
        );
    }

    /// RFC 0003 §1.2: each malformed entry is `V8030` naming its cause at the entry; a `git`
    /// entry is `V8005` until git sources land (plan P5).
    #[test]
    fn dependency_errors_name_their_cause() {
        for (entry, says, unsupported) in [
            ("Util = \"1.0.0\"", "not a package name", false),
            ("std = \"1.0.0\"", "reserved", false),
            ("util = \"*\"", "not a version floor", false),
            ("util = \"1.2\"", "not a version floor", false),
            ("util = \"1.2.0-beta.1\"", "not a version floor", false),
            ("util = \"01.2.0\"", "not a version floor", false),
            (
                "util = { version = \"1.0.0\", path = \"../u\" }",
                "more than one",
                false,
            ),
            ("util = { path = \"/abs/u\" }", "relative directory", false),
            ("util = { path = \"./\" }", "relative directory", false),
            (
                "util = { effects = [\"Clock\"] }",
                "needs `version`, `path` or `git`",
                false,
            ),
            ("util = { git = \"https://x/u\" }", "needs `rev`", false),
            (
                "util = { git = \"https://x/u\", rev = \"main\" }",
                "40-hex",
                false,
            ),
            (
                "util = { git = \"https://x/u\", branch = \"main\" }",
                "unknown key `branch`",
                false,
            ),
            (
                "util = { path = \"../u\", rev = \"9f2c0b7e1a4d3c6b8e5f7a9d0c1b2e3f4a5b6c7d\" }",
                "goes with `git`",
                false,
            ),
            (
                "util = { path = \"../u\", trusted = \"yes\" }",
                "`true` or `false`",
                false,
            ),
            (
                "util = { path = \"../u\", effects = \"Clock\" }",
                "list of effect names",
                false,
            ),
            ("util = 3", "string or a table", false),
            (
                "util = { git = \"https://x/u\", rev = \"9f2c0b7e1a4d3c6b8e5f7a9d0c1b2e3f4a5b6c7d\" }",
                "not supported in this version",
                true,
            ),
        ] {
            let text =
                format!("[package]\nname = \"x\"\n[dependencies]\n{entry}\nok = \"1.0.0\"\n");
            let (deps, errors) = dependencies(&text);
            assert_eq!(deps.len(), 1, "{entry}");
            let [e] = &errors[..] else {
                panic!("{entry}: {errors:?}")
            };
            assert!(e.message.contains(says), "{entry}: {}", e.message);
            assert_eq!(e.unsupported, unsupported, "{entry}");
            assert_eq!(&text[e.span.clone()], entry, "{entry}");
            // The rest of the manifest is still valid.
            assert_eq!(
                Manifest::parse(&text).unwrap().dependencies,
                deps,
                "{entry}"
            );
        }
        let (_, errors) = dependencies("dependencies = 3\n[package]\nname = \"x\"\n");
        assert!(errors[0].message.contains("must be a table"));
    }

    #[test]
    fn floors_follow_cargo() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("1.4.0").satisfies(v("1.2.0")));
        assert!(!v("2.0.0").satisfies(v("1.2.0")));
        assert!(!v("1.1.9").satisfies(v("1.2.0")));
        assert!(v("0.4.3").satisfies(v("0.4.1")));
        assert!(!v("0.5.0").satisfies(v("0.4.1")));
        assert_eq!(v("10.0.3").to_string(), "10.0.3");
        assert_eq!(relative_path("..\\a\\.\\b\\..\\c"), Some("../a/c".into()));
        assert_eq!(relative_path("a/.."), None);
        assert_eq!(relative_path("C:/x"), None);
    }

    /// RFC 0003 §1.1, §1.3: the new keys parse; their shapes are `V8006` at the value.
    #[test]
    fn package_keys_and_packages_table() {
        let m = Manifest::parse("[package]\nname = \"lib\"\nsakura = \"1.2\"\neffects = [\"Http\"]\ndescription = \"d\"\nlicense = \"MIT\"\nrepository = \"https://x\"\nkeywords = [\"a\"]\n[packages]\nregistry = \"file:///r\"\nvendor = \"vendor\"\nmin_age = \"0d\"\n").unwrap();
        assert_eq!(m.package.effects.as_deref(), Some(&["Http".to_owned()][..]));
        assert_eq!(m.packages.min_age.as_deref(), Some("0d"));
        for (text, at) in [
            ("[package]\nname = \"x\"\nsakura = \"1\"\n", "\"1\""),
            (
                "[package]\nname = \"x\"\nkeywords = [\"1\",\"2\",\"3\",\"4\",\"5\",\"6\",\"7\",\"8\",\"9\"]\n",
                "[\"1\",\"2\",\"3\",\"4\",\"5\",\"6\",\"7\",\"8\",\"9\"]",
            ),
            (
                "[package]\nname = \"x\"\n[packages]\nmin_age = \"3 days\"\n",
                "\"3 days\"",
            ),
            (
                "[package]\nname = \"x\"\n[packages]\nregistry = \"http://r\"\n",
                "\"http://r\"",
            ),
        ] {
            let e = Manifest::parse(text).unwrap_err();
            assert_eq!(&text[e.span.clone()], at, "{text}: {}", e.message);
        }
        let e =
            Manifest::parse("[package]\nname = \"x\"\n[packages]\nmirror = \"m\"\n").unwrap_err();
        assert!(
            e.message.contains("unknown field `mirror`"),
            "{}",
            e.message
        );
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
