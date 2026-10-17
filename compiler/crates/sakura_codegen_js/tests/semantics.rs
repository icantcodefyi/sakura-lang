//! Runs `compiler/tests/semantics/*.sakura` (`// mode: run`, milestone ≤ M2) through the real
//! pipeline under node and compares stdout and the exit code.

mod common;

fn directive<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    src.lines()
        .take_while(|l| l.starts_with("//"))
        .find_map(|l| l.strip_prefix("// ")?.strip_prefix(key)?.strip_prefix(": "))
}

#[test]
fn semantics_corpus() {
    let dir = common::compiler_dir().join("tests/semantics");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sakura"))
        .collect();
    files.sort();
    let mut failures = Vec::new();
    let mut passed = 0;
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let milestone = directive(&src, "milestone").unwrap_or("M0");
        let mode = directive(&src, "mode").unwrap_or("run");
        if milestone > option_env!("SAKURA_MAX_MILESTONE").unwrap_or("M2") || mode == "check" {
            continue;
        }
        if mode == "test" {
            match common::test(&name, &src) {
                Err(e) => failures.push(format!("{name}: does not build:\n{e}")),
                Ok(r) if r.code == 0 => passed += 1,
                Ok(r) => failures.push(format!(
                    "{name}: tests failed (exit {})\n{}{}",
                    r.code, r.stdout, r.stderr
                )),
            }
            continue;
        }
        let expected = std::fs::read_to_string(path.with_extension("stdout")).unwrap_or_default();
        let exit: i32 = directive(&src, "expect-exit").map_or(0, |c| c.trim().parse().unwrap());
        match common::run(&name, &src) {
            Err(e) => failures.push(format!("{name}: does not build:\n{e}")),
            Ok(r)
                if r.stdout == expected
                    && r.code == exit
                    && directive(&src, "expect-defect").is_none_or(|d| {
                        r.stderr.contains(&format!("\"code\":\"{}\"", d.trim()))
                    }) =>
            {
                passed += 1
            }
            Ok(r) => failures.push(format!(
                "{name}: exit {} (want {exit})\n--- stdout\n{}--- want\n{expected}--- stderr\n{}",
                r.code, r.stdout, r.stderr
            )),
        }
    }
    eprintln!(
        "semantics corpus: {passed} passed, {} failed",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
