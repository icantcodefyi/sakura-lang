# Friction log: Repair Queue on installed sakura 0.1.0

Author: Claude Opus 5.5, in one fresh session with no repository checkout. My sources
were the installed CLI (`sakura 0.1.0 (2014e821b87a0d30cb8061b6158f99723f0f676d)`), the
website `.md` exports (saved under `web/` while I worked) and BRIEF.md. Each item says
what I expected, what happened and how I got past it.

## 2026-10-21: the website recipe does not compile on the installed release

- **Expected:** The [router recipe](https://www.sakuralang.com/docs/cookbook/router_endpoint)
  would compile on 0.1.0. It does `import std/request.{BadRequest, Get, Request, Text}`
  and asserts `response.body == Text("…")`.
- **Happened:** I got `V1002: module std/request has no exported item Text`. On 0.1.0,
  `sakura doc std::response::Response` shows `body: String`, and `Request` has
  `body: String` with no `redirect` field. The website's stdlib page lists a
  `Payload = Empty | Text | Bytes` body that this release does not have. Both the website
  and the guide embedded in the binary (`sakura prompt`) say they "track main", so neither
  promises to match the installed release, and no versioned docs exist for 0.1.0.
- **Got past it:** I treated `sakura doc <symbol>` as the authority for this binary and
  used `String` bodies (`Json.decode(r.body)`).

## 2026-10-21: `sakura doc` cannot show a module or an effect operation

- **Expected:** `sakura doc std::sql` would list the module, and
  `sakura doc std::sql::transaction` would document `Sql.transaction`.
- **Happened:** Both print `no symbol matches`. So do `std::sql::Sql::transaction` and
  `--search transaction`. Only `sakura doc std::sql::Sql` prints the effect, as a
  one-line signature with no prose. I could find no documentation of whether a
  non-`SqlError` failure inside `Sql.transaction` rolls back.
- **Got past it:** I read the website stdlib page for module listings. I tested rollback
  in a scratch project: a domain `fail` inside the transaction body rolls back the earlier
  insert on js and wasm. Test 20 now covers the SQL-failure case.

## 2026-10-21: `error` is a keyword, so a JSON body cannot have an `error` field

- **Expected:** An error response shaped as `{"error": "...", "message": "..."}`, the
  common JSON shape.
- **Happened:** `pub type ErrorView = { error: String, … }` is `V0001 expected }, found error`.
  The cascade then reported `problem` as having "no body" and taking "1 argument". No
  attribute renames a field for JSON.
- **Got past it:** I named the field `code`. The API's error bodies are
  `{"code": "...", "message": "..."}`, and a comment in `src/api.sak` explains why.

## 2026-10-21: importing names from a package module does not bind the module

- **Expected:** After `import repair_queue/store.{Ticket, …}`, I expected to be able to
  call `store.create(…)`.
- **Happened:** `V1002 cannot find store in this scope`. For `policy.capacity()` the help
  line suggested `Add import std/retry.{policy}`, an unrelated std function with the same
  name. Accepting that fix would have compiled into something wrong.
- **Got past it:** I wrote two lines per module, `import repair_queue/store` and
  `import repair_queue/store.{…}`. `sakura fmt` accepts this.

## 2026-10-21: no unused-import diagnostic

- **Expected:** A warning for unused imports from `sakura check`.
- **Happened:** `check` printed `no problems` while `Decision`, `InProgress`, `CreateBody`
  and `StateBody` were imported but unused.
- **Got past it:** I removed them by reading the import lists by hand.

## 2026-10-21: no helper for a JSON response with a non-200 status

- **Expected:** A helper like `Response.json_status(201, value)`, since `ok_json` is
  always 200 and `Response.status(code, body)` takes text without a JSON content type.
- **Happened:** No such helper exists on 0.1.0.
- **Got past it:** `Response { ..Response.ok_json(value), status: Status.from(code) }`
  inside a small `json_status` function with a `requires` range check.

## 2026-10-21: producing a synthetic cassette had no public recipe

- **Expected:** A documented way to make the "synthetic HTTP request/response cassette"
  the brief asks for, without a live server.
- **Happened:** The cassette guide says to record with `--record` through an outer `Http`
  handler but gives no exact line format beyond field names. Two snags:
  - `sakura test --record` refuses to start without a manifest `net` entry, even though my
    outer handler was in-process and made no network call.
  - Node printed `ExperimentalWarning: The flag --allow-net is under experimental phase`
    on stderr.
- **Got past it:**
  - I wrote a throwaway test whose outer handler was
    `handler Http { fn send(req) { Router.handle(app(), req) } … }` over `Sql.memory()`.
  - I ran `sakura test --record --filter record` once and then deleted the test. This is
    documented in `tests/fixtures/README.md`.
  - Writing a user handler for a std effect, including `Sql` with its generic `query` op,
    worked without trouble. That was a pleasant surprise, and test 25's broken database
    relies on it.
  - Under `--record` my filter also matched the replay-vs-router test, which failed
    because the cassette was still empty. That was my mistake, not the tool's.

## 2026-10-21: a host-spelling mismatch or a read-only grant passes the startup check

- **Expected:** These misconfigurations would be refused at startup with exit 75, like an
  empty `net` or `fs.read` list:
  - a manifest granting `net = ["localhost:8787"]` while the program listens on
    `127.0.0.1:8787`;
  - a manifest granting `fs.read` but not `fs.write`.
- **Happened:** Both start and then fail with exit 1. The first fails at `Serve.listen`
  with `ServeError { reason: "127.0.0.1:8787 is not in the manifest's net list" }`, after
  the database was already opened and migrated. The second fails at the first SQLite open
  with `SqlError { message: "repair-queue.sqlite: not in the manifest's fs.write list" }`.
  This matches the manifest docs, which only promise 75 for empty lists, but the brief's
  "same host spelling" rule is enforced late.
- **Got past it:** `integration.sh` asserts the exit codes actually observed: 75, 75, 1
  and 1. It also asserts that no listener is left running.

## 2026-10-21: no in-language way to test persistence or the socket (G03/G05)

- **Expected:** Per the brief, ordinary `sakura test` has no host permissions. So
  `Sql.memory` tests cannot show that the SQLite file persists or that the listener
  works, and neither the website nor the CLI gives a recipe for that.
- **Happened:** I confirmed there is no `sakura test` mode with host grants.
  `sakura run` starts a node child (`target/sakura/js/__main__.js`), and stopping the
  server means signalling that child.
- **Got past it:** I wrote `integration.sh`:
  - copies the project under `target/integration`;
  - starts `sakura run`;
  - creates and transitions a ticket with curl, then checks the documented status codes;
  - stops the server by the PID that `lsof` reports for the 8787 listener;
  - restarts and reads the ticket back;
  - asserts that the second start applied 0 migration steps;
  - checks the refused-grant cases.

  This runs on the installed binary but is not a `sakura test`.

## 2026-10-21: the brief's release and migration steps do not fit this session

- **Expected:** The brief names `v0.1.0-alpha.1` as the starting release and asks for a
  1.0.0-beta.1 migration from Astra's `MIGRATION.md`.
- **Happened:** The installed binary is `0.1.0`, not alpha.1. There is no `MIGRATION.md`
  in my directory, and I had no beta binary.
- **Got past it:** I built and tested against 0.1.0 only and recorded its exact version
  in `src/main.sak`. No migration was attempted, and none is claimed. I did not set up
  CI, so the js and wasm results are local runs only.
