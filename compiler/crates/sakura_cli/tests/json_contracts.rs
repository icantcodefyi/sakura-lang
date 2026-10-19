//! Design §17.232 (b): every `--json` command, run over a success, a diagnostic and a failure
//! fixture, prints only records whose `kind → field → type` the baseline
//! `compiler/contracts/json.txt` lists (`kind schema_version field type required|optional`).
//! A field path is `a`, `a.b` for an object's field and `a[]` for an array's elements; the test
//! descends into an object or array only where the baseline lists its children, so a map with
//! open keys is one `object` line. An addition is a new baseline line in the same commit.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

const OK: &str = "pub fn main() -> Unit ! {Console} { Console.print(\"hi\") }\n\n\
    test \"adds\" { expect_eq(1 + 1, 2) }\n";

/// A fixable `V1004` (a fix record with edits), a type mismatch and an effect row mismatch
/// (`expected`/`actual`, the second with `row`) and a hole.
const DIAGNOSED: &str = "fn count() -> Int { list.len([1, 2]) }\n\
    fn bad() -> Int { \"no\" }\n\
    fn noisy() -> Unit { Console.print(\"x\") }\n\
    fn unfinished(x: Int) -> Int { ?result }\n";

const FIXABLE: &str = "fn count() -> Int { list.len([1, 2]) }\n";

const FAILING_TEST: &str = "test \"wrong\" { expect_eq(1 + 1, 3) }\n";

const AUDITED: &str = "import std/untrusted\n\n\
    type Percent = Int where it >= 0 && it <= 100\n\n\
    @total\nfn half(x: Int) -> Int { x / 2 }\n\n\
    pub fn main() -> Unit ! {Console} {\n  let p = Percent.from(50)\n  \
    let n = Untrusted.trust(Untrusted.of(half(p)), @trust(\"computed locally\"))\n  \
    Console.print(\"{n}\")\n}\n";

const PROVED: &str = "fn scale(p: Int, total: Int) -> Int\n  \
    requires total >= 0 && p >= 0 && p <= 100 && total <= 1000\n  \
    ensures result <= total\n{ total * p / 100 }\n\n\
    pub fn main() -> Unit { let _a = scale(50, 10) }\n";

const DISPROVED: &str = "fn scale(p: Int, total: Int) -> Int\n  \
    requires total >= 0 && p >= 0 && p <= 100 && total <= 1000\n  \
    ensures result <= total\n{ total * p / 50 }\n\n\
    pub fn main() -> Unit { let _a = scale(50, 10) }\n";

struct Case {
    name: &'static str,
    args: &'static [&'static str],
    source: Option<&'static str>,
    exit: i32,
}

const fn case(
    name: &'static str,
    args: &'static [&'static str],
    source: Option<&'static str>,
    exit: i32,
) -> Case {
    Case {
        name,
        args,
        source,
        exit,
    }
}

/// One success, one diagnostic and one failure per `--json` command; `main.sak` holds `source`.
const CASES: &[Case] = &[
    case("check_ok", &["check", "--json", "main.sak"], Some(OK), 0),
    case(
        "check_diagnostic",
        &["check", "--json", "main.sak"],
        Some(DIAGNOSED),
        1,
    ),
    case(
        "check_missing_path",
        &["check", "--json", "missing.sak"],
        None,
        2,
    ),
    case(
        "fix_applied",
        &["fix", "--json", "main.sak"],
        Some(FIXABLE),
        0,
    ),
    case(
        "fix_preview_diagnostic",
        &["fix", "--preview", "--json", "main.sak"],
        Some(DIAGNOSED),
        0,
    ),
    case(
        "fix_unknown_id",
        &["fix", "--json", "--apply", "f:nope", "main.sak"],
        Some(FIXABLE),
        2,
    ),
    case(
        "build_ok_manifest",
        &["build", "--json", "--manifest", "main.sak"],
        Some(OK),
        0,
    ),
    case(
        "build_diagnostic",
        &["build", "--json", "main.sak"],
        Some(DIAGNOSED),
        1,
    ),
    case(
        "build_missing_path",
        &["build", "--json", "missing.sak"],
        None,
        2,
    ),
    case("test_ok", &["test", "--json", "main.sak"], Some(OK), 0),
    case(
        "test_ok_wasm",
        &["test", "--json", "--target", "wasm", "main.sak"],
        Some(OK),
        0,
    ),
    case(
        "test_diagnostic",
        &["test", "--json", "main.sak"],
        Some(DIAGNOSED),
        1,
    ),
    case(
        "test_failing",
        &["test", "--json", "main.sak"],
        Some(FAILING_TEST),
        3,
    ),
    case(
        "audit_ok",
        &["audit", "--json", "main.sak"],
        Some(AUDITED),
        0,
    ),
    case(
        "audit_diagnostic",
        &["audit", "--json", "main.sak"],
        Some(DIAGNOSED),
        0,
    ),
    case(
        "audit_missing_path",
        &["audit", "--json", "missing.sak"],
        None,
        2,
    ),
    case(
        "doc_symbol",
        &["doc", "--json", "std::list::len"],
        Some(OK),
        0,
    ),
    case(
        "doc_diagnostic",
        &["doc", "--json", "count"],
        Some(DIAGNOSED),
        0,
    ),
    case(
        "doc_no_match",
        &["doc", "--json", "no_such_symbol"],
        Some(OK),
        0,
    ),
    case(
        "verify_proved",
        &["verify", "--json", "main.sak"],
        Some(PROVED),
        0,
    ),
    case(
        "verify_diagnostic",
        &["verify", "--json", "main.sak"],
        Some(DIAGNOSED),
        1,
    ),
    case(
        "verify_disproved",
        &["verify", "--json", "main.sak"],
        Some(DISPROVED),
        1,
    ),
];

#[derive(Debug)]
struct Line {
    types: BTreeSet<String>,
    required: bool,
}

/// `(kind, schema_version) → path → line` from `compiler/contracts/json.txt`.
type Baseline = BTreeMap<(String, String), BTreeMap<String, Line>>;

const TYPES: [&str; 7] = [
    "string", "integer", "number", "boolean", "null", "object", "array",
];

fn baseline() -> Baseline {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/json.txt");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut out = Baseline::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let [kind, version, field, types, req] = f[..] else {
            panic!("json.txt:{}: want 5 fields: {line}", n + 1);
        };
        let types: BTreeSet<String> = types.split('|').map(str::to_owned).collect();
        assert!(
            types.iter().all(|t| TYPES.contains(&t.as_str())),
            "json.txt:{}: types are {TYPES:?}: {line}",
            n + 1
        );
        assert!(
            matches!(req, "required" | "optional"),
            "json.txt:{}: {line}",
            n + 1
        );
        let fields = out.entry((kind.into(), version.into())).or_default();
        let dup = fields.insert(
            field.into(),
            Line {
                types,
                required: req == "required",
            },
        );
        assert!(
            dup.is_none(),
            "json.txt:{}: duplicate {kind} {version} {field}",
            n + 1
        );
    }
    out
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Checks `value` at `path` against `fields`, then its listed children; pushes every mismatch.
fn check(fields: &BTreeMap<String, Line>, path: &str, value: &Value, errors: &mut Vec<String>) {
    if !path.is_empty() {
        let Some(line) = fields.get(path) else {
            errors.push(format!("unlisted field {path} {}", type_of(value)));
            return;
        };
        let t = type_of(value);
        if !line.types.contains(t) && !(t == "integer" && line.types.contains("number")) {
            errors.push(format!("{path}: {t}, the baseline says {:?}", line.types));
        }
    }
    match value {
        Value::Object(map) => {
            let prefix = if path.is_empty() {
                String::new()
            } else {
                format!("{path}.")
            };
            if !path.is_empty() && !fields.keys().any(|k| k.starts_with(&prefix)) {
                return;
            }
            for (k, v) in map {
                check(fields, &join(path, k), v, errors);
            }
            let children = fields.iter().filter(|(k, _)| {
                k.strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.contains(['.', '[']))
            });
            for (k, line) in children {
                if line.required && !map.contains_key(&k[prefix.len()..]) {
                    errors.push(format!("required field {k} is missing"));
                }
            }
        }
        Value::Array(items) => {
            let element = format!("{path}[]");
            if fields.contains_key(&element) {
                for item in items {
                    check(fields, &element, item, errors);
                }
            }
        }
        _ => {}
    }
}

/// Z3 5.3.1 is required in CI (§17.137); locally `verify` cases are skipped without it.
fn has_z3() -> bool {
    let ok = std::process::Command::new("z3")
        .arg("--version")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("5.3.1"));
    assert!(
        ok || std::env::var_os("CI").is_none(),
        "CI requires Z3 5.3.1 on PATH"
    );
    ok
}

#[test]
fn every_json_record_matches_the_baseline() {
    let baseline = baseline();
    let z3 = has_z3();
    let root = std::env::temp_dir().join(format!("sakura-json-contracts-{}", std::process::id()));
    let cases: Vec<&Case> = CASES
        .iter()
        .filter(|c| z3 || c.args[0] != "verify")
        .collect();
    let runs = common::par_map(&cases, |c| {
        let dir = root.join(c.name);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(source) = c.source {
            std::fs::write(dir.join("main.sak"), source).unwrap();
        }
        let state = root.join(format!("{}-state", c.name));
        let out = common::sakura_env(c.args, &dir, &[("XDG_STATE_HOME", &state)]);
        let manifest = std::fs::read_to_string(dir.join("target/sakura/manifest.json")).ok();
        (out, manifest)
    });
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for (c, (out, manifest)) in cases.iter().zip(runs) {
        let at = |e: String| format!("{}: {e}", c.name);
        if out.code != Some(c.exit) {
            errors.push(at(format!(
                "exit {:?}, want {}\n{}",
                out.code, c.exit, out.stderr
            )));
        }
        let lines = out.stdout.lines().map(|l| (l, "stdout"));
        for (text, source) in lines.chain(manifest.as_deref().map(|m| (m, "manifest.json"))) {
            let record: Value = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(at(format!("{source}: not JSON ({e}): {text}")));
                    continue;
                }
            };
            let (Some(kind), Some(version)) =
                (record["kind"].as_str(), record["schema_version"].as_str())
            else {
                errors.push(at(format!(
                    "{source}: a record without kind and schema_version: {text}"
                )));
                continue;
            };
            let key = (kind.to_owned(), version.to_owned());
            let Some(fields) = baseline.get(&key) else {
                errors.push(at(format!(
                    "{source}: kind {kind} {version} is not in the baseline"
                )));
                continue;
            };
            seen.insert(key);
            let mut found = Vec::new();
            check(fields, "", &record, &mut found);
            errors.extend(
                found
                    .into_iter()
                    .map(|e| at(format!("{source}: {kind} {version}: {e}"))),
            );
        }
    }
    if z3 {
        for key in baseline.keys().filter(|k| !seen.contains(*k)) {
            errors.push(format!("no fixture emits {} {}", key.0, key.1));
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        errors.is_empty(),
        "{} contract errors:\n{}",
        errors.len(),
        errors.join("\n")
    );
}
