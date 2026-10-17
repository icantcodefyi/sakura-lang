//! Core IR (typed ANF), lowering from typed HIR, match compilation, validation.
//!
//! The IR is defined in [`ir`] (design §13.2); [`validate`] checks its invariants after every
//! pass; [`build`] has construction helpers; the `Display` impl prints a stable text form.

pub mod build;
pub mod builtins;
pub mod ir;
mod pretty;
mod validate;

pub use ir::*;
pub use validate::{ValidationError, validate};
