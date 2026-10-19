//! Ownership facts in the LIR (design §17.176 (2)): `sakura_core::opt::owned` run over the
//! snapshot read back as Core, not serialised into it (§17.149 (2)), so the LIR's facts and
//! the JS backend's are one analysis of the committed form; `tests/direct.rs` holds [`dump`]
//! to the JS backend's `--dump owned` on every corpus program.

use crate::snapshot;

/// `--dump owned` from the snapshot: each consuming site of every module outside std.
pub fn dump(core: &snapshot::Module) -> String {
    sakura_core::opt::owned::display(&snapshot::convert::back(core))
}
