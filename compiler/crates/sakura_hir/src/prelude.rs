//! The prelude (design §9): names in scope in every module without an import.
//!
//! Builtin types come from [`Builtin`](crate::Builtin). Everything else is declared in Sakura Lang by
//! the `std` package (`compiler/std/*.sakura`) and plugged in here by name: each entry says which
//! std module declares it. Entries whose module is not in the snapshot simply do not resolve,
//! and using them reports `V1002` naming the missing module. To grow the prelude, declare the
//! item `pub` in its std module and add a row below.

/// `(name, std module path)`. A lowercase name whose module path equals the name binds the
/// module itself (the template tags `sql`, `html`, `url`, `shell`).
pub const PRELUDE: &[(&str, &str)] = &[
    ("Option", "option"),
    ("Some", "option"),
    ("None", "option"),
    ("Result", "result"),
    ("Ok", "result"),
    ("Err", "result"),
    ("Console", "console"),
    ("Clock", "clock"),
    ("Random", "random"),
    ("Env", "env"),
    ("Log", "log"),
    ("Task", "task"),
    ("show", "core"),
    // Std types the prelude effects mention (std/INDEX.md).
    ("Duration", "duration"),
    ("Instant", "instant"),
    ("Uuid", "uuid"),
    ("Json", "json"),
    ("Level", "log"),
    // std/test: used unqualified by every `test` block in design §14.3 and §15.
    ("Test", "test"),
    ("expect", "test"),
    ("expect_eq", "test"),
    ("expect_fail", "test"),
    ("expect_fails_with", "test"),
    ("expect_defect", "test"),
    ("expect_snapshot", "test"),
    ("expect_output", "test"),
    // Template tags resolve to modules (§4.11).
    ("sql", "sql"),
    ("html", "html"),
    ("url", "url"),
    ("shell", "shell"),
];

pub fn lookup(name: &str) -> Option<&'static str> {
    PRELUDE.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
}
