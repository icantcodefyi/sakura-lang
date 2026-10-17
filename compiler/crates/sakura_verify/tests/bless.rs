//! Writes the committed Core snapshots `tests/core/<name>.json` from `tests/fixtures/<name>.sakura`
//! through the real frontend. Runs only under `SAKURA_BLESS=1`, so changes in M0–M3 lowering never
//! fail this crate's tests (plan §3.5); review every snapshot diff it produces.

#[test]
fn bless_core_snapshots() {
    sakura_core_snapshot::convert::bless_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
    );
}
