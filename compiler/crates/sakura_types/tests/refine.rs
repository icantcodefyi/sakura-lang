//! Refined types in the checker (design §4.3, §6.4, §7.2, §17.71, §17.72): literals into a
//! refined type (`V5002`), `Untrusted.validate` (`V5003`) and linear contracts (`W5005`).

mod common;

use common::check;
use sakura_hir::{DefKind, Expr};
use sakura_types::TypedBody;

const EMAIL: &str = "type Email = String where String.len(it) <= 254\ntype Percent = Int where it >= 0 && it <= 100\n";

fn codes(src: &str) -> Vec<&'static str> {
    let c = check(&format!("{EMAIL}{src}"));
    assert!(c.std_diags.is_empty(), "std: {:?}", c.std_diags);
    eprintln!("{}", c.render());
    c.codes()
}

/// `(converted expression's source, conversion target name)` of a body, in source order.
fn conversions(c: &common::Checked, name: &str) -> Vec<(String, String)> {
    let (_, body, typed): (_, _, &TypedBody) = c.body_of(name);
    let mut out: Vec<_> = typed
        .conversions
        .iter()
        .map(|(e, (def, _))| {
            let r = body.source_map.expr(e);
            let text = c.src[usize::from(r.start())..usize::from(r.end())].to_string();
            let d = c.program.def(*def);
            (
                r.start(),
                text,
                d.name.clone(),
                matches!(d.kind, DefKind::Fn(_)),
            )
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, t, n, _)| (t, n)).collect()
}

// ---- V5002: literals into refined types ----

#[test]
fn a_literal_into_a_refined_type_is_v5002_and_a_conversion() {
    let src = format!(
        "{EMAIL}let support: Email = \"help@sakura.dev\"\nfn half() -> Percent {{ 50 }}\nfn cold() -> Int {{ let p: Percent = -0\n  p }}\n"
    );
    let c = check(&src);
    assert_eq!(c.codes(), ["V5002", "V5002", "V5002"], "{}", c.render());
    assert!(
        c.diags
            .iter()
            .all(|d| d.severity == sakura_diagnostics::Severity::Warning),
        "{}",
        c.render()
    );
    assert!(
        c.render().contains(
            "this literal is checked against the refinement of `Email` when the program starts"
        ),
        "{}",
        c.render()
    );
    assert_eq!(
        conversions(&c, "support"),
        [("\"help@sakura.dev\"".into(), "from".into())]
    );
    assert_eq!(conversions(&c, "half"), [("50".into(), "from".into())]);
    assert_eq!(conversions(&c, "cold"), [("-0".into(), "from".into())]);
}

#[test]
fn literals_reach_refined_parameters_and_fields() {
    let src = "type Grade = { score: Percent }\nfn show(p: Percent) -> Int { p }\nfn main() -> Int {\n  let g = Grade { score: 90 }\n  show(10) + g.score\n}\n";
    assert_eq!(codes(src), ["V5002", "V5002"]);
}

#[test]
fn only_literals_of_the_base_type_coerce() {
    // A non-literal, or a literal of another type, is still a type mismatch (§17.71).
    let src = "fn a(n: Int) -> Percent { n }\nfn b() -> Percent { \"50\" }\nfn c() -> Option<Percent> { Some(5) }\n";
    assert_eq!(codes(src), ["V2001", "V2001", "V2001"]);
}

// ---- Untrusted.validate and V5003 ----

#[test]
fn validate_into_an_annotated_refined_type_is_a_conversion() {
    let src = format!(
        "{EMAIL}import std/core.{{Violation}}\nfn parse(u: Untrusted<String>) -> Email ! {{Fail<Violation>}} {{\n  let e: Email = Untrusted.validate(u)?\n  e\n}}\nfn method(u: Untrusted<Int>) -> Result<Percent, Violation> {{ u.validate() }}\n"
    );
    let c = check(&src);
    assert!(c.diags.is_empty(), "{}", c.render());
    let conv = conversions(&c, "parse");
    assert_eq!(conv, [("Untrusted.validate(u)".into(), "validate".into())]);
    // The target is `Email`'s generated `validate`, not `Untrusted.validate`.
    let (_, body, typed) = c.body_of("parse");
    let (call, (def, _)) = typed.conversions.iter().next().unwrap();
    assert!(matches!(body.exprs[call], Expr::Call { .. }));
    assert_eq!(
        sakura_hir::RefinementIntro::of(c.program.def(*def)),
        Some(sakura_hir::RefinementIntro::Validate)
    );
    let email = c.program.lookup(c.main_module(), "Email").unwrap();
    assert_eq!(def.module, email.module);
    assert_eq!(
        conversions(&c, "method"),
        [("u.validate()".into(), "validate".into())]
    );
}

#[test]
fn validate_into_a_plain_type_is_v5003() {
    let src = "import std/core.{Violation}\nfn plain(u: Untrusted<String>) -> String ! {Fail<Violation>} {\n  let s: String = Untrusted.validate(u)?\n  s\n}\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["V5003"], "{}", c.render());
    assert!(
        c.render()
            .contains("validates into a refined type, but `String` is not one"),
        "{}",
        c.render()
    );
    assert!(
        c.render()
            .contains("Annotate the binding with a refined type"),
        "{}",
        c.render()
    );
}

#[test]
fn validate_without_a_target_or_with_the_wrong_base_is_v5003() {
    let src = "import std/core.{Violation}\nfn unknown(u: Untrusted<String>) -> Bool {\n  match Untrusted.validate(u) { Ok(_) => true, Err(_) => false }\n}\nfn wrong(u: Untrusted<String>) -> Result<Percent, Violation> { Untrusted.validate(u) }\nfn value() -> Int {\n  let f = Untrusted.validate\n  1\n}\n";
    let c = check(&format!("{EMAIL}{src}"));
    let r = c.render();
    assert_eq!(c.codes(), ["V5003", "V5003", "V5003"], "{r}");
    assert!(r.contains("nothing here names it"), "{r}");
    assert!(
        r.contains("cannot validate an `Untrusted<String>` into `Percent`: `Percent` does not refine `String`"),
        "{r}"
    );
    assert!(r.contains("must be called directly"), "{r}");
}

// ---- W5005: linear contracts ----

#[test]
fn a_clause_scanning_a_parameter_is_w5005_once() {
    let src = "fn count(xs: List<Int>, s: String) -> Int\n  requires all_positive: List.all(xs, fn(x) { x > 0 }) && List.any(xs, fn(x) { x > 1 })\n  requires String.contains(s, \"@\")\n  ensures result >= 0\n{\n  List.len(xs)\n}\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["W5005", "W5005"], "{}", c.render());
    assert!(
        c.render().contains(
            "this contract calls `String.contains` on `s`, so every check scans all of it"
        ),
        "{}",
        c.render()
    );
}

#[test]
fn refinements_and_property_requires_are_w5005_but_bounded_clauses_are_not() {
    let src = "type Tags = List<String> where List.contains(it, \"x\")\ntype Ok = List<Int> where List.len(it) < 10\nfn first(xs: List<Int>) -> Int\n  requires List.len(xs) > 0\n{\n  0\n}\nproperty \"members\"(xs: List<Int>, x: Int) requires List.contains(xs, x) {\n  expect(List.len(xs) > 0)\n}\n";
    assert_eq!(codes(src), ["W5005", "W5005"]);
}

#[test]
fn std_contracts_are_exempt() {
    let c = check("fn f() -> Int { 1 }\n");
    assert!(c.std_diags.is_empty(), "{:?}", c.std_diags);
}
