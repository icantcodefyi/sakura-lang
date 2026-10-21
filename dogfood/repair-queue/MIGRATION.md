# Migration: repair-queue, v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.

## Prediction (written before the upgrade)

The program is a `Serve` + `Router` + `Sql` service. The service reads bodies only through
`Request.json(req, limit)` and answers with `Response.ok_json` and a record update of it.
The tests drive the router in memory, read response bodies as strings, build one request
with a `body:` field, and replay a client cassette (`Http.recording`) whose bodies they
decode. A user `handler Sql` (`broken_db`) fakes failures. `integration.sh` runs the real
listener with `sakura run` and curl.

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | **Yes** | (a) `acceptance.sak` `put`: `Request { ..Request.of(Put, …), body }` writes a `String` into `body` → `body: Text(body)`. (b) `ticket`, `problem`, `listing`: `Json.decode(r.body)?` → `Response.text(r)` (or `Response.json`). (c) test 25: `String.contains(r.body, …)` ×3 → `Response.text(r)`. (d) test 22: `route.body != unknown.body` compares two `Payload`s; probably still compiles, if `Payload` has equality. (e) `cassette.sak`: `r.body` / `x.body` read as strings in four places, "a test that compared a replayed `response.body` with a string compares `Response.bytes` or `Response.text`". Unchanged by the list: `Request.post`, `Request.post_json`, `Request.json`, `Response.ok_json`, `Response.status`. |
| 2 `Method` arms | No | `Get`, `Put`, `Delete` are constructed, never matched |
| 3 `redirect` | No edit | no 3xx anywhere; the client cassette is all 2xx/4xx |
| 4 `Clock`/`Task` operations | No | the one user handler is `handler Sql`, which the list does not name |
| 5 tier 1b crypto | No | no `std/crypto` |
| 6 `--json` closing kinds | No | `integration.sh` reads no `sakura … --json` output |
| 7 `Router` patterns | No edit | routes are literal segments and `:id`; no `*`, `?`, `+`, `(`, `{`, `\` |
| 8 `Path` with `\` or `:` | No | `Path.of("repair-queue.sqlite")`, `client-session.jsonl` |
| 9 tier 2 crypto | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | No code; script optional | `integration.sh` wraps `sakura` in `env -u ANTHROPIC_BASE_URL -u ANTHROPIC_AUTH_TOKEN`, a guard the program never needed |
| 11 log fields as JSON | No | no `Log` |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No | 400 messages are the program's own text, not `Violation.clause` |
| 14 `W1003 unused_import` | Check | the author removed unused imports by hand on 0.1.0 (FRICTION.md); some may remain |

Predicted diff: item 1 hunks (a), (b), (c), (e), possibly (d), all in tests.

## Result (after the upgrade)

Unchanged source on beta.1: `sakura check` fails with 15 `V2001` errors, every one at a
site item 1 predicted: `acceptance.sak` 35 (a), 42/44/46 (b), 447–449 (c); `cassette.sak`
31/33/35/46/48/49/52/53 (e). (d) `route.body != unknown.body` compiles unchanged, since
`Payload` has equality.

After the upgrade: `sakura check` no problems; `sakura fmt --check` clean; `sakura test`
32/32 on js and 32/32 on `--target wasm`; `sakura build` and `sakura build --target wasm`
succeed; `./integration.sh` with the beta first on `PATH` passes every check: the ticket
survives a restart on the same SQLite file, the 404/405/400/409/200 matrix holds over
curl, and the four missing-grant runs are refused with 75, 75, 1 and 1.

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `client.sak`: new `pub fn body_text(r: Response) -> String`, a `match` on `Payload.Empty \| Payload.Text(s) \| Payload.Bytes(b)` | Yes (one helper instead of 14 inline matches) | Yes, item 1 ("a read of `r.body` becomes a `match` on `Empty \| Text(s) \| Bytes(b)`") |
| `client.sak`, `acceptance.sak`: import `Payload` from `std/request` | Yes | **Partly.** Item 1 writes `body: Text("s")` and `Empty \| Text(s) \| Bytes(b)` unqualified; as written they fail `V1002 cannot find \`Text\` in this scope; did you mean \`Test\`?` and `V1007 \`Bytes\` is the builtin type \`Bytes\`, not a value`. The list names neither the import nor the qualification. |
| `acceptance.sak` `put`: `body` → `body: Payload.Text(body)` | Yes | Yes, item 1 (a) |
| `acceptance.sak` `ticket`/`problem`/`listing`: `Json.decode(body_text(r))?` | Yes | Yes, item 1 (b) |
| `acceptance.sak` test 25: `String.contains(body_text(r), …)` ×3 | Yes | Yes, item 1 (c) |
| `acceptance.sak`: import `repair_queue/client.{body_text}` | Yes | Follows from item 1 |
| `cassette.sak`: `body_text` in the 7 replay and router reads | Yes | Yes, item 1 (e) |
| `src/main.sak` header provenance line; `integration.sh` usage comment names the beta | No | n/a |

Why not `Response.text`/`Response.json`, which item 1 also offers: their beta signatures are
`text(r: Response, limit: Int) -> Untrusted<String> ! {Fail<HttpError>}` and
`json<T>(r: Response, limit: Int) -> T ! {Fail<DecodeError | HttpError>}`. The list writes
`Response.text` and `Response.json` with no arguments beside `Response.bytes(r, limit)`, and
does not say `text` returns `Untrusted<String>` (releasing it needs `Untrusted.trust(u, reason)`).
For test reads of bodies the program built itself, the `match` is the shorter migration.

**Migration-list gaps (wording, not missing items):**

1. Item 1's `Text("s")` / `Empty | Text(s) | Bytes(b)` do not compile as written: the variants
   are not in scope without `import std/request.{Payload}` and `Payload.` qualification, and
   `Bytes(b)` cannot be unqualified at all because `Bytes` is the builtin type. Evidence:
   `V1002` at `acceptance.sak:36` and `client.sak:33-34`, `V1007` at `client.sak:35` on the
   first attempt.
2. Item 1 lists `Response.text` and `Response.json` without their `limit` argument and
   without `text`'s `Untrusted<String>` result, so a mechanical `r.body` → `Response.text(r)`
   does not compile.

Listed items checked and not needed: 3, 7 (routes are literal and `:id`), 10 (`integration.sh`'s
`env -u` stays; it also strips `ANTHROPIC_AUTH_TOKEN`), 14.
