//! Refined types in the checker (design §4.3, §6.4, §7.2, §17.71, §17.72, §17.87, §17.90):
//! literals into a refined type decided at compile time (`V5004`), `Untrusted.validate`
//! (`V5003`) and linear contracts (`W5005`).

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

/// Every conversion recorded anywhere in the user module.
fn all_conversions(c: &common::Checked) -> usize {
    let m = c.main_module();
    c.typed
        .bodies
        .iter()
        .filter(|((module, _), _)| *module == m)
        .map(|(_, b)| b.conversions.iter().count())
        .sum()
}

/// Checks `src` (after the `Email`/`Percent` prelude): no diagnostic and no conversion.
fn decided(src: &str) {
    let c = check(&format!("{EMAIL}{src}"));
    assert!(c.std_diags.is_empty(), "std: {:?}", c.std_diags);
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(
        all_conversions(&c),
        0,
        "a decided literal needs no start-up check"
    );
}

/// Checks `src`: no diagnostic, and exactly the literal `lit` of `name` is converted.
fn undecided(src: &str, name: &str, lit: &str) {
    let c = check(&format!("{EMAIL}{src}"));
    assert!(c.diags.is_empty(), "{}", c.render());
    assert_eq!(conversions(&c, name), [(lit.into(), "from".into())]);
    assert_eq!(all_conversions(&c), 1);
}

// ---- §17.87: literals into refined types are decided at compile time ----

#[test]
fn literals_that_satisfy_the_predicate_need_no_conversion() {
    decided(
        "let support: Email = \"help@sakura.dev\"\nfn half() -> Percent { 50 }\nfn cold() -> Int { let p: Percent = -0\n  p }\n",
    );
}

#[test]
fn std_status_and_the_tour_email_are_decided() {
    decided(
        "import std/response.{Status}\ntype Address = String where String.len(it) <= 254 && String.contains(it, \"@\")\nlet missing: Status = 404\nlet owner: Address = \"ada@example.com\"\n",
    );
}

#[test]
fn a_user_total_predicate_in_the_fragment_is_decided() {
    decided(
        "@total\nfn even(n: Int) -> Bool {\n  if n < 0 { even(-n) } else {\n    let half = n / 2\n    half * 2 == n\n  }\n}\ntype Even = Int where even(it)\ntype Ratio = Float where it >= 0.0 && it <= 1.0\ntype Yes = Bool where it\ntype Word = String where it.len() > 0 && !String.starts_with(it, \" \") && !it.ends_with(\" \")\nlet e: Even = -4\nlet r: Ratio = 0.25\nlet y: Yes = true\nlet w: Word = \"sakura\"\n",
    );
}

#[test]
fn literals_reach_refined_parameters_and_fields() {
    let src = "type Grade = { score: Percent }\nfn show(p: Percent) -> Int { p }\nfn main() -> Int {\n  let g = Grade { score: 90 }\n  show(10) + g.score + show(101)\n}\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["V5004"], "{}", c.render());
    assert!(
        c.render().contains("`101` does not satisfy"),
        "{}",
        c.render()
    );
}

#[test]
fn a_literal_that_fails_the_predicate_is_v5004_with_the_clause() {
    let src = format!("{EMAIL}let p: Percent = 150\n");
    let c = check(&src);
    let r = c.render();
    assert_eq!(c.codes(), ["V5004"], "{r}");
    let d = &c.diags[0];
    assert_eq!(d.severity, sakura_diagnostics::Severity::Error);
    assert_eq!(d.phase, sakura_diagnostics::Phase::Contract);
    assert!(d.fixes.is_empty(), "no mechanical repair");
    assert!(
        r.contains("V5004 3:18 `150` does not satisfy the `where` clause of `Percent`"),
        "{r}"
    );
    assert!(
        r.contains("Definition 2:26 `it >= 0 && it <= 100` the refinement of `Percent`"),
        "{r}"
    );
}

#[test]
fn the_tour_email_rejects_a_string_without_an_at() {
    let src = "type Address = String where String.len(it) <= 254 && String.contains(it, \"@\")\nlet a: Address = \"x\"\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["V5004"], "{}", c.render());
    assert!(
        c.render().contains("`\"x\"` does not satisfy"),
        "{}",
        c.render()
    );
}

#[test]
fn a_defect_in_the_predicate_is_v5004() {
    // 64-bit overflow, division by zero and a failed `requires` defect on every backend.
    let src = "type Cube = Int where it * it * it > 0\n@total\nfn inverse(n: Int) -> Int { 100 / n }\ntype Inv = Int where inverse(it) > 0\nfn positive(n: Int) -> Bool\n  requires n > 0\n{\n  true\n}\ntype Pos = Int where positive(it)\nlet c: Cube = 3000000\nlet i: Inv = 0\nlet p: Pos = -1\n";
    let c = check(&format!("{EMAIL}{src}"));
    let r = c.render();
    assert_eq!(c.codes(), ["V5004", "V5004", "V5004"], "{r}");
    assert_eq!(r.matches(": evaluating it defects").count(), 3, "{r}");
}

#[test]
fn a_predicate_outside_the_fragment_keeps_the_start_up_check() {
    // `String.split` and `List.contains` are outside the scalar fragment (§17.87).
    undecided(
        "type Dotted = String where List.len(String.split(it, \".\")) > 1\nlet host: Dotted = \"sakura.dev\"\n",
        "host",
        "\"sakura.dev\"",
    );
}

#[test]
fn an_int_beyond_the_js_safe_range_is_undecidable() {
    // 10^16 fits 64 bits but not ±(2^53−1): JS and WASM disagree, so it is not decided.
    undecided(
        "type Wide = Int where it * it > 0\nlet w: Wide = 100000000\n",
        "w",
        "100000000",
    );
}

#[test]
fn a_predicate_over_the_step_budget_keeps_the_start_up_check() {
    let src = "@total\nfn spin(n: Int) -> Bool { if n == 0 { true } else { spin(n + 1) } }\ntype Spun = Int where spin(it)\nlet s: Spun = 1\n";
    undecided(src, "s", "1");
}

#[test]
fn short_circuit_decides_before_reaching_the_undecidable_part() {
    let src = "type Rel = String where it != \"\" && !String.starts_with(it, \"/\") && !List.contains(String.split(it, \"/\"), \"..\")\nlet bad: Rel = \"/etc\"\nlet good: Rel = \"a.txt\"\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["V5004"], "{}", c.render());
    assert_eq!(
        conversions(&c, "good"),
        [("\"a.txt\"".into(), "from".into())]
    );
}

#[test]
fn std_path_and_status_literals_are_decided() {
    // `std/path.is_relative` is in the scalar fragment (§17.87, WS4).
    decided(
        "import std/path.{Path}\nimport std/response.{Status}\nlet notes: Path = \"notes/today.txt\"\nlet dots: Path = \"a..b\"\nlet ok: Status = 200\n",
    );
    let c = check("import std/path.{Path}\nlet up: Path = \"a/../b\"\nlet abs: Path = \"/etc\"\n");
    assert_eq!(c.codes(), ["V5004", "V5004"], "{}", c.render());
}

/// The four string built-ins of the fragment agree with the JS runtime's `string_*` on
/// non-ASCII input: each expected value comes from `sakura_runtime.js`, and the literal is
/// accepted only if the evaluator computes the same.
#[test]
fn string_built_ins_match_the_js_runtime() {
    let cases = [
        ("héllo", "é"),
        ("日本語テキスト", "本語"),
        ("🌸 sakura", "🌸"),
        ("a\u{301}b", "\u{301}"),
        ("naïve café", "café"),
        ("ß", "s"),
        ("", ""),
        ("x🌸", "🌸"),
    ];
    let json: Vec<String> = cases
        .iter()
        .map(|(s, t)| format!("[{s:?},{t:?}]"))
        .collect();
    let script = format!(
        "const rt = await import('./sakura_runtime.js'); for (const [s, t] of [{}]) \
         console.log([rt.string_len(s), rt.string_contains(s, t), rt.string_starts_with(s, t), rt.string_ends_with(s, t)].join(' '));",
        json.join(",")
    );
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/js");
    let out = std::process::Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .current_dir(runtime)
        .output()
        .expect("node runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<Vec<String>> = stdout
        .lines()
        .map(|l| l.split(' ').map(str::to_owned).collect())
        .collect();
    assert_eq!(
        rows.len(),
        cases.len(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut src = String::new();
    for (i, ((s, t), row)) in cases.iter().zip(&rows).enumerate() {
        let [len, contains, starts, ends] = &row[..] else {
            panic!("{row:?}");
        };
        src.push_str(&format!(
            "type T{i} = String where String.len(it) == {len} && String.contains(it, {t:?}) == {contains} && String.starts_with(it, {t:?}) == {starts} && String.ends_with(it, {t:?}) == {ends}\nlet v{i}: T{i} = {s:?}\n"
        ));
    }
    decided(&src);
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
    // §17.90: `String.contains` is not a trigger; one warning for the `List` clause.
    let src = "fn count(xs: List<Int>, s: String) -> Int\n  requires all_positive: List.all(xs, fn(x) { x > 0 }) && List.any(xs, fn(x) { x > 1 })\n  requires String.contains(s, \"@\")\n  ensures result >= 0\n{\n  List.len(xs)\n}\n";
    let c = check(&format!("{EMAIL}{src}"));
    assert_eq!(c.codes(), ["W5005"], "{}", c.render());
    assert!(
        c.render()
            .contains("on `xs`, so every check scans all of it"),
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

// ---- §4.3: a generic refined type's base is taken at its arguments ----

const GENERIC: &str = "import std/core.{Violation}\ntype NonEmpty<T> = List<T> where List.len(it) > 0\ntype Box<T> = { item: T }\ntype Full<T> = Box<T> where true\ntype Small<T> = Int where it < 10\n";

#[test]
fn generic_refinements_coerce_to_their_base_at_the_arguments() {
    let src = "fn top(xs: NonEmpty<Int>) -> Int { List.fold(xs, 0, fn(best, s) { Int.max(best, s) }) }\nfn count(xs: NonEmpty<String>) -> Int { xs.len() }\nfn shown(xs: NonEmpty<Int>) -> String { \"{xs}\" }\nfn same(xs: NonEmpty<Int>) -> Bool { xs == xs }\nfn item(b: Full<Int>) -> Int { b.item + 1 }\nfn head(xs: NonEmpty<Int>) -> Int {\n  match xs {\n    [x, ..] => x,\n    [] => 0,\n  }\n}\nfn wrap() -> NonEmpty<Int> { NonEmpty.from([1, 2]) }\nfn checked() -> Result<NonEmpty<Int>, Violation> { NonEmpty.validate([1]) }\nfn small() -> Small<String> { 3 }\n";
    let c = check(&format!("{GENERIC}{src}"));
    assert!(c.diags.is_empty(), "{}", c.render());
}

#[test]
fn generic_refinements_keep_their_arguments_and_decide_literals() {
    let c = check(&format!(
        "{GENERIC}fn wrong(xs: NonEmpty<Int>) -> List<String> {{ xs }}\nfn big() -> Small<String> {{ 11 }}\n"
    ));
    assert_eq!(c.codes(), ["V2001", "V5004"], "{}", c.render());
}

#[test]
fn matches_on_a_refined_list_name_the_missing_case() {
    let c = check(&format!(
        "{GENERIC}fn f(xs: NonEmpty<Int>) -> Int {{\n  match xs {{\n    [] => 0,\n  }}\n}}\n"
    ));
    assert_eq!(c.codes(), ["V4001"], "{}", c.render());
    assert!(
        c.render().contains("`[_, ..]` not covered"),
        "{}",
        c.render()
    );
}
