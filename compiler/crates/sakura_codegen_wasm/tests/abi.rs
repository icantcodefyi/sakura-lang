//! ABI §0, §10: the Node host this backend ships implements the revision the emitter and the
//! runtime object share (`runtime/rt/src/abi.rs`) and names every `stats()` counter.

use sakura_codegen_wasm::{ABI_VERSION, HOST_FILES, STATS};

#[test]
fn the_node_host_implements_the_shared_abi_revision_and_counters() {
    let host = HOST_FILES[0].1;
    assert!(
        host.contains(&format!("export const ABI_VERSION = 0x{ABI_VERSION:06x};")),
        "sakura_wasm_host.js's ABI_VERSION is not 0x{ABI_VERSION:06x}"
    );
    let list = host
        .split_once("export const STATS = [")
        .and_then(|(_, rest)| rest.split_once("];"))
        .expect("the host lists its counters")
        .0;
    let names: Vec<&str> = list
        .split(',')
        .map(|n| n.trim().trim_matches('"'))
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(names.len(), STATS, "{names:?}");
    assert_eq!(names.last(), Some(&"restarts"));
}
