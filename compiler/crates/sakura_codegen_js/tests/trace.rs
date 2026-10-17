//! Effect traces (design §17.80): `SAKURA_TRACE=<path>` records every performed operation and
//! every finalizer run, cancellation and defect, in order, with the task that did it.

mod common;

#[test]
fn traces_record_performs_finalizers_cancellations_and_defects() {
    let src = r#"import std/fs.{Fs, Write}
import std/path.{Path}
import std/task.{Task}
pub fn main() -> Unit ! {Console} {
  with Fs.memory([]) {
    let r = try { Fs.with_file(Path.of("a.txt"), Write, fn(f) { Fs.write_chunk(f, String.to_bytes("x")) }) }
    Console.print("{r.is_ok()}")
  }
  with Clock.virtual(), Task.deterministic(0) {
    let w = Task.race([fn() { nap(10, 1) }, fn() { nap(5, 2) }])
    Console.print("{w}")
  }
  Console.print("{1 / List.len([])}")
}

fn nap(ms: Int, v: Int) -> Int ! {Clock} {
  Clock.sleep(Duration.millis(ms))
  v
}
"#;
    let (m, sources) = common::build("trace", src, false).unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("trace");
    common::write(&m, sources, &dir);
    let path = dir.join("trace.jsonl");
    let out = std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .env("SAKURA_TRACE", &path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(70));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "true\n2\n");
    let trace = std::fs::read_to_string(&path).unwrap();
    let want = [
        r#"{"seq":0,"task":0,"effect":"std::fs::Fs","op":"with_file"}"#,
        r#"{"seq":1,"task":0,"effect":"std::fs::Fs","op":"write_chunk"}"#,
        r#"{"seq":2,"task":0,"event":"finalizer"}"#,
        r#"{"seq":3,"task":0,"effect":"std::console::Console","op":"print"}"#,
        r#"{"seq":4,"task":0,"effect":"std::task::Task","op":"race"}"#,
        r#"{"seq":5,"task":1,"effect":"std::clock::Clock","op":"sleep"}"#,
        r#"{"seq":6,"task":2,"effect":"std::clock::Clock","op":"sleep"}"#,
        r#"{"seq":7,"task":2,"event":"cancel"}"#,
        r#"{"seq":8,"task":0,"effect":"std::console::Console","op":"print"}"#,
        r#"{"seq":9,"task":0,"event":"defect","code":"D002"}"#,
    ];
    assert_eq!(trace.lines().collect::<Vec<_>>(), want, "{trace}");
    // Without SAKURA_TRACE nothing is written.
    std::fs::remove_file(&path).unwrap();
    std::process::Command::new("node")
        .arg(dir.join("__main__.js"))
        .env_remove("SAKURA_TRACE")
        .output()
        .unwrap();
    assert!(!path.exists());
}
