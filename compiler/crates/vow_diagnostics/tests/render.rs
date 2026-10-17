use serde_json::Value;
use vow_diagnostics::codes::{V0001, V3001, V9001};
use vow_diagnostics::*;

fn hello() -> Vec<SourceFile> {
    // `é` is two UTF-8 bytes, so every later byte offset differs from the char offset.
    vec![SourceFile::new(
        "file:///work/main.vow",
        "fn greet(name: String) -> Unit {\n  Console.print(\"héllo, {name}\")\n}\n",
    )]
}

fn v3001(files: &[SourceFile]) -> Diagnostic {
    let text = &files[0].text;
    let call = text.find("Console.print").unwrap() as u32;
    let sig = text.find("Unit").unwrap() as u32 + 4;
    Diagnostic::error(
        V3001,
        Phase::Effect,
        "function `greet` performs `Console`, which its signature does not allow",
    )
    .with_symbol("main::greet")
    .with_span(Span::primary(FileId(0), call, call + 13).with_label("`Console` performed here"))
    .with_span(
        Span::primary(FileId(0), call, text.find(")\n").unwrap() as u32 + 1)
            .with_role(SpanRole::Cause)
            .with_label("`Console.print` introduces `Console` here"),
    )
    .with_expected(Rendered {
        render: "{}".into(),
        structured: Some(serde_json::json!({"row": {"labels": [], "tail": null}})),
    })
    .with_actual(Rendered {
        render: "{Console}".into(),
        structured: None,
    })
    .with_fix(Fix {
        title: "Add `Console` to the signature of `greet`".into(),
        applicability: Applicability::MachineApplicable,
        impact: Impact::InterfaceChange,
        edits: vec![Edit {
            span: Span::primary(FileId(0), sig, sig),
            replacement: " ! {Console}".into(),
        }],
    })
}

fn lines(s: &str) -> Vec<Value> {
    s.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn diagnostic_record_matches_schema_shape() {
    let files = hello();
    let out = render_json_lines(&[v3001(&files)], &files, "sha256:snap");
    insta::assert_snapshot!(out);
    let recs = lines(&out);
    assert_eq!(recs.len(), 2);
    let d = recs[0].as_object().unwrap();
    for key in [
        "schema_version",
        "kind",
        "id",
        "code",
        "severity",
        "phase",
        "message",
        "snapshot",
        "symbol_id",
        "spans",
        "related_ids",
        "expected",
        "actual",
        "obligation",
        "counterexample",
        "explanation_uri",
        "fixes",
    ] {
        assert!(d.contains_key(key), "missing {key}");
    }
    assert_eq!(d.len(), 17, "no extra keys");
    assert_eq!(recs[1]["kind"], "summary");
    assert_eq!(recs[1]["truncated"], false);
}

#[test]
fn byte_offsets_are_utf8_and_slice_the_source() {
    let files = hello();
    let rec = &lines(&render_json_lines(&[v3001(&files)], &files, "s"))[0];
    let cause = &rec["spans"][1];
    let (s, e) = (
        cause["start_byte"].as_u64().unwrap() as usize,
        cause["end_byte"].as_u64().unwrap() as usize,
    );
    assert_eq!(&files[0].text[s..e], "Console.print(\"héllo, {name}\")");
    assert_eq!(cause["content_hash"], files[0].content_hash.as_str());
    assert!(files[0].content_hash.starts_with("sha256:") && files[0].content_hash.len() == 71);
}

#[test]
fn out_of_range_spans_are_clamped_not_panics() {
    let files = hello();
    let len = files[0].text.len() as u32;
    // 37 is inside the two-byte `é`; human rendering must not slice through it.
    let d = Diagnostic::error(V9001, Phase::Internal, "bad span")
        .with_span(Span::primary(FileId(0), 37, len + 50))
        .with_span(Span::primary(FileId(7), 1, 2).with_role(SpanRole::Use));
    let rec = &lines(&render_json_lines(std::slice::from_ref(&d), &files, "s"))[0];
    assert_eq!(rec["spans"][0]["end_byte"], len);
    assert_eq!(rec["spans"][1]["uri"], "vow:///unknown");
    let human = render_human(&[d], &files);
    assert!(human.contains("error[V9001]: bad span"), "{human}");
}

#[test]
fn ordering_roots_first_then_position_and_ids_are_deterministic() {
    let files = hello();
    let at = |code, start, msg: &str| {
        Diagnostic::error(code, Phase::Parse, msg).with_span(Span::primary(
            FileId(0),
            start,
            start + 1,
        ))
    };
    let diags = vec![
        at(V0001, 5, "cascade").with_related(2),
        at(V0001, 9, "second"),
        at(V0001, 3, "first"),
        at(V0001, 3, "first"),
    ];
    let a = render_json_lines(&diags, &files, "s");
    assert_eq!(a, render_json_lines(&diags, &files, "s"));
    let recs = lines(&a);
    let msgs: Vec<_> = recs[..4]
        .iter()
        .map(|r| r["message"].as_str().unwrap())
        .collect();
    assert_eq!(msgs, ["first", "first", "second", "cascade"]);
    assert_ne!(recs[0]["id"], recs[1]["id"]);
    assert_eq!(recs[3]["related_ids"][0], recs[0]["id"]);
    assert_eq!(
        recs[1]["id"],
        format!("{}-1", recs[0]["id"].as_str().unwrap())
    );
}

#[test]
fn output_is_capped_with_a_summary() {
    let files = hello();
    let diags: Vec<_> = (0..250)
        .map(|i| {
            Diagnostic::error(V0001, Phase::Parse, format!("e{i}")).with_span(Span::primary(
                FileId(0),
                0,
                1,
            ))
        })
        .collect();
    let recs = lines(&render_json_lines(&diags, &files, "s"));
    assert_eq!(recs.len(), MAX_DIAGNOSTICS + 1);
    let summary = recs.last().unwrap();
    assert_eq!(summary["truncated"], true);
    assert_eq!(summary["total"], 250);
    assert_eq!(summary["errors"], 250);
}

#[test]
fn human_rendering() {
    let files = hello();
    insta::assert_snapshot!(render_human(&[v3001(&files)], &files));
}

#[test]
fn every_registry_code_explains_itself() {
    for info in codes::REGISTRY {
        assert_eq!(codes::lookup(info.code.0).unwrap().code, info.code);
    }
}

#[test]
fn rendered_row_has_the_schema_shape() {
    let r = Rendered::row(
        "{Clock, Db | e}",
        vec![
            ("std::Clock".into(), vec![]),
            ("app::Db".into(), vec!["Int".into()]),
        ],
        Some("e".into()),
    );
    let d = Diagnostic::error(V3001, Phase::Effect, "m")
        .with_span(Span::primary(FileId(0), 0, 1))
        .with_expected(r)
        .with_actual(Rendered::text("{}"));
    let rec = &lines(&render_json_lines(&[d], &hello(), "s"))[0];
    assert_eq!(
        rec["expected"],
        serde_json::json!({"render": "{Clock, Db | e}", "row": {
            "labels": [{"effect": "std::Clock", "args": []}, {"effect": "app::Db", "args": ["Int"]}],
            "tail": "e"}})
    );
    assert_eq!(rec["actual"], serde_json::json!({"render": "{}"}));
}

#[test]
fn hole_record_shape() {
    let files = hello();
    let at = files[0].text.find("name}").unwrap() as u32;
    let hole = HoleReport {
        name: "result".into(),
        span: Span::primary(FileId(0), at, at + 4),
        expected_type: Rendered::text("Int"),
        allowed_effects: Rendered::row("{}", vec![], None),
        locals: vec![("x".into(), Rendered::text("Int"))],
        candidates: vec![("x".into(), "exact_local".into())],
        contracts_in_scope: vec![],
    };
    let recs = hole_records(&[hole], &files, "sha256:snap");
    insta::assert_json_snapshot!(recs);
    assert!(recs[0]["id"].as_str().unwrap().starts_with("h:"));
}
