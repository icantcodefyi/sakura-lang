//! The playground's compiler (website `/play`): one file checked, and on request built to JS, in
//! the browser, the way `sakura check` and `sakura run` do it. Nothing here runs the program or
//! verifies it: the page runs the JS in a Web Worker, and `sakura verify` needs Z3.
//!
//! The wasm32 module's ABI is four exports over one byte buffer, so the page needs no glue:
//! `buffer(len)` sizes it and returns its address, the page writes the UTF-8 source there,
//! `compile(len, emit)` replaces it with the answer's JSON and returns that length, and
//! `buffer_ptr()` gives its (possibly moved) address.

use std::sync::Mutex;

use sakura_analysis::{Analysis, BuildOptions, Snapshot, disk};
use sakura_diagnostics::{
    Diagnostic, Severity, SourceFile, diagnostic_records, hole_records, render_human,
};
use serde_json::{Value, json};

/// The program's one file, shown as `main.sak`.
const URI: &str = "file:///play/main.sak";
const CWD: &str = "file:///play";

static BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// `source` checked, and with `emit` built for `sakura run`, as
/// `{ "version", "human", "diagnostics", "holes", "errors", "effects", "files" }`: `human` is what
/// `sakura check` (or `sakura run`, on errors) prints, `diagnostics` and `holes` its `--json`
/// records, `effects` the effects `main` leaves to the host (`null` without `main`), and
/// `files` the emitted JS (`[path, text]`, no source maps), `null` unless `emit` and no errors.
pub fn compile(source: &str, emit: bool) -> Value {
    let snapshot = Snapshot::with_std([(URI.to_owned(), source.to_owned())]);
    let analyzed = Analysis::analyze(&snapshot);
    let (diags, holes, files) = if emit {
        let opts = BuildOptions {
            need_entry: true,
            ..BuildOptions::default()
        };
        let built = Analysis::build_js(&snapshot, &opts);
        (built.diagnostics, Vec::new(), built.files)
    } else {
        (analyzed.diagnostics(), analyzed.check.holes.clone(), None)
    };
    let errors = diags.iter().any(Diagnostic::is_error);
    let shown: Vec<SourceFile> = (snapshot.files().iter())
        .map(|f| SourceFile {
            uri: disk::display_uri(&f.uri, CWD),
            ..f.clone()
        })
        .collect();
    let mut human = render_human(&diags, &shown);
    for h in &holes {
        let locals: Vec<String> = (h.locals.iter())
            .map(|(n, t)| format!("{n}: {}", t.render))
            .collect();
        let candidates: Vec<&str> = h.candidates.iter().map(|(e, _)| e.as_str()).collect();
        human.push_str(&format!(
            "hole ?{}: expected {}, effects {}; locals [{}]; candidates [{}]\n",
            h.name,
            h.expected_type.render,
            h.allowed_effects.render,
            locals.join(", "),
            candidates.join(", ")
        ));
    }
    human.push_str(&tally(&diags));
    let files = files.filter(|_| !errors).map(|files| {
        (files.into_iter())
            .filter(|(path, _)| !path.ends_with(".map"))
            .collect::<Vec<_>>()
    });
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "human": human,
        "diagnostics": diagnostic_records(&diags, snapshot.files(), snapshot.hash()),
        "holes": hole_records(&holes, snapshot.files(), snapshot.hash()),
        "errors": errors,
        "effects": analyzed.main_effects(),
        "files": files,
    })
}

/// `sakura check`'s last line.
fn tally(diags: &[Diagnostic]) -> String {
    let count = |s| diags.iter().filter(|d| d.severity == s).count();
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (count(Severity::Error), count(Severity::Warning)) {
        (0, 0) => "no problems".into(),
        (e, 0) => plural(e, "error"),
        (0, w) => plural(w, "warning"),
        (e, w) => format!("{}, {}", plural(e, "error"), plural(w, "warning")),
    }
}

fn buffer_lock() -> std::sync::MutexGuard<'static, Vec<u8>> {
    BUFFER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sizes the buffer to `len` bytes and returns its address, for the page to write the source.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn buffer(len: usize) -> *mut u8 {
    let mut b = buffer_lock();
    b.clear();
    b.resize(len, 0);
    b.as_mut_ptr()
}

/// The buffer's address (it moves when `compile` writes the answer).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn buffer_ptr() -> *const u8 {
    buffer_lock().as_ptr()
}

/// [`compile`]s the first `len` bytes of the buffer (invalid UTF-8 is replaced, as `V0015`'s
/// text is) and replaces the buffer with the answer's JSON; returns its length.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn compile_buffer(len: usize, emit: u32) -> usize {
    let source = {
        let b = buffer_lock();
        String::from_utf8_lossy(&b[..len.min(b.len())]).into_owned()
    };
    let answer = compile(&source, emit != 0).to_string().into_bytes();
    let n = answer.len();
    *buffer_lock() = answer;
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_hello_and_reports_errors() {
        let ok = compile(
            "import std/console.{Console}\n\npub fn main() -> () ! {Console} {\n    Console.print(\"hi\")\n}\n",
            true,
        );
        assert_eq!(ok["errors"], false, "{}", ok["human"]);
        assert_eq!(ok["effects"], json!(["Console"]));
        let files = ok["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f[0] == "__main__.js"));
        assert!(
            !files
                .iter()
                .any(|f| f[0].as_str().unwrap().ends_with(".map"))
        );

        let bad = compile("pub fn main() -> Int { \"no\" }\n", false);
        assert_eq!(bad["errors"], true);
        assert_eq!(bad["files"], Value::Null);
        assert!(
            bad["diagnostics"][0]["code"]
                .as_str()
                .unwrap()
                .starts_with('V')
        );
        assert!(bad["human"].as_str().unwrap().contains("main.sak"));
    }
}
