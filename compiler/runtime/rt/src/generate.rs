//! `sakura-gen-v1` (ABI §11, design §17.96): the deterministic generation protocol shared with
//! the JS runtime (`SplitMix`, `caseStream`, `taskStream` in `runtime/js/sakura_runtime.js`), so
//! `--seed N` gives the same property inputs and the same `Task.deterministic` fiber order on
//! every backend. Raw bits are SplitMix64; `below(n)` is unbiased rejection sampling on the top
//! `ceil(log2 n)` bits; `float()` is the upper 53 bits over 2^53; `bool()` is the top bit.

/// The protocol id printed with every property failure and `sakura test --json` record.
pub const PROTOCOL: &str = "sakura-gen-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitMix {
    pub state: u64,
}

impl SplitMix {
    pub fn new(state: u64) -> SplitMix {
        SplitMix { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` for `1 <= n <= 2^63`.
    pub fn below(&mut self, n: u64) -> u64 {
        let k = 64 - (n - 1).leading_zeros();
        loop {
            let x = self.next_u64().checked_shr(64 - k).unwrap_or(0);
            if x < n {
                return x;
            }
        }
    }

    pub fn float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn bool(&mut self) -> bool {
        self.next_u64() >> 63 == 1
    }

    /// `lo + below(hi - lo + 1)` for `lo <= hi`.
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        let span = hi.wrapping_sub(lo) as u64;
        lo.wrapping_add(self.below(span + 1) as i64)
    }
}

/// One SplitMix64 output from state `x`.
pub fn splitmix(x: u64) -> u64 {
    SplitMix::new(x).next_u64()
}

/// One step from `s ^ x` (domain separation).
pub fn mix(s: u64, x: u64) -> u64 {
    splitmix(s ^ x)
}

/// FNV-1a 64 of an ASCII label.
pub fn fnv64(label: &str) -> u64 {
    label.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The stream of property case `run`, parameter `param`, under `--seed seed`.
pub fn case_stream(seed: i64, run: u64, param: u64) -> SplitMix {
    let base = splitmix(seed as u64);
    SplitMix::new(mix(mix(base, fnv64("case") ^ run), param))
}

/// `Task.deterministic(seed)`'s stream.
pub fn task_stream(seed: i64) -> SplitMix {
    SplitMix::new(mix(splitmix(seed as u64), fnv64("task")))
}
