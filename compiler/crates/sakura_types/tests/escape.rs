//! The scoped-value and resource escape check (design §6.3, §8.4, §17.70): `V6002`, `V6003`.

mod common;

use common::check;

const FS: &str = "import std/fs.{Fs, Dir, File, Mode, FsError}\nimport std/path.{Path}\n";

fn codes(src: &str) -> Vec<&'static str> {
    let c = check(&format!("{FS}{src}"));
    assert!(c.std_diags.is_empty(), "std: {:?}", c.std_diags);
    let codes = c.codes();
    eprintln!("{}", c.render());
    codes
}

fn clean(src: &str) {
    let c = check(&format!("{FS}{src}"));
    assert!(c.diags.is_empty(), "{}", c.render());
}

// ---- V6002 (a): the value of a `with` ----

#[test]
fn a_with_returning_its_handler_is_v6002() {
    let src = "fn leak() -> Handler<Fs> ! {Fs} {\n  with Fs.rooted(Fs.root()) { Fs.rooted(Fs.root()) }\n}\n";
    assert_eq!(codes(src), ["V6002"]);
}

#[test]
fn a_with_returning_a_dir_is_v6002() {
    let src = "fn leak() -> Dir ! {Fs} {\n  with Fs.rooted(Fs.root()) { Fs.root() }\n}\n";
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6002"], "{}", c.render());
    assert!(
        c.render()
            .contains("the value of this `with` is a `Dir`, a scoped value"),
        "{}",
        c.render()
    );
}

#[test]
fn a_lambda_capturing_a_dir_from_a_with_is_v6002() {
    let src = "type Count = fn() -> Int\nfn leak() -> Count ! {Fs} {\n  with Fs.rooted(Fs.root()) {\n    let d = Fs.root()\n    fn() { List.len([d]) }\n  }\n}\n";
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6002"], "{}", c.render());
    assert!(
        c.render().contains("captures a scoped value"),
        "{}",
        c.render()
    );
}

#[test]
fn a_lambda_capture_through_an_alias_and_a_record_is_v6002() {
    let src = "type Job = { run: fn() -> Int }\nfn leak() -> Job ! {Fs} {\n  with Fs.rooted(Fs.root()) {\n    let d = Fs.root()\n    let e = d\n    let f = fn() { List.len([e]) }\n    Job { run: f }\n  }\n}\n";
    assert_eq!(codes(src), ["V6002"]);
}

#[test]
fn a_record_holding_a_dir_returned_from_a_with_is_v6002() {
    let src = "type Place = { dir: Dir, name: String }\nfn leak() -> Place ! {Fs} {\n  with Fs.rooted(Fs.root()) { Place { dir: Fs.root(), name: \"x\" } }\n}\n";
    assert_eq!(codes(src), ["V6002"]);
}

#[test]
fn a_return_out_of_a_with_is_v6002() {
    let src = "fn leak(b: Bool) -> Dir ! {Fs} {\n  with Fs.rooted(Fs.root()) {\n    if b { return Fs.root() }\n  }\n  Fs.root()\n}\n";
    assert_eq!(codes(src), ["V6002"]);
}

#[test]
fn plain_values_and_ordinary_returns_of_scoped_values_are_fine() {
    // §6.3: a scoped value may be returned from a function and passed down the stack.
    clean(
        "fn root() -> Dir ! {Fs} { Fs.root() }\nfn files() -> Handler<Fs> { Fs.memory([]) }\nfn count() -> Int ! {Fs} {\n  with Fs.rooted(root()) {\n    let d = Fs.root()\n    let f = fn() { List.len([d]) }\n    f()\n  }\n}\n",
    );
}

#[test]
fn a_lambda_capturing_only_plain_values_may_leave_a_with() {
    clean(
        "type Count = fn() -> Int\nfn make() -> Count ! {Fs} {\n  with Fs.rooted(Fs.root()) {\n    let n = 3\n    fn() { n + 1 }\n  }\n}\n",
    );
}

// ---- V6002 (b): a top-level `let` ----

#[test]
fn a_top_level_handler_is_v6002() {
    let c = check(&format!("{FS}let files = Fs.memory([])\n"));
    assert_eq!(c.codes(), ["V6002"], "{}", c.render());
    assert!(
        c.render()
            .contains("a top-level `let` cannot hold `Handler<Fs>`"),
        "{}",
        c.render()
    );
}

#[test]
fn a_top_level_function_value_is_fine() {
    clean("let make = fn() { Fs.memory([]) }\n");
}

// ---- V6002 (c): `Task.spawn` ----

#[test]
fn a_task_capturing_a_handler_bound_inside_an_inner_with_is_v6002() {
    let src = "fn go() -> Unit ! {Task, Fs} {\n  Task.scope(fn(s) {\n    with Fs.rooted(Fs.root()) {\n      let h = Fs.rooted(Fs.root())\n      let t = Task.spawn(s, fn() { with h { Fs.exists(Path.of(\"a\")) } })\n      let _ = Task.join(t)\n    }\n  })\n}\n";
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6002"], "{}", c.render());
    assert!(
        c.render().contains("this task captures `h`"),
        "{}",
        c.render()
    );
    assert!(c.render().contains("Cause"), "{}", c.render());
}

#[test]
fn a_task_capturing_a_dir_bound_inside_an_inner_with_is_v6002() {
    let src = "fn go() -> Unit ! {Task, Fs} {\n  Task.scope(fn(s) {\n    with Fs.rooted(Fs.root()) {\n      let d = Fs.root()\n      let t = Task.spawn(s, fn() { List.len([d]) })\n      let _ = Task.join(t)\n    }\n  })\n}\n";
    assert_eq!(codes(src), ["V6002"]);
}

#[test]
fn a_task_whose_scope_is_inside_the_with_is_fine() {
    clean(
        "fn go() -> Unit ! {Task, Fs} {\n  with Fs.rooted(Fs.root()) {\n    let d = Fs.root()\n    Task.scope(fn(s) {\n      let t = Task.spawn(s, fn() { List.len([d]) })\n      let _ = Task.join(t)\n    })\n  }\n}\n",
    );
}

#[test]
fn a_task_capturing_values_bound_outside_the_with_is_fine() {
    clean(
        "fn go(d: Dir) -> Unit ! {Task, Fs} {\n  Task.scope(fn(s) {\n    with Fs.rooted(d) {\n      let n = 2\n      let t = Task.spawn(s, fn() { List.len([d]) + n })\n      let _ = Task.join(t)\n    }\n  })\n}\n",
    );
}

// ---- V6003: resources ----

#[test]
fn returning_the_file_from_with_file_is_v6003() {
    let src = "fn keep_open(p: Path) -> File ! {Fs, Fail<FsError>} {\n  Fs.with_file(p, Mode.Read, fn(file) { file })\n}\n";
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6003"], "{}", c.render());
    assert!(
        c.render()
            .contains("this `File` escapes the callback of `with_file`"),
        "{}",
        c.render()
    );
}

#[test]
fn a_record_holding_the_file_is_v6003() {
    let src = "type Open = { file: File, name: String }\nfn keep(p: Path) -> Open ! {Fs, Fail<FsError>} {\n  Fs.with_file(p, Mode.Read, fn(f) { Open { file: f, name: \"x\" } })\n}\n";
    assert_eq!(codes(src), ["V6003"]);
}

#[test]
fn a_closure_over_the_file_is_v6003() {
    let src = "fn later(p: Path) -> fn() -> Bytes ! {Fs, Fail<FsError>} ! {Fs, Fail<FsError>} {\n  Fs.with_file(p, Mode.Read, fn(f) { fn() { Fs.read_chunk(f, 10) } })\n}\n";
    assert_eq!(codes(src), ["V6003"]);
}

#[test]
fn a_file_inside_a_with_is_reported_once() {
    let src = "fn leak(p: Path) -> File ! {Fs, Fail<FsError>} {\n  with Fs.rooted(Fs.root()) { Fs.with_file(p, Mode.Read, fn(f) { f }) }\n}\n";
    assert_eq!(codes(src), ["V6003"]);
}

#[test]
fn a_wrapper_passing_the_callback_on_is_checked_at_its_caller() {
    let src = "fn open<T, e>(p: Path, body: fn(File) -> T ! e) -> T ! {Fs, Fail<FsError> | e} {\n  Fs.with_file(p, Mode.Read, body)\n}\nfn leak(p: Path) -> File ! {Fs, Fail<FsError>} { open(p, fn(f) { f }) }\nfn fine(p: Path) -> Bytes ! {Fs, Fail<FsError>} { open(p, fn(f) { Fs.read_chunk(f, 4) }) }\n";
    assert_eq!(codes(src), ["V6003"]);
}

#[test]
fn reading_inside_the_callback_is_fine() {
    clean(
        "fn head(p: Path) -> Bytes ! {Fs, Fail<FsError>} {\n  Fs.with_file(p, Mode.Read, fn(f) { Fs.read_chunk(f, 16) })\n}\n",
    );
}

#[test]
fn a_file_stored_in_handler_state_is_v6003() {
    let src = "effect Keep {\n  fn keep(f: File) -> Unit\n}\nfn keeper() -> Handler<Keep> {\n  handler Keep {\n    var last: Option<File> = None\n    fn keep(f) { last = Some(f) }\n  }\n}\n";
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6003"], "{}", c.render());
    assert!(
        c.render().contains("handler state cannot hold"),
        "{}",
        c.render()
    );
}

#[test]
fn a_user_resource_type_is_checked_like_file() {
    let src = "@resource\ntype Conn\neffect Db {\n  fn with_conn<T, e>(body: fn(Conn) -> T ! e) -> T ! e\n}\nfn leak() -> Conn ! {Db} { Db.with_conn(fn(c) { c }) }\n";
    assert_eq!(codes(src), ["V6003"]);
}

// ---- §14.3 acceptance: a route handler returning `Fs.File` ----

#[test]
fn a_route_handler_returning_a_file_is_v6003() {
    let src = "import std/router.{Router}\nimport std/response.{Response}\nfn routes(r: Router<{Fs, Fail<FsError>}>) -> Router<{Fs, Fail<FsError>}> {\n  Router.get(r, \"/file\", fn(req) {\n    let _ = Fs.with_file(Path.of(\"a.txt\"), Mode.Read, fn(f) { f })\n    Response.status(200, \"opened\")\n  })\n}\n";
    assert_eq!(codes(src), ["V6003"]);
}

// ---- §17.91: V6001 on a user `@external` that mints a capability ----

#[test]
fn a_user_external_returning_a_std_capability_is_v6001() {
    let c = check(&format!(
        "{FS}@external(js, \"./forge.js\", \"root\")\nfn forge() -> Dir\n"
    ));
    assert_eq!(c.codes(), ["V6001"], "{}", c.render());
    let r = c.render();
    assert!(
        r.contains("V6001 4:15 `Dir` (std/fs) is a capability of another package"),
        "{r}"
    );
}

#[test]
fn a_user_external_mentioning_a_capability_at_any_depth_is_v6001() {
    // Inside `Option`, a function type, a user record, a std resource and a handler.
    let src = "type Wrap = { dir: Dir }\n@external(js, \"./f.js\", \"a\")\nfn a(f: fn(Option<Dir>) -> Int) -> Int\n@external(js, \"./f.js\", \"b\")\nfn b() -> Wrap\n@external(js, \"./f.js\", \"c\")\nfn c(file: File) -> Int\n@external(js, \"./f.js\", \"d\")\nfn d() -> Handler<Fs>\n";
    let c = check(&format!("{FS}{src}"));
    let r = c.render();
    assert_eq!(c.codes(), ["V6001", "V6001", "V6001", "V6001"], "{r}");
    assert!(
        r.contains("`Wrap` (main/main) contains `Dir` (std/fs)"),
        "{r}"
    );
    assert!(r.contains("`File` (std/fs) is a capability"), "{r}");
    assert!(
        r.contains("a `Handler<…>` cannot cross a user `@external`"),
        "{r}"
    );
}

#[test]
fn a_package_may_declare_externals_over_its_own_external_types() {
    clean(
        "@resource\ntype Conn\ntype Pool = { conns: List<Conn> }\n@external(js, \"./db.js\", \"open\")\nfn open(url: String) -> Conn\n@external(js, \"./db.js\", \"pool\")\nfn pool(n: Int) -> Pool\nfn plain(d: Dir) -> Dir { d }\n",
    );
}

// ---- §17.88, §17.89: the callback rule on a parameter that *contains* a resource ----

const STREAM: &str = "@resource\ntype Body\ntype Incoming = { status: Int, body: Body }\neffect Net {\n  fn stream<T, e>(body: fn(Incoming) -> T ! e) -> T ! e\n  fn read(b: Body) -> Int\n}\n";

#[test]
fn a_callback_returning_a_record_holding_a_resource_is_v6003() {
    let src = format!(
        "{STREAM}fn whole() -> Incoming ! {{Net}} {{ Net.stream(fn(r) {{ r }}) }}\nfn body() -> Body ! {{Net}} {{ Net.stream(fn(r) {{ r.body }}) }}\n"
    );
    let c = check(&format!("{FS}{src}"));
    assert_eq!(c.codes(), ["V6003", "V6003"], "{}", c.render());
    assert!(
        c.render().contains("escapes the callback of `stream`"),
        "{}",
        c.render()
    );
}

#[test]
fn reading_a_streamed_body_inside_the_callback_is_fine() {
    clean(&format!(
        "{STREAM}fn size() -> Int ! {{Net}} {{ Net.stream(fn(r) {{ r.status + Net.read(r.body) }}) }}\n"
    ));
}
