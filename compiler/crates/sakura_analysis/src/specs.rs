//! Interfaces and frozen specs: what a patch changes in the public surface (`check_patch`), and
//! whether it touches a frozen symbol's signature, effect row, error union, contracts, test body
//! or the manifest's permissions (design §7.3, `V8020`).

use std::collections::BTreeMap;

use sakura_diagnostics::{Diagnostic, Phase, Span, codes};
use serde::Serialize;

use crate::{Analyzed, Symbol};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InterfaceEntry {
    pub id: String,
    pub kind: &'static str,
    pub signature: String,
    pub contracts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InterfaceChange {
    pub id: String,
    pub before: InterfaceEntry,
    pub after: InterfaceEntry,
    pub effects_changed: bool,
    pub contracts_changed: bool,
}

/// How the public interface of the user package differs between two snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InterfaceDelta {
    pub added: Vec<InterfaceEntry>,
    pub removed: Vec<InterfaceEntry>,
    pub changed: Vec<InterfaceChange>,
    /// `[before, after]` rendering of the manifest's `[permissions]` when it changed.
    pub permissions: Option<[String; 2]>,
}

impl InterfaceDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.permissions.is_none()
    }
}

fn entry(s: &Symbol) -> InterfaceEntry {
    InterfaceEntry {
        id: s.id.clone(),
        kind: s.kind,
        signature: s.signature.clone(),
        contracts: s.contracts.clone(),
    }
}

fn user_symbols(a: &Analyzed) -> impl Iterator<Item = &Symbol> {
    a.symbols().iter().filter(|s| s.package != "std")
}

impl Analyzed {
    /// The public interface of the user package: `pub` items with signatures and contracts.
    pub fn interface(&self) -> BTreeMap<String, InterfaceEntry> {
        user_symbols(self)
            .filter(|s| s.is_pub && !matches!(s.kind, "test" | "property"))
            .map(|s| (s.id.clone(), entry(s)))
            .collect()
    }

    /// What changed in the public interface from `base` to `self`.
    pub fn interface_delta(&self, base: &Analyzed) -> InterfaceDelta {
        let (old, new) = (base.interface(), self.interface());
        let mut delta = InterfaceDelta::default();
        for (id, before) in &old {
            match new.get(id) {
                None => delta.removed.push(before.clone()),
                Some(after) if after != before => {
                    let effects = |a: &Analyzed| {
                        a.symbols()
                            .iter()
                            .find(|s| &s.id == id)
                            .and_then(|s| s.effects.clone())
                    };
                    delta.changed.push(InterfaceChange {
                        id: id.clone(),
                        effects_changed: effects(base) != effects(self),
                        contracts_changed: before.contracts != after.contracts,
                        before: before.clone(),
                        after: after.clone(),
                    });
                }
                Some(_) => {}
            }
        }
        delta.added = new
            .iter()
            .filter(|(id, _)| !old.contains_key(*id))
            .map(|(_, e)| e.clone())
            .collect();
        let (p0, p1) = (base.permissions_text(), self.permissions_text());
        if p0 != p1 {
            delta.permissions = Some([p0, p1]);
        }
        delta
    }

    /// The manifest's `[permissions]` table in a canonical rendering ("" without a manifest).
    fn permissions_text(&self) -> String {
        self.manifest()
            .and_then(Result::ok)
            .map(|m| m.permissions.render())
            .unwrap_or_default()
    }

    /// `V8020` for every way `self` changes a frozen spec of `base` (design §7.3): a frozen
    /// symbol's signature (parameters, result, effect row, error union), any of its
    /// `requires`/`ensures`/`where` clauses, a frozen test or property body, or, with
    /// [`Frozen::All`] or the name `permissions`, the manifest's `[permissions]`.
    pub fn frozen_spec_violations(&self, base: &Analyzed, frozen: &Frozen) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let fallback = self.snapshot.user_files().next();
        let at = |s: Option<&Symbol>| match s {
            Some(s) => Some(Span::primary(s.file, s.name_range[0], s.name_range[1])),
            None => fallback.map(|f| Span::primary(f, 0, 0)),
        };
        let mut report = |span: Option<Span>, symbol: &str, message: String| {
            let mut d =
                Diagnostic::error(codes::V8020, Phase::Package, message).with_symbol(symbol);
            if let Some(span) = span {
                d = d.with_span(span);
            }
            out.push(d);
        };
        for old in user_symbols(base).filter(|s| frozen.covers(s)) {
            let new = user_symbols(self).find(|s| s.id == old.id);
            let Some(new) = new else {
                report(
                    at(None),
                    &old.id,
                    format!("frozen {} `{}` was removed", old.kind, old.name),
                );
                continue;
            };
            if new.signature != old.signature {
                report(
                    at(Some(new)),
                    &old.id,
                    format!(
                        "the signature of frozen `{}` changed from `{}` to `{}`",
                        old.name, old.signature, new.signature
                    ),
                );
            }
            let removed: Vec<&String> = old
                .contracts
                .iter()
                .filter(|c| !new.contracts.contains(c))
                .collect();
            let added: Vec<&String> = new
                .contracts
                .iter()
                .filter(|c| !old.contracts.contains(c))
                .collect();
            // A named clause whose condition changed is one change, not a removal and an addition.
            for clause in &removed {
                let message = match counterpart(clause, &added) {
                    Some(now) => format!(
                        "frozen clause `{clause}` of `{}` was changed to `{now}`",
                        old.name
                    ),
                    None => format!("frozen clause `{clause}` of `{}` was removed", old.name),
                };
                report(at(Some(new)), &old.id, message);
            }
            for clause in added.iter().filter(|c| counterpart(c, &removed).is_none()) {
                report(
                    at(Some(new)),
                    &old.id,
                    format!("clause `{clause}` was added to frozen `{}`", old.name),
                );
            }
            if new.body != old.body {
                report(
                    at(Some(new)),
                    &old.id,
                    format!("the body of frozen {} `{}` changed", old.kind, old.name),
                );
            }
        }
        if frozen.covers_permissions() {
            let (before, after) = (base.permissions_text(), self.permissions_text());
            if before != after {
                let span = self
                    .snapshot
                    .file_ids()
                    .find(|&f| self.snapshot.file(f).uri.ends_with("/sakura.toml"))
                    .map(|f| Span::primary(f, 0, 0))
                    .or(at(None));
                report(
                    span,
                    "permissions",
                    format!("the frozen `[permissions]` changed from {before:?} to {after:?}"),
                );
            }
        }
        out
    }
}

/// The clause in `others` with the same kind and name as `clause`.
fn counterpart<'a>(clause: &str, others: &[&'a String]) -> Option<&'a String> {
    let key = clause_key(clause)?;
    others.iter().find(|o| clause_key(o) == Some(key)).copied()
}

/// `requires ordered` for `requires ordered: lo <= hi`; `None` for an unnamed clause.
fn clause_key(clause: &str) -> Option<&str> {
    let (key, _) = clause.split_once(':')?;
    let mut words = key.split(' ');
    let named = matches!(words.next(), Some("requires" | "ensures" | "where"))
        && words
            .next()
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_'))
        && words.next().is_none();
    named.then_some(key)
}

/// Which specs `--frozen-specs` / `apply_patch` protect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frozen {
    All,
    /// Symbol names (`clamp`, `Console.print`) or ids (`main::app::clamp`), and `permissions`.
    Only(Vec<String>),
}

impl Frozen {
    /// `all`, or a comma-separated list of names and ids.
    pub fn parse(text: &str) -> Frozen {
        if text.trim() == "all" {
            return Frozen::All;
        }
        Frozen::Only(
            text.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }

    fn covers(&self, s: &Symbol) -> bool {
        match self {
            Frozen::All => true,
            Frozen::Only(names) => names.iter().any(|n| *n == s.name || *n == s.id),
        }
    }

    fn covers_permissions(&self) -> bool {
        match self {
            Frozen::All => true,
            Frozen::Only(names) => names.iter().any(|n| n == "permissions"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Analysis, Snapshot};

    fn analyze(text: &str) -> std::sync::Arc<Analyzed> {
        Analysis::analyze(&Snapshot::with_std([(
            "file:///p/src/app.sakura".into(),
            text.into(),
        )]))
    }

    const BASE: &str = "pub fn clamp(x: Int, lo: Int, hi: Int) -> Int\n  requires ordered: lo <= hi\n{\n  if x < lo { lo } else if x > hi { hi } else { x }\n}\n\nfn helper() -> Int { 1 }\n\ntest \"clamps\" { expect_eq(clamp(5, 0, 3), 3) }\n";

    #[test]
    fn body_edits_keep_the_interface_and_frozen_specs() {
        let base = analyze(BASE);
        let edited = analyze(&BASE.replace("{ x }\n}", "{ x + 0 }\n}"));
        assert!(edited.interface_delta(&base).is_empty());
        assert!(
            edited
                .frozen_spec_violations(&base, &Frozen::All)
                .is_empty()
        );
    }

    #[test]
    fn weakening_a_contract_or_test_is_v8020() {
        let base = analyze(BASE);
        let weakened = analyze(
            &BASE
                .replace("  requires ordered: lo <= hi\n", "")
                .replace("clamp(5, 0, 3), 3", "clamp(5, 0, 3), 5"),
        );
        let delta = weakened.interface_delta(&base);
        assert_eq!(delta.changed.len(), 1);
        assert!(delta.changed[0].contracts_changed && !delta.changed[0].effects_changed);
        let diags = weakened.frozen_spec_violations(&base, &Frozen::All);
        let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(diags.len(), 2, "{messages:?}");
        assert!(messages[0].contains("`requires ordered: lo <= hi` of `clamp`"));
        assert!(messages[1].contains("body of frozen test `clamps`"));
        assert!(diags.iter().all(|d| d.code == codes::V8020));
        assert_eq!(diags[0].symbol_id.as_deref(), Some("main::app::clamp"));
        // Only the named symbols are frozen.
        let only = Frozen::parse("helper");
        assert!(weakened.frozen_spec_violations(&base, &only).is_empty());
    }

    #[test]
    fn effect_row_changes_are_interface_changes() {
        let base = analyze("pub fn f() -> Int { 1 }\n");
        let new = analyze("import std/console.{Console}\npub fn f() -> Int ! {Console} { 1 }\n");
        let delta = new.interface_delta(&base);
        assert!(delta.changed[0].effects_changed, "{delta:?}");
        let diags = new.frozen_spec_violations(&base, &Frozen::parse("f"));
        assert!(diags[0].message.contains("signature of frozen `f`"));
    }
}
