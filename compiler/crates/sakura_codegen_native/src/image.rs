//! A module's static block for the native runtime (ABI §1b, §9), built by `sakura_lir::image`
//! as the WASM backend's is (design §17.229 (2)). Code indices follow the native dispatch
//! table: the module's functions, then [`CODES`] in its order (`sakura_lir::image::code_order`).

use std::collections::HashMap;

use sakura_lir::image::{self as shared, Clauses, Statics};
use sakura_lir::lir::Module;
use sakura_rt::native::{CODES, Image};

pub use sakura_lir::image::check_message;

/// The result top `rt_vt_result` checks a hosted builtin's answer against
/// (`sakura_lir::image::HOSTED`).
pub fn hosted_top(name: &str) -> u32 {
    shared::HOSTED
        .iter()
        .find(|(h, _)| *h == name)
        .map_or(shared::top::RESULT, |(_, t)| *t)
}

/// The defect message of a builtin the native runtime does not implement.
pub fn unsupported_builtin(name: &str) -> String {
    format!("builtin {name} is not supported by the native host yet")
}

pub const NOT_RESUMABLE: &str = "a frame resumed at a block that is not resumable";
pub const NO_ARM: &str = "no switch arm applied";
pub const HOST_FAILED: &str = "a host builtin failed";
pub const OVERFLOW: (u16, &str) = (1, "integer overflow");
pub const ZERO: (u16, &str) = (2, "division by zero");

pub fn hole_message(id: u32) -> String {
    format!("typed hole ?{id} was evaluated")
}

pub fn unsupported_message(what: &str) -> String {
    format!("not supported by the WASM backend yet: {what}")
}

/// A module's static block and the facts code generation reads from it.
pub struct Tables {
    pub statics: Statics,
    tables: shared::Tables,
    natives: u32,
    schemas: u32,
    /// Host-built root handlers by builtin.
    pub host_handlers: HashMap<String, Clauses>,
    /// The test runner's `std::test::Test` clauses, when the module has the effect.
    pub test: Option<Clauses>,
    /// The code of each `cps` builtin the runtime implements (`rt_k_<name>`).
    pub cps: HashMap<String, u32>,
}

impl Tables {
    pub fn new(m: &Module) -> Tables {
        let n = m.funcs.len() as u32;
        let names: Vec<&str> = CODES.iter().map(|(name, _)| *name).collect();
        let mut statics = Statics::default();
        let tables = shared::tables(m, &mut statics, &names);
        let handlers = shared::handlers(m, &mut statics, n, &names);
        let schemas = shared::schemas(m, &mut statics);
        Tables {
            statics,
            tables,
            natives: handlers.natives,
            schemas,
            host_handlers: handlers.host,
            test: handlers.test,
            cps: shared::cps(n, &names),
        }
    }

    /// The finished block, padded to 8 bytes.
    pub fn image(&mut self) -> Image {
        self.statics.finish();
        Image {
            bytes: self.statics.bytes.clone(),
            layout: self.tables.layout,
            codes: self.tables.codes,
            effects: self.tables.effects,
            ops: self.tables.ops,
            natives: self.natives,
            schemas: self.schemas,
        }
    }
}
