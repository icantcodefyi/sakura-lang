//! Design §17.107 (plan §3.6 C1): Core itself carries each parameter's refinement and lowers the
//! `T$check` of every refined type a signature names, and its text form tells monomorphised
//! copies apart, so the verifier reads Core alone.

use sakura_analysis::{Analysis, Snapshot};
use sakura_core::{FnKind, Module, Ty};

const PROGRAM: &str = r#"
type Percent = Int where it >= 0 && it <= 100
type Small = Int where it < 10

fn scale(p: Percent, total: Int) -> Int { total * p / 100 }

// `Small` is named only inside a signature's type and never introduced.
fn count(xs: List<Small>) -> Int { List.len(xs) }

fn run<T: Show>(x: T) -> String { "{x}" }

pub fn main() -> Unit {
  let _a = scale(Percent.from(50), 10)
  let _b = count([])
  let _c = run(1)
  let _d = run(1.5)
}
"#;

fn core() -> Module {
    let snap = Snapshot::with_std([("file:///c1/src/main.sakura".into(), PROGRAM.into())]);
    let (m, diags) = Analysis::lower_core(&snap, &Default::default());
    m.unwrap_or_else(|| panic!("{diags:?}"))
}

fn user_def(m: &Module, name: &str) -> Vec<usize> {
    (0..m.defs.len())
        .filter(|&i| m.defs[i].name == name && m.defs[i].module == "main::main")
        .collect()
}

#[test]
fn refined_parameters_name_their_check() {
    let m = core();
    let [scale] = user_def(&m, "scale")[..] else {
        panic!("one scale")
    };
    let d = &m.defs[scale];
    assert_eq!(d.refined_params.len(), d.params.len());
    assert_eq!(d.refined_params[1], None, "`total` is not refined");
    let r = &m.refinements[d.refined_params[0].expect("`p` is refined").index()];
    assert_eq!(r.qualified_name(), "main::main::Percent");
    let check = m.def(r.check);
    assert_eq!(check.name, "Percent$check");
    assert_eq!((check.params.len(), &check.ret), (1, &Ty::Bool));
    // Backends see the base type.
    assert_eq!(d.local(d.params[0]).ty, Ty::Int);
    // A def without refined parameters keeps the field empty.
    let [main] = user_def(&m, "main")[..] else {
        panic!("one main")
    };
    assert!(m.defs[main].refined_params.is_empty());
}

#[test]
fn every_refinement_a_signature_names_is_lowered() {
    let m = core();
    let small = m
        .refinements
        .iter()
        .find(|r| r.name == "Small")
        .expect("Small");
    let check = m.def(small.check);
    assert_eq!(check.name, "Small$check");
    assert!(matches!(check.kind, FnKind::Top { .. }));
    assert_eq!(check.ret, Ty::Bool);
}

#[test]
fn the_text_form_tells_copies_apart() {
    let m = core();
    let runs = user_def(&m, "run");
    assert!(runs.len() >= 2, "a copy per `Show` type argument");
    let text = m.to_string();
    let called = runs
        .iter()
        .filter(|i| text.contains(&format!("call @run#{i}(")))
        .count();
    assert_eq!(called, 2, "{text}");
    let [scale] = user_def(&m, "scale")[..] else {
        panic!("one scale")
    };
    let r = m.defs[scale].refined_params[0].unwrap().0;
    assert!(
        text.contains(&format!(
            "fn scale#{scale}(p%0: Int where Percent#{r}, total%1: Int)"
        )),
        "{text}"
    );
}
