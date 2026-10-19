//! `compiler/examples/<name>/`: the design §15 projects (`examples/README.md`). In the project
//! directory, `// mode: run` runs `sakura run -- <args>` and compares stdout with `expected.stdout`
//! (each line an anchored regex under `// stdout-regex: true`), then `sakura test`; `// mode: test`
//! runs `sakura test`; `// mode: check` requires a clean `sakura check`.

mod common;

use common::*;
use std::collections::BTreeMap;
use std::path::Path;

fn run(dir: &Path, d: &BTreeMap<String, String>) -> Result<(), String> {
    let mode = d.get("mode").map_or("run", String::as_str);
    let ok = |args: &[&str]| {
        let out = sakura(args, dir);
        match out.code {
            Some(0) if !out.timed_out => Ok(out.stdout),
            _ => Err(format!(
                "`sakura {}` exited {:?}\nstdout:\n{}\nstderr:\n{}",
                args.join(" "),
                out.code,
                out.stdout,
                out.stderr
            )),
        }
    };
    if mode == "check" {
        return ok(&["check"]).map(|_| ());
    }
    if mode == "run" {
        let mut args = vec!["run", "--"];
        args.extend(d.get("args").map_or("", String::as_str).split_whitespace());
        let stdout = ok(&args)?;
        example_stdout_matches(dir, d, &stdout)?;
    }
    ok(&["test"]).map(|_| ())
}

#[test]
fn examples() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("src/main.sak").is_file())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no examples under {}", root.display());
    let mut report = Report::default();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let d = directives(&std::fs::read_to_string(dir.join("src/main.sak")).unwrap());
        match skip_reason(&d, EXAMPLES_MILESTONE) {
            Some(why) => report.skip(&name, why),
            None => report.settle(&name, &d, run(&dir, &d)),
        }
    }
    report.finish("examples");
}
