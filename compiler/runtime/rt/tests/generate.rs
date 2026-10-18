//! `sakura-gen-v1` (ABI §11): the SplitMix64 reference vector (research/09-evidence
//! `splitmix.mjs`), and streams equal to the JS runtime's for the same domains (the expected
//! values were printed by `runtime/js/sakura_runtime.js`'s `caseStream` and `taskStream`).

use sakura_rt::generate::{SplitMix, case_stream, task_stream};

#[test]
fn the_reference_vector_from_state_zero() {
    let mut r = SplitMix::new(0);
    let got: Vec<u64> = (0..5).map(|_| r.next_u64()).collect();
    assert_eq!(
        got,
        [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f,
            0xf88b_b8a8_724c_81ec,
            0x1b39_896a_51a8_749b,
        ]
    );
}

#[test]
fn derived_draws_stay_in_range() {
    let mut r = SplitMix::new(7);
    let mut seen = [false; 6];
    for _ in 0..200 {
        let x = r.below(6);
        seen[x as usize] = true;
        let f = r.float();
        assert!((0.0..1.0).contains(&f));
    }
    assert!(seen.iter().all(|&s| s));
    assert_eq!(SplitMix::new(1).below(1), 0);
    assert_eq!(r.int(-3, -3), -3);
}

#[test]
fn streams_agree_with_the_js_runtime() {
    assert_eq!(case_stream(0, 3, 1).next_u64(), JS_CASE_0_3_1);
    assert_eq!(case_stream(-5, 0, 0).next_u64(), JS_CASE_M5_0_0);
    assert_eq!(task_stream(42).next_u64(), JS_TASK_42);
}

// Printed by: node -e 'import("./runtime/js/sakura_runtime.js").then(r => console.log(
//   r.caseStream(0, 3, 1).next(), r.caseStream(-5, 0, 0).next(), r.taskStream(42).next()))'
const JS_CASE_0_3_1: u64 = 0x3d20_0756_d6ea_1d7b;
const JS_CASE_M5_0_0: u64 = 0xa6ce_4cb4_407b_f559;
const JS_TASK_42: u64 = 0x5ddd_28c8_0ae9_bdb1;
