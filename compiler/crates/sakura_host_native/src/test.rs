//! `sakura test --target native`: the WASM host's `runTests` and its `Test` imports
//! (`sakura_wasm_host.js`), as the native host's. Every test of the module (or those whose name
//! contains the filter) runs as a task under the runner's `Test` handler, reported with the
//! JSON records `test_runner.js` writes: one `test` per test, then a `test_summary`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use sakura_rt::native::{HandleId, Host, Hosted, Instance, Outcome};

use crate::{NativeHost, fs, run_task};

/// The running test's state and the runner's settings (WASM's `host.test` and `host.runner`).
#[derive(Default)]
pub struct Runner {
    pub seed: i64,
    /// `tests/__snapshots__`, absolute; empty without a project directory.
    pub snapshot_dir: String,
    pub update: bool,
    pub failures: Vec<String>,
    pub output: Vec<String>,
    pub module: String,
}

impl Runner {
    /// `tests/__snapshots__/<module>/<name>.snap` holds `rendered` and a newline; the problem,
    /// if any, is the test's failure (WASM's `snapshot`).
    pub fn snapshot(&mut self, name: &str, rendered: &str) {
        if let Some(problem) = self.check(name, rendered) {
            self.failures.push(problem);
        }
    }

    fn check(&self, name: &str, rendered: &str) -> Option<String> {
        if self.snapshot_dir.is_empty() {
            return Some("snapshots need a project directory".into());
        }
        let ok = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
        if name.is_empty() || !name.chars().all(ok) || name.starts_with('.') {
            let quoted = serde_json::Value::from(name).to_string();
            return Some(format!(
                "snapshot name {quoted} must be letters, digits, '_', '-' or '.'"
            ));
        }
        let mut file = PathBuf::from(&self.snapshot_dir);
        file.extend(self.module.split("::").skip(1));
        file.push(format!("{name}.snap"));
        let want = format!("{rendered}\n");
        let have = std::fs::read(&file)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        if have.as_deref() == Some(want.as_str()) {
            return None;
        }
        if self.update {
            if let Some(dir) = file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&file, &want);
            return None;
        }
        let file = file.display();
        let Some(have) = have else {
            return Some(format!(
                "snapshot {name} does not exist yet; run `sakura test --update-snapshots` and review it"
            ));
        };
        if have.replace("\r\n", "\n") == want.replace("\r\n", "\n") {
            return Some(format!(
                "snapshot {name} differs from {file} only in line endings: `*.snap -text` in .gitattributes keeps git from converting snapshots; if the new value is right, run `sakura test --update-snapshots`"
            ));
        }
        Some(format!(
            "snapshot {name} differs from {file}:\n--- expected\n{have}--- actual\n{want}"
        ))
    }

    /// `Test.fixtures()`: `tests/fixtures` beside the snapshots, read-only (JS `fixtures_dir`;
    /// `--record` is the JS backend's), an empty memory directory without a project.
    pub fn fixtures(&self) -> Hosted<fs::Dir> {
        if self.snapshot_dir.is_empty() {
            return Ok(fs::Dir::memory(vec![]));
        }
        let dir = Path::new(&self.snapshot_dir).with_file_name("fixtures");
        let grants = fs::Grants {
            read: vec![PathBuf::from(".")],
            write: vec![],
        };
        // ponytail: a missing `tests/fixtures` is an empty directory here, where JS keeps a
        // directory whose every read fails; both answer every read with an `Err`.
        Ok(fs::Dir::root(&dir, Some(&grants)).unwrap_or_else(|_| fs::Dir::memory(vec![])))
    }

    /// `tests/regressions/<name>.json` beside the snapshots (JS `regressions.load`), if any.
    pub fn regression(&self, name: &str) -> Option<String> {
        if self.snapshot_dir.is_empty() {
            return None;
        }
        let dir = Path::new(&self.snapshot_dir).with_file_name("regressions");
        std::fs::read_to_string(dir.join(format!("{name}.json"))).ok()
    }
}

/// The test table (`sakura_codegen_native::tests_table`): the `Test` handler's clauses and
/// `(code, name, module)` per test.
pub struct Table {
    pub handler: (u32, u32, u32),
    pub tests: Vec<(u32, String, String)>,
}

/// The table in `b`, or `None` when it is malformed.
pub fn parse_table(b: &[u8]) -> Option<Table> {
    fn word(b: &[u8], at: &mut usize) -> Option<u32> {
        let v = u32::from_le_bytes(b.get(*at..*at + 4)?.try_into().ok()?);
        *at += 4;
        Some(v)
    }
    fn text(b: &[u8], at: &mut usize) -> Option<String> {
        let len = word(b, at)? as usize;
        let s = std::str::from_utf8(b.get(*at..*at + len)?)
            .ok()?
            .to_string();
        *at += len;
        Some(s)
    }
    let at = &mut 0;
    let handler = (word(b, at)?, word(b, at)?, word(b, at)?);
    let mut tests = vec![];
    for _ in 0..word(b, at)? {
        tests.push((word(b, at)?, text(b, at)?, text(b, at)?));
    }
    (*at == b.len()).then_some(Table { handler, tests })
}

/// One stdout line: a record with `schema_version` first, as `runTests`' `emit`.
fn emit(host: &mut NativeHost, fields: &[(&str, serde_json::Value)]) {
    let body: Vec<String> = std::iter::once(("schema_version", serde_json::json!("1.0")))
        .chain(fields.iter().cloned())
        .map(|(k, v)| format!("{}:{v}", serde_json::Value::from(k)))
        .collect();
    host.console_print(&format!("{{{}}}", body.join(",")));
}

/// Run the table's tests on `inst` with the runner's arguments `<seed> <filter>
/// <snapshot-dir> <update: 0|1>`; returns the exit code: 3 if any failed.
pub fn run_tests(inst: &mut Instance<NativeHost>, table: &Table, args: &[String]) -> i32 {
    let seed: i64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let filter = args.get(1).map_or("", String::as_str);
    inst.host().runner = Runner {
        seed,
        snapshot_dir: args.get(2).cloned().unwrap_or_default(),
        update: args.get(3).is_some_and(|u| u == "1"),
        ..Runner::default()
    };
    let (effect, codes, n) = table.handler;
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    for (code, name, module) in &table.tests {
        if !filter.is_empty() && !name.contains(filter) {
            skipped += 1;
            continue;
        }
        let r = &mut inst.host().runner;
        r.failures.clear();
        r.output.clear();
        r.module.clone_from(module);
        let start = Instant::now();
        let o = match inst.start_test(*code, seed, effect, codes, n) {
            Ok(task) => {
                let o = run_task(inst, task);
                inst.task_free(task);
                o
            }
            Err(d) => Outcome::Defect {
                code: d.code,
                site: 0,
                message: d.message.into(),
            },
        };
        let duration_ms = start.elapsed().as_millis() as u64;
        let host = inst.host();
        host.flush_out();
        let mut failures = std::mem::take(&mut host.runner.failures);
        let output = std::mem::take(&mut host.runner.output);
        let (status, defect) = match &o {
            Outcome::Defect { code, message, .. } => (
                "defect",
                serde_json::json!({"code": format!("D{code:03}"), "message": message}),
            ),
            Outcome::Fail(shown) => {
                failures.push(format!("test failed with {shown}"));
                ("failed", serde_json::Value::Null)
            }
            _ if !failures.is_empty() => ("failed", serde_json::Value::Null),
            _ => ("passed", serde_json::Value::Null),
        };
        if status == "passed" {
            passed += 1;
        } else {
            failed += 1;
        }
        emit(
            host,
            &[
                ("kind", "test".into()),
                ("name", name.as_str().into()),
                ("module", module.as_str().into()),
                ("status", status.into()),
                ("seed", seed.into()),
                ("generator", "sakura-gen-v1".into()),
                ("duration_ms", duration_ms.into()),
                ("failures", failures.into()),
                ("output", output.into()),
                ("defect", defect),
            ],
        );
    }
    let host = inst.host();
    emit(
        host,
        &[
            ("kind", "test_summary".into()),
            ("passed", passed.into()),
            ("failed", failed.into()),
            ("skipped", skipped.into()),
            ("total", table.tests.len().into()),
            ("seed", seed.into()),
        ],
    );
    host.flush_out();
    if failed > 0 { 3 } else { 0 }
}

impl NativeHost {
    pub(crate) fn test_fixtures_handle(&mut self) -> Hosted<HandleId> {
        let d = self.runner.fixtures()?;
        Ok(self.dirs.insert(d))
    }
}
