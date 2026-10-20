//! Design §17.288 (9): `THIRD_PARTY.md` has a row for every dependency of `sakura_host_native`
//! and of `sakura_rt` (its `native` feature's optional ones included), and every notice it links
//! exists. The manifests are read by hand: a `[dependencies]` table's keys, workspace crates
//! (`sakura_*`) left out.

use std::path::Path;

fn dependencies(manifest: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(manifest).unwrap();
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == "[dependencies]";
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            let name = line.split(['=', '.']).next().unwrap().trim();
            if !name.starts_with("sakura_") {
                out.push(name.to_owned());
            }
        }
    }
    out
}

#[test]
fn every_dependency_has_a_notice_row() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let notices = std::fs::read_to_string(here.join("THIRD_PARTY.md")).unwrap();
    let mut deps = dependencies(&here.join("Cargo.toml"));
    deps.extend(dependencies(&here.join("../../runtime/rt/Cargo.toml")));
    assert!(
        deps.iter().any(|d| d == "p256"),
        "the manifests were not read: {deps:?}"
    );
    let missing: Vec<_> = deps
        .iter()
        .filter(|d| !notices.contains(&format!("\n| [{d}](https://crates.io/crates/{d}/")))
        .collect();
    assert!(
        missing.is_empty(),
        "THIRD_PARTY.md has no row for {missing:?}"
    );
    for link in notices.split("](third-party/").skip(1) {
        let file = &link[..link.find(')').unwrap()];
        assert!(
            here.join("third-party").join(file).exists(),
            "missing notice third-party/{file}"
        );
    }
}
