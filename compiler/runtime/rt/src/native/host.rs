//! The native host interface (ABI §1b): what an instance asks of its embedder. The runtime
//! calls it only on the instance's thread and never while a borrow the host could observe is
//! live; the host never receives the context, so it cannot re-enter the runtime. Inputs are
//! borrowed (copy-in by construction), answers are owned (the runtime copies them out).

use alloc::string::String;
use alloc::vec::Vec;

/// A host handle as the guest's `Handle` object names it (ABI §8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleId {
    pub kind: u32,
    pub slot: u32,
    pub generation: u32,
}

/// Why a host operation produced no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// The operation's ordinary `Err(reason)` (statuses 1 and 2 of ABI §8.2: a refusal under
    /// `[permissions]` is reported the same way, as JS).
    Err(String),
    /// The host failed (status 3): defect `D091` with this message.
    Failure(String),
    /// The guest handed the host something a checked program cannot (a forged or foreign
    /// handle): defect `D090` with this message (ABI §8.4).
    Fault(String),
}

pub type Hosted<T> = Result<T, HostError>;

/// `std::fs::Mode`, the constructor index the guest passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    Read,
    Write,
    Append,
}

/// A `std::sql::SqlValue` bound as a parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SqlValue<'a> {
    Null,
    Int(i64),
    Float(f64),
    Text(&'a str),
    Blob(&'a [u8]),
}

/// A live worker's grants (`std/worker`'s `Permissions`, ABI 1.15.0): the table the worker
/// runs with, checked by the host against its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkerGrants {
    pub fs_read: Vec<String>,
    pub fs_write: Vec<String>,
    pub net: Vec<String>,
    pub env: Vec<String>,
}

/// A `std::request::Request` as a host sends it or a cassette keys it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpRequest<'a> {
    /// Upper case, as `fetch` sends it (`GET`).
    pub method: &'a str,
    pub url: &'a str,
    pub headers: Vec<(&'a str, &'a str)>,
    /// The payload's bytes (`Text` as UTF-8); `None` for `Empty`.
    pub body: Option<&'a [u8]>,
    /// `secret_headers` with their values: only `http_send` and `http_stream_open` (imports of
    /// kind `secret`, ABI §8.1) see them; the cassette operations get none.
    pub secret_headers: Vec<(&'a str, &'a str)>,
}

/// A response a cassette replays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: i64,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// What the runtime answers a JS-built handler's host while it runs a clause (design §17.308
/// (3)): the host names types and never learns a layout id or touches the heap.
pub trait Guest {
    /// The layout type id of the record or variant `name` (`std::request::Request`).
    fn type_id(&mut self, name: &str) -> Option<u32>;
    /// `Json.encode` of the `sakura-vt1` `std::json::Json` value `json` (`JSON.stringify`'s text).
    fn json_text(&mut self, json: &[u8]) -> Option<String>;
    /// `Json.schema` of a clause's schema argument, as a `sakura-vt1` `std::json::Json` value.
    fn json_schema(&mut self, schema: &str) -> Option<Vec<u8>>;
    /// `Json.parse(text)` (JS `JSON.parse`, then its `Json`): the `sakura-vt1`
    /// `Result<Json, DecodeError>`.
    fn json_parse(&mut self, text: &str) -> Option<Vec<u8>>;
}

/// One instance's host. Methods without a default are the root handlers' imports, which every
/// host provides; the `hosted` builtins default to a host failure (`D091`), so a host
/// implements only the services it offers.
pub trait Host {
    fn console_print(&mut self, line: &str);
    /// The next line of input without its newline, `None` at the end.
    fn console_read_line(&mut self) -> Option<String>;
    /// `Console.eprint`: a stderr line, as `log_write` writes one.
    fn console_eprint(&mut self, line: &str) {
        self.log_write(line);
    }
    fn clock_now_ms(&mut self) -> i64;
    fn clock_monotonic_ms(&mut self) -> i64;
    /// Microseconds on the same clock as `clock_monotonic_ms`.
    fn clock_monotonic_us(&mut self) -> i64;
    /// `async` (ABI §8.2): complete `request` (`Instance::complete`) after `ms` milliseconds.
    fn clock_sleep_ms(&mut self, request: u32, ms: i64);
    /// Cancel the host side of `request` (ABI §8.5); a later completion is discarded anyway.
    fn abort(&mut self, request: u32);
    /// Uniform in `lo..=hi` (`lo <= hi` is checked by the runtime).
    fn random_int(&mut self, lo: i64, hi: i64) -> i64;
    /// Uniform in `[0, 1)`.
    fn random_float(&mut self) -> f64;
    fn random_bytes(&mut self, n: usize) -> Vec<u8>;
    /// A canonical lowercase version-4 UUID.
    fn random_uuid(&mut self) -> String;
    /// One stderr line (`Log.stderr()`).
    fn log_write(&mut self, line: &str);
    /// Whether effect traces are recorded (`SAKURA_TRACE`), read once at instantiation.
    fn trace_enabled(&mut self) -> bool;
    fn trace_write(&mut self, line: &str);
    /// The guest's last reference to `handle` died (ABI §8.4).
    fn retire_handle(&mut self, handle: HandleId);

    /// `std/worker` topics (ABI 1.15.0): deliver topic `name`'s messages from other instances
    /// to this one from now on (`Instance::topic_deliver`, between runs).
    fn topic_listen(&mut self, _name: &str) {}
    /// `text` published on topic `name` here, for the subscriptions of other instances.
    fn topic_post(&mut self, _name: &str, _text: &str) {}

    /// `std/worker` (ABI 1.15.0): a live worker running the `@worker` function whose adapter is
    /// code `code` with `grants` and `capacity` messages per direction: its id (1, 2, …), -1
    /// when the grants are not within this instance's own table or leave an effect the entry
    /// declares without its permission, -2 at 16 live workers, -3 when `code` is not a
    /// `@worker` function's, -4 when this host has no live workers.
    fn worker_start(&mut self, _code: u32, _grants: WorkerGrants, _capacity: i64) -> i64 {
        -4
    }
    /// `async`: send `text` to worker `id` (0: the parent) and complete `request` with the
    /// `sakura-vt1` `Int` 0, or 2 once the other side has ended, waiting while it holds
    /// `capacity` unreceived messages.
    fn worker_post(&mut self, _request: u32, _id: i64, _text: &str) {}
    /// `async`: complete `request` with the `sakura-vt1` tuple `(0, text)` of worker `id`'s
    /// oldest message, then `(1, "")` once it finished or `(2, code)` once it failed.
    fn worker_take(&mut self, _request: u32, _id: i64) {}
    /// `async`: complete `request` (no payload) once worker `id`'s thread has exited.
    fn worker_join(&mut self, _request: u32, _id: i64) {}
    /// Terminate worker `id`; it does not wait for the thread.
    fn worker_stop(&mut self, _id: i64) {}
    /// Worker `id` (0: the parent) receives nothing more from this side: once it has taken
    /// what was sent, its `take` answers `(1, "")`.
    fn worker_shut(&mut self, _id: i64) {}

    /// `Cli.args()`: the program's arguments.
    fn cli_args(&mut self) -> Hosted<Vec<String>> {
        Err(unsupported())
    }
    fn env_get(&mut self, _key: &str) -> Hosted<Option<String>> {
        Err(unsupported())
    }
    fn fs_root(&mut self) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_memory_root(&mut self, _files: Vec<(String, Option<Vec<u8>>)>) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_subdir(&mut self, _dir: HandleId, _name: &str) -> Hosted<HandleId> {
        Err(unsupported())
    }
    fn fs_read(&mut self, _dir: HandleId, _path: &str) -> Hosted<Vec<u8>> {
        Err(unsupported())
    }
    fn fs_write(&mut self, _dir: HandleId, _path: &str, _data: &[u8]) -> Hosted<()> {
        Err(unsupported())
    }
    /// Entries `(name, is_dir)` sorted by name.
    fn fs_list(&mut self, _dir: HandleId, _path: &str) -> Hosted<Vec<(String, bool)>> {
        Err(unsupported())
    }
    fn fs_exists(&mut self, _dir: HandleId, _path: &str) -> Hosted<bool> {
        Err(unsupported())
    }
    /// The id of a new open file (`File.id`).
    fn file_open(&mut self, _dir: HandleId, _path: &str, _mode: FileMode) -> Hosted<i64> {
        Err(unsupported())
    }
    fn file_read(&mut self, _id: i64, _max: i64) -> Hosted<Vec<u8>> {
        Err(unsupported())
    }
    fn file_write(&mut self, _id: i64, _data: &[u8]) -> Hosted<()> {
        Err(unsupported())
    }
    /// Closes file `id`; a second close does nothing.
    fn file_close(&mut self, _id: i64) {}

    /// `http_body_of(bytes)`: a `Body` handle (kind `vt::BODY`) that delivers `bytes`, for
    /// handlers without a connection (`Http.scripted`, cassettes).
    fn http_body_of(&mut self, _bytes: &[u8]) -> Hosted<HandleId> {
        Err(unsupported())
    }
    /// `http_stream_read(body, max)`: up to `max` bytes, empty at the end, `Err` once closed.
    fn http_body_read(&mut self, _body: HandleId, _max: i64) -> Hosted<Vec<u8>> {
        Err(unsupported())
    }
    /// Closes `body` (`http_stream_closing`'s finalizer); a second close does nothing.
    fn http_body_close(&mut self, _body: HandleId) {}
    /// Whether reading `body` waits for the network (a live body with nothing buffered):
    /// then the read is `http_body_fetch`, else `http_body_read`.
    fn http_body_waits(&mut self, _body: HandleId) -> bool {
        false
    }
    /// `async`: complete `request` with status 0 and up to `max` raw bytes of `body` (empty at
    /// its end), or 1 and the reason.
    fn http_body_fetch(&mut self, _request: u32, _body: HandleId, _max: i64) {}
    /// `async` (`Http.host()`'s `http_send`): send `req` with `redirect: manual` and complete
    /// `request` with status 0 and the `sakura-vt1` tuple `(Int status, List<(String, String)>
    /// headers, Bytes body)`, or 1 and the reason no response arrived; 3 is a host failure.
    fn http_send(&mut self, _request: u32, _req: &HttpRequest<'_>) {}
    /// Why `req` is refused before it is sent (the manifest's `net` list), answered at once as
    /// the `Err` of `http_send` or `http_stream_open`, with no request.
    fn http_refused(&mut self, _req: &HttpRequest<'_>) -> Option<String> {
        None
    }
    /// `async`: as `http_send`, completing with `(Int status, List<(String, String)> headers,
    /// Handle body)` (kind `vt::BODY`) once the status and headers arrive.
    fn http_stream_open(&mut self, _request: u32, _req: &HttpRequest<'_>) {}
    /// `Http.recording`'s cassette at `path` inside `dir`: its id, or `Err(reason)` naming the
    /// file (and the line of a malformed one).
    fn http_recording_open(&mut self, _dir: HandleId, _path: &str) -> Hosted<i64> {
        Err(unsupported())
    }
    /// The next recorded response for `req` in cassette `id`; `Ok(None)` is a miss while
    /// recording, `Err(reason)` a miss otherwise or a refused credential header.
    fn http_recording_lookup(
        &mut self,
        _id: i64,
        _req: &HttpRequest<'_>,
    ) -> Hosted<Option<HttpResponse>> {
        Err(unsupported())
    }
    /// Append one exchange to cassette `id` (only while recording).
    fn http_recording_append(
        &mut self,
        _id: i64,
        _req: &HttpRequest<'_>,
        _resp: &HttpResponse,
    ) -> Hosted<()> {
        Err(unsupported())
    }

    /// A JS-built handler (`Llm.anthropic`): build `name` from its arguments (a `sakura-vt1`
    /// tuple, `Secret`s with their values: kind `secret`, ABI §8.1); its instance id.
    fn js_handler(&mut self, _name: &str, _args: &[u8]) -> Hosted<i64> {
        Err(unsupported())
    }
    /// Run operation `op` of handler `instance` on its arguments (a tuple; a closure argument
    /// crosses as `Unit` and stays in the guest) until it answers (`wasm.rs` `js_answer`'s
    /// format, plus answer 5: decode text by a schema on the clause's task, then resume).
    fn js_clause(&mut self, _g: &mut dyn Guest, _instance: i64, _op: u32, _args: &[u8]) -> Vec<u8> {
        js_unsupported()
    }
    /// Resume `session` with the operation's result (`status` 0) or failure (1), one value.
    fn js_resume(&mut self, _g: &mut dyn Guest, _session: u32, _status: u32, _v: &[u8]) -> Vec<u8> {
        js_unsupported()
    }
    /// Run the host function behind `callback` handle `id` on its argument (one value).
    fn js_callback(&mut self, _g: &mut dyn Guest, _id: i64, _arg: &[u8]) -> Vec<u8> {
        js_unsupported()
    }

    /// `Sql.memory()` (`path` `None`) or `Sql.sqlite(path)`: a new database's id. Its handle
    /// (kind `vt::SQL`, slot the id) retires with the handler. A file opens at `sql_root`.
    fn sql_open(&mut self, _path: Option<&str>) -> Hosted<u32> {
        Err(unsupported())
    }
    /// Whether database `db` waits for `sql_root` before its first operation.
    fn sql_needs_root(&mut self, _db: u32) -> bool {
        false
    }
    /// Open `db`'s file inside `dir` (the guest's `Fs.root()`); `Err(reason)` fails the
    /// operation, and the next one asks again.
    fn sql_root(&mut self, _db: u32, _dir: HandleId) -> Hosted<()> {
        Err(unsupported())
    }
    /// `async`: run one statement on `db` and complete `request` (`Instance::complete`). With
    /// `rows`, status 0's payload is the `sakura-vt1` tuple of the column names (`List<String>`)
    /// and the rows (a `List` of tuples of `Unit` for NULL, `Int`, `Float`, `String` or `Bytes`
    /// cells); without, the `Int` of rows changed. Status 1 carries SQLite's message (the
    /// `SqlError`), 3 a host failure's text (`D091`). `abort(request)` interrupts it.
    fn sql_run(
        &mut self,
        _request: u32,
        _db: u32,
        _text: &str,
        _params: &[SqlValue<'_>],
        _rows: bool,
    ) {
    }
    /// A savepoint on `db`: its token, or `Err(reason)`.
    fn sql_begin(&mut self, _db: u32) -> Hosted<i64> {
        Err(unsupported())
    }
    /// End savepoint `token`, released (`commit`) or rolled back; a second end does nothing.
    /// A failed release is a host failure (`D091`); a failed rollback has nowhere to go.
    fn sql_end(&mut self, _db: u32, _token: i64, _commit: bool) -> Hosted<()> {
        Ok(())
    }

    /// `Serve.host()` (async): listen on `host:port` and hand each request to
    /// `Instance::serve_invoke` with `listener`; complete `request` with the UTF-8 reason of
    /// the `ServeError` only when the server cannot start or stops. `abort(request)` stops it.
    fn serve_listen(&mut self, _request: u32, _listener: u32, _host: &str, _port: i64) {}
    /// The response of the served request answered by `task`: the `sakura-vt1` tuple of the
    /// `Response`.
    fn serve_respond(&mut self, _task: u32, _response: &[u8]) {}

    /// The test runner's `std::test::Test` operations (`sakura test --target native`, as the
    /// WASM host's `test_*` imports): the running test's failures and captured output, its
    /// snapshot check and `tests/fixtures` (a read-only `Dir`).
    fn test_record_failure(&mut self, _message: &str) {}
    fn test_failures(&mut self) -> i64 {
        0
    }
    fn test_output(&mut self, _line: &str) {}
    fn test_output_lines(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn test_snapshot(&mut self, _name: &str, _rendered: &str) {}
    fn test_fixtures(&mut self) -> Hosted<HandleId> {
        Err(unsupported())
    }
    /// The text of `tests/regressions/<name>.json` (`sakura test --contracts`), replayed before
    /// a contract test's generated cases; `None` when there is none.
    fn test_regression(&mut self, _name: &str) -> Option<String> {
        None
    }
}

/// Answer 2, `D090`: a JS-built handler this host does not run.
fn js_unsupported() -> Vec<u8> {
    let mut out = alloc::vec![2, 90, 0, 0, 0];
    out.extend_from_slice(b"a host operation this native host does not provide");
    out
}

fn unsupported() -> HostError {
    HostError::Failure("a host operation this native host does not provide".into())
}
