//! Desugaring and name resolution from syntax to HIR (design §13.1, §14.2).
//!
//! The input is an in-memory snapshot of packages and their source files; this crate never
//! touches the filesystem. The output is a [`Program`]: every module lowered into arenas of
//! [`Def`]s and [`Body`]s with all names resolved to [`DefId`]s, [`LocalId`]s or builtins.
//!
//! # Invariants
//!
//! Consumers (`vow_types`, `vow_core`) may rely on these for every [`Program`]:
//!
//! - **Total.** Lowering never panics and always produces a program, whatever the input.
//!   Syntax that is missing or malformed becomes [`Expr::Missing`], [`Pat::Missing`] or
//!   [`TypeRefKind::Missing`]; names that do not resolve become [`Res::Err`] /
//!   [`TypeRes::Err`]. Each such node has a diagnostic in [`Program::diagnostics`] (or the
//!   parse errors), so later phases treat them as already-reported and must not cascade.
//! - **Resolved.** Every [`Res::Local`] refers to a local of the same body that is in scope at
//!   that point; every [`Res::Def`] / [`TypeRes::Def`] refers to a definition that exists and
//!   is visible from the referencing module. [`TypeRes::Param`] indexes the generics of the
//!   enclosing definition (the parent type's generics for variants, the effect's generics
//!   for ops).
//! - **Namespaces.** `Res::Def` in value position names a `Fn`, `Const`, `Variant`, `Error`
//!   (tag constructor), or `Op`. `TypeRes::Def` in type position names a `Type` or `Error`;
//!   in an effect row or handler, an `Effect`. Kind mismatches are reported and become `Err`.
//! - **Spans.** Every [`ExprId`] and [`PatId`] has a range in its body's [`SourceMap`]; every
//!   def, local, type and row carries its own range. All ranges are in the owning module's
//!   [`Module::file`].
//! - **Desugared.** `while c { b }` is `Loop(If(c, b, Break))`; `x?` is [`Expr::Raise`];
//!   `mod.f(a)`, `Type.f(a)`, `Eff.op(a)` and `Type.Variant(a)` are `Call(Path(Def), [a])`;
//!   `-<literal>` in patterns is folded. Method calls on values stay [`Expr::MethodCall`]
//!   because they need the receiver's type (§4.9 rules 1 and 5; see
//!   [`Program::resolve_home`]).
//! - **Deterministic.** Arena order is source order; the same snapshot always produces the
//!   same ids, diagnostics and dumps.

mod hir;

pub use hir::*;
