# Modules and imports

**Draft (normative at v1.0.0).**

Informative scope: this chapter covers modules and packages as the resolver sees them, imports,
visibility, name lookup through home modules, the entry point, and the unused-import warning
`W1003`.
Its sources are [design §4.9 and §9](../design.md) with rulings §17.127, §17.130, §17.131 and
§17.294 RQ-5.
The import syntax is [grammar G16](grammar.md); dependencies on other packages are refused
today (`V8005`) and are out of scope.
[Draft conventions](README.md) identify normative material; rules are labelled **MOnn**.

Several citations are snapshot tests in `compiler/crates/sakura_hir/tests/lower.rs`, which
build a small program of several modules in memory; the link label names the snapshot.

## Modules and packages

**MO01** Each source file is one module whose path is the package's name followed by the file's path under `src/` without `.sak`, and the package's name comes from `sakura.toml`. [the_package_is_named_by_its_manifest](../../compiler/crates/sakura_cli/tests/cli.rs), [imports_across_modules](../../compiler/crates/sakura_hir/tests/snapshots/lower__imports_across_modules.snap).

**MO02** A file compiled without a manifest belongs to the package `main`, and `std` and `main` are refused as manifest names (`V8002`). [Modules `main/a` and `main/b`](../../compiler/tests/ui/V1030_import_cycle/a.sak), [reserved_package_names_are_v8002](../../compiler/crates/sakura_cli/tests/cli.rs).

## Imports

**MO03** `import p/m` binds the module name `m`, `import p/m.{a, B}` binds the listed items directly, and `as n` binds the module as `n` instead. [Imports](../../compiler/tests/semantics/imports.sak), [imports_across_modules](../../compiler/crates/sakura_hir/tests/snapshots/lower__imports_across_modules.snap), [the_package_is_named_by_its_manifest](../../compiler/crates/sakura_cli/tests/cli.rs).

**MO04** Nothing outside the prelude is in scope without an import, so an effect or type from another module used unqualified and unimported is `V1003` in type position and `V1002` in value position, with a fix that adds the import when std has the name. [diag_explicit_imports](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_explicit_imports.snap), [V1002](../../compiler/tests/ui/V1002_unresolved_name.sak), [V1003](../../compiler/tests/ui/V1003_unresolved_type.sak).

**MO05** A module whose last segment is a keyword, such as `std/test`, binds no name when imported plainly, which is `V1005`, and is imported with `.{…}` or `as`. [diag_explicit_imports](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_explicit_imports.snap).

**MO06** Importing a module that does not exist is `V1005`, naming an item the module does not export is `V1002`, and importing in a cycle is `V1030`. [V1005](../../compiler/tests/ui/V1005_unresolved_module.sak), [diag_private_and_missing_items](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_private_and_missing_items.snap), [V1030](../../compiler/tests/ui/V1030_import_cycle/a.sak).

Informative: an `import` is an edge of the module graph whether or not the module uses what it
binds (design §9, §17.307 (1)), so an unused import that closes a cycle is `V1030` beside
`W1003`, and removing it, `W1003`'s fix, removes the cycle. MO06 gains the words when the
`V1030_import_cycle_unused` fixture lands.

**MO18** The prelude is in scope in every module without an import: the types `Unit Bool Int Float String Bytes Never List Map Set Range Handler Secret Untrusted Option Result Duration Instant Uuid Json Level Violation`, the constructors `Some None Ok Err`, the effects `Fail Console Clock Random Env Log Task Test`, the functions `show expect expect_eq expect_fail expect_fails_with expect_defect expect_output expect_snapshot` and the template tags `sql html url`; any other std name needs its import (MO04). [Every prelude name, no import](../../compiler/tests/semantics/prelude_names_need_no_import.sak), [a non-prelude std name](../../compiler/tests/ui/V1002_non_prelude_std_name.sak).

Informative: design §9 has no wildcard imports, no relative imports and no re-exports; the
grammar has no syntax for them. The prelude also reserves the tag `shell` for a `std/shell`
that does not exist yet, so using it is `V1002` today, and `Trust` resolves but may only appear
as the type of `@trust("…")` (`V6001`), so the fixture names neither.

## Visibility

**MO07** Items are private to their module unless declared `pub`, and naming a private item from another module is `V1006`. [V1006](../../compiler/tests/ui/V1006_private_item.sak), [diag_private_and_missing_items](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_private_and_missing_items.snap).

**MO08** A `pub opaque type` exports its name only, so its constructors and fields are `V1006` outside the declaring module, and it is `Schema` only inside it, so decoding it or generating it elsewhere is `V2033`. [Constructor](../../compiler/tests/ui/V1006_opaque_constructor.sak), [field](../../compiler/tests/ui/V1006_opaque_field.sak), [decode](../../compiler/tests/ui/V2033_decode_opaque_html.sak), [property](../../compiler/tests/ui/V2033_property_opaque.sak), [opaque_types_are_schema_only_in_their_module](../../compiler/crates/sakura_types/tests/acceptance.rs).

**MO09** Two items of one name at the same level of a module are `V1001`, and a local binding that shadows an imported name is allowed with `W1002`. [V1001](../../compiler/tests/ui/V1001_duplicate_definition.sak), [W1002](../../compiler/tests/ui/W1002_shadowed_import.sak), [diag_shadowed_import](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_shadowed_import.snap).

## Name lookup

**MO10** `T.f` names `f` in the home module of the type or effect `T`, which for a compiler-known type is `std/<lowercase name>`, and `x.f(args)` on a value of type `T` means `T.f(x, args)`. [home_module_calls](../../compiler/crates/sakura_hir/tests/snapshots/lower__home_module_calls.snap), [method chain](../../compiler/tests/semantics/newline_rule.sak), [diag_private_and_missing_items](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_private_and_missing_items.snap).

**MO11** A type name used as a value or pattern, or as the last segment of a path, is a variant or error tag, so a module may declare a variant `String` and still call `String.len`. [variant_named_like_builtin](../../compiler/crates/sakura_hir/tests/snapshots/lower__variant_named_like_builtin.snap).

## Unused imports

**MO12** Each imported name that nothing in the module uses gets one `W1003`, where a use in a value, a type, a pattern or a `@contract_test` attribute counts and a module bound by `as` is a name like any other. [W1003](../../compiler/tests/ui/W1003_unused_import.sak), [diagnostics](../../compiler/tests/ui/W1003_unused_import.jsonl), [diag_explicit_imports](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_explicit_imports.snap).

**MO13** `W1003`'s fix is machine-applicable and removes the unused name with its separating comma, or the whole `import` when nothing it brings is used. [Fixed output](../../compiler/tests/ui/W1003_unused_import.fixed.sak), [records](../../compiler/tests/ui/W1003_unused_import.jsonl).

**MO14** An import that a local binding shadows everywhere it would be used is unused, so it gets `W1003` beside `W1002`. [W1002 with W1003](../../compiler/tests/ui/W1002_shadowed_import.sak), [diag_shadowed_import](../../compiler/crates/sakura_hir/tests/snapshots/lower__diag_shadowed_import.snap).

**MO17** std's own modules are not checked for `W1003`. [std_modules_are_exempt_from_w1003](../../compiler/crates/sakura_hir/tests/lower.rs).

Informative: the agent-DX harness exemption that §17.297 refused is not a compiler rule.

## Entry point

**MO15** `sakura run` needs a `pub fn main`, its absence being `V8001`, and `main`'s row may hold only std effects (`V6010`), for each of which the host installs a root handler. [V8001](../../compiler/tests/ui/V8001_missing_entry_point.sak), [V6010](../../compiler/tests/ui/V6010_non_std_effect_in_main.sak), [host_root_handlers](../../compiler/tests/semantics/host_root_handlers.sak).

**MO16** A `main` that declares a std effect its manifest's `[permissions]` grant nothing for, or that its target cannot provide, exits 75 at startup before `main` runs. [run_exits_75_when_permissions_do_not_cover_main](../../compiler/crates/sakura_cli/tests/cli.rs), [run_exits_75_for_websocket_without_net](../../compiler/crates/sakura_cli/tests/cli.rs).

Informative: what each root handler may reach under `[permissions]` is design §6.2
([root_handlers_apply_manifest_permissions](../../compiler/crates/sakura_codegen_js/tests/m3.rs)).
