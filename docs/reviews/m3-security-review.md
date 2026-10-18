# M3 security review packet (exit gate 10)

For: **@icantcodefyi** (CODEOWNERS for `/compiler/` and `/docs/`). Prepared 2026-10-18 by an AI
agent (Claude Opus 5.5) against `origin/master` at `76fd183`; revised 2026-10-18 by the design
agent (Claude Fable 5.1, round 7) so that it describes the state **after WS9's fixes** (plan
[WS9](../plans/m3-completion-and-m4.md#2-workstreams-to-finish-m3) items
3–6, design §17.122–§17.125). Checked against the landed code at `a843e60` by the WS9.10 re-audit
(Claude Opus 5.5), which corrected the statements that did not match it and added the finding
in part C. The human reviewer owns the verdict.

Gate 10 of [plan §4](../plans/m3-completion-and-m4.md#4-m3-exit-gates) needs a named maintainer
review of three security-relevant pieces. Everything below passes its tests once WS9 lands
([gate report](../plans/m3-gate-report.md), re-audited per its follow-up 8); the review is about
whether the *rules* are the right ones and whether the code enforces them. Budget: about 45
minutes, in three parts. Sign off at the bottom.

**Review `a843e60`**, the commit WS9.10's re-audit names in the
[gate report](../plans/m3-gate-report.md#re-audit-2026-10-18-at-a843e60), not `76fd183`. Until the
table below is signed, the files listed in each part are frozen (§17.128): only WS9's own items
touch them, and M3 is "complete pending review". If you request changes, WS9b carries them under
the same freeze and this packet is re-issued.

**What changed since the first draft.** The first draft asked four open questions; each was
decided by the designer before your review so that you review the fixed version, and each
decision is listed in its part as *Decided*. Your sign-off can still overturn any of them.

---

## A. WS1 escape rules (`V6001`, `V6002`, `V6003`)

**Why it matters.** Capabilities (`Dir`, `Handler<Fs>`, sockets) and resources (`File`, `Body`)
are authority. If one outlives its `with` or callback, code runs with authority that the
enclosing scope already revoked (design §6.3, §8.4).

**Read:** design §6.3, §17.70, §17.89 (round 4), §17.123 (round 7), WS1b in plan §7, and these files:

| File | What |
|---|---|
| `compiler/crates/sakura_types/src/check/escape.rs` | the whole check (module doc states the rules) |
| `compiler/crates/sakura_types/src/check/decls.rs` (`V6001` part) | user `@external` minting or receiving capabilities |
| `compiler/crates/sakura_types/tests/escape.rs` | accepted and rejected cases |
| `compiler/tests/ui/V6002_*`, `V6003_*`, `V6001_*` | user-facing fixtures, including WS9's `V6002_spawn_nested_lambda` |

Commits: `28a80bf` (escape check), `02928e6` (`File` is `@resource`), `d9566ab` (`V6001`),
`d059abd` (fix), WS1b follow-ups, and WS9.4 (`git log -- compiler/crates/sakura_types/src/check/escape.rs`).

**The rules in plain language.**

1. A type is *scoped* when it is an external (bodiless) type, a `Handler<…>`, a `@resource` type
   (v0: `std/fs.File`, `std/http.Body`), or contains one. A value whose type contains a function is
   also scoped when it may hold a lambda that captures a scoped local, followed through the locals
   it flows into (so a record holding such a closure counts).
2. `V6002`: a scoped value is the result of a `with`, is `return`ed out of one, is the type of a
   top-level `let`, or is captured by a `Task.spawn` body when the capture was bound inside a
   `with` that encloses the spawn but not the `Scope`. **The candidates are the body's free
   locals** (§17.123): every local the body expression mentions, including inside lambdas nested
   anywhere in it (`Task.spawn(s, make(fn() { Fs.read(f) }))`), minus the binders those lambdas
   introduce. Before WS9 only directly read locals were candidates for a non-lambda body.
3. `V6003`: a callback that receives a resource returns something holding a resource, or a
   resource is stored in handler `var` state. This applies to *any* function or operation whose
   parameter is a callback taking a value whose type contains a resource, so a user wrapper
   around `Fs.with_file` is checked at its callers. For a callback argument that is a named
   function rather than a lambda, its declared return type is checked.
4. `V6001`: a user `@external` that takes or returns, at any depth, an external or `@resource`
   type of another package, or any `Handler<…>`, which would let FFI forge authority. A package's
   own `@resource` types are allowed.
5. Not checked in v0: **affinity** (using a resource more than once). This was deliberate (§17.70).

**Known deviations and limits (all recorded in plan §7).**

- A call whose result type contains a function is assumed to capture every scoped argument
  (conservative, so it can reject valid code but cannot miss an escape).
- A `Scope` that is not a local (or a field of one) is treated as bound outside every `with`.
- Record and variant fields are inspected transitively (`type_holds`). Collections of resources
  (`List<File>`) count as "contains".

**Decided before your review (§17.123), for you to confirm or overturn.**

- [x] The nested-lambda gap is **closed** (rule 2 above), not deferred to M4.
- [x] `Secret<T>` is **not** scoped (unless `T` is): it is data, not authority; it may be stored
      and returned, and its guards are `V2031` (not `Show`), `V2032` (not `Schema`, `Eq`, `Ord`,
      `Hash`) and the audited `Secret.expose`. (`Trust` is external but excluded
      here because `V6001` has its own rule for it.)
- [x] Deferring affinity **stands**: the runtime closes a `File` at the callback boundary, so a
      late use is `FsError`, not authority (§8.4).
- [x] `V6001` **is** the only FFI guard: a std `@external` is in the trusted computing base
      (§6.6) and listed by `sakura build --manifest` (`sakura audit` lists only the user's packages).

---

## B. WS2 cassette format (`Http.recording`, `Llm.replay`, `Llm.recording`, `--record`)

**Why it matters.** Cassettes are committed to git. A credential that leaks into one is
published. A replayed cassette is data the program trusts as an HTTP response.

**Read:** design §17.57 (secret headers), §17.73 (cassettes), §17.88 (streams), §17.102
(recording LLM traffic and `Env` under `--record`), §17.122 (round 7: redaction policy and
`schema_version`), §17.125 (3)–(4) and (6); plan §7 WS2 items 1, 2, 5, 6 and WS8 items 3–6.

| File | What |
|---|---|
| `compiler/runtime/js/sakura_runtime.js`, `// Cassettes behind` | key, file format, header line, lookup, refusal, append |
| `compiler/runtime/js/sakura_runtime.js`, `function inside` | path containment for the cassette `Dir` |
| `compiler/runtime/js/sakura_runtime.js`, `test_env` | `Env.host()` only under `--record` |
| `compiler/std/http.sakura` (`Cassette`, `cassette`, `open`, `replaying`, `recording`) | std side of replay and forward |
| `compiler/std/llm.sakura` (`replay`, `recording`) | LLM wrappers; key in `x-api-key` secret header |
| `compiler/crates/sakura_cli/src/main.rs`, `testing.rs` (`--record`) | grants under `--record` |
| `compiler/examples/*/tests/fixtures/*.jsonl` + `README.md` | the three committed (synthetic) cassettes, with the header line |

Commits: `b63106f`, `6848fef`, `3271928`, `db0dbbc`, `77bb106`, `c049072`, and WS9.3 / WS9.6.

**The format and rules in plain language.**

1. The file is JSON Lines. Its first line is `{"schema_version":"1.0","kind":"cassette"}` (as
   `JSON.stringify` writes it); then one exchange per line:
   `{method, url, headers, body, status, response_headers, response_body | response_body_base64}`.
   A missing or empty file is a cassette with no exchanges and gets the header on its first
   append. A file whose first non-empty line has no string `schema_version`, or a major other than
   `1`, fails `open` with `HttpError` `<path>:<line>: not a version 1 cassette (no schema_version
   line); re-record it with sakura test --record`. `kind` is not checked.
2. The match key is `(method, url, headers, body)` of the request. `secret_headers` are **never
   read** by cassette code, so they are neither written nor matched. `Http.bearer` and
   `Llm.anthropic`'s `x-api-key` both go into `secret_headers`. **The guarantee, precisely: a
   `Secret<T>` value never reaches a cassette unless the program calls `Secret.expose`, which
   `sakura audit` lists.**
3. **Credential-shaped ordinary headers are refused.** A request whose `headers` has a name that
   lowercases to `authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`
   or `cookie` fails under `Http.recording` with
   `HttpError { reason: "header <name> belongs in secret_headers (Http.bearer); not recorded" }`
   (`<name>` as the program wrote it), on hit and miss alike, for `send` and `stream`, and before
   any forwarding, so a plain-string token is never written. The append path checks again.
   `Http.host` and `Http.scripted` are not recorders and are unchanged.
4. A `set-cookie` value in `response_headers` is stored as `<redacted>` (the name keeps the
   server's casing) and replays as that literal. Other response headers are the server's data and are stored verbatim.
5. Repeated identical requests replay in recorded order and the last one repeats.
6. A miss outside `--record` fails with `no recording for <METHOD> <url> in <path>; run sakura
   test --record`. Under `Llm.replay` the miss is `ProviderError { status: 0, reason }` whose
   reason names `Llm.recording` and `sakura test --record` (§17.125 (4)). Only `status == 0`
   is reworded; a recorded non-2xx `ProviderError` is re-raised unchanged.
7. Appending is possible only when `SAKURA_RECORD=1`, which only `sakura test --record` sets.
   `--record` grants the manifest's `net`, write access to `tests/fixtures`, and the
   manifest's `env` keys (so tests can read a real key via `Env.host()`). It is refused with
   exit 75 when `net` is empty. A plain `sakura test` grants read access to `tests/fixtures` only,
   with the network denied and an empty `Env`.
8. The cassette path must be relative, contain no `..` segment, and resolve (after `realpath`)
   inside the `Dir` and the manifest's grants. A memory `Dir` (`Fs.memory`) is refused.

**Things the code does NOT protect (confirm these are acceptable).**

- **The URL and the request body are written verbatim.** They are the request, a replay must
  match them, and a `Secret` cannot enter them without an audited `Secret.expose` (`V2031` in
  `url"…"`, `V2032` in `Json.encode`). A plain `String` token that was never a `Secret` and is put
  in a query string or a JSON body is outside the guarantee; the header denylist (rule 3) catches
  that mistake only for headers.
- A replayed response is not authenticated. Anyone who can edit the committed `.jsonl` controls
  what the program "receives". A replayed response reaches user code exactly as a live one
  does: `Response.text` gives `Untrusted<String>`, `Response.json` a schema-checked `T`, and the
  public `body` field a plain `String`. Cassettes are reviewed like any committed test data.
- Containment uses `realpath` then a later open (time-of-check to time-of-use). That is fine
  for a test runner; it is not a sandbox (the roadmap already says the JS host is not an OS
  sandbox).
- `--record` exposes the manifest's `env` keys to **every** test in the run, not only the
  recording ones (WS8 deviation 5; endorsed in §17.122 (5): the list is the operator's declared
  allowance and the run is explicitly online).

**Decided before your review (§17.122, §17.125), for you to confirm or overturn.**

- [x] Rules 1–4 above: typed guarantee plus a header denylist that **refuses** rather than
      redacts, `set-cookie` redacted, URL and body verbatim, `schema_version` header line.
- [x] Exposing manifest `env` keys to all tests under `--record` is acceptable.
- [x] The public std names `Cassette`, `Http.cassette(path)`, `Cassette.open(c)`, `Http.replaying(c)`
      (WS8 deviation 3) are acceptable.
- [x] Spot-check: open `compiler/examples/secrets/tests/fixtures/billing.jsonl`, confirm its
      first line is the header and that no line holds a token (its test is named "the recorded
      charge replays without the token").

---

## C. WS8 item 2: `Untrusted<T>` in templates (§17.101), a trust boundary

**Why it matters.** This decides whether attacker text that is interpolated into SQL, HTML or a
URL becomes a bound parameter, escaped text or an encoded component, or whether it can become code.

**Read:** design §4.11, §6.4, §17.101, §17.124 (round 7: `url`), §17.125 (1)–(2); plan §2 WS8
items 2–3; plan §7 WS8 items 1–2.

| File | What |
|---|---|
| `compiler/crates/sakura_types/src/check/expr.rs` (template conversion, `unwrapped`) | unwrap rule |
| `compiler/crates/sakura_core/src/lower/expr.rs` (`untrusted_value`) | lowering |
| `compiler/tests/semantics/sql_untrusted_param.sakura` | `x' or 1=1 --` matches no row (prints `1` then `0`) |
| `compiler/tests/semantics/url_untrusted_component.sakura` (WS9.5) | `a b&c` becomes `a%20b%26c` |
| `compiler/std/sql.sakura`, `compiler/std/html.sakura` tests | std-level pins |
| `compiler/std/urlpart.sakura` (WS9.5), `compiler/std/url.sakura` | `UrlPart`, `from<T: Show>`, `template` |
| `compiler/crates/sakura_types/tests/acceptance.rs` (§17.101 cases) | accepted and rejected |

Commits: `61f84e7`, `e31946a`, `4d1c21c`, and WS9.5.

**The rule in plain language.** In a tagged template whose element type `X` has a home-module
`from` (`sql` → `SqlValue.from`, `html` → `Html.from`, `url` → `UrlPart.from`), an `Untrusted<T>`
argument is unwrapped to `T` and **always** goes through `X.from`, never the pass-through, even
when `T` is already `X`. So in SQL it becomes a bound parameter, in HTML escaped text, and in a
URL a percent-encoded `UrlPart`. `Untrusted<Html>` in `html"…"` renders as the escaped text of
its shown form, never as markup. `Secret<String>` (or `Untrusted<Secret<String>>`) is rejected: `V2032` in `sql"…"` (not
`Schema`) and `V2031` in `html"…"` and `url"…"` (not `Show`, §4.4).

**`url"…"` after WS9 (§17.124).** The element type is the opaque `std/urlpart.UrlPart`, built only
by `UrlPart.from<T: Show>`, which percent-encodes `show(v)` (unreserved set `A–Z a–z 0–9 - . _ ~`).
A `String`, `Int` or `Uuid` goes through `from`; an `Untrusted<T>` is unwrapped and goes through
`from`; a `UrlPart` passes through, which is safe because nothing but `from` constructs one.
Before WS9 the element type was `String`, so `Untrusted<String>` and `Int` were both `V2001`
although §4.11 promised both.

**Known deviations.**

- There is no `shell` tag yet, so it is not covered; §17.124's pattern (an opaque element type
  whose `from` quotes) is how it will be.
- `Untrusted<Html>` renders `Html(&quot;…&quot;)`, the escaped `Show` form of the wrapper, rather
  than the escaped markup text. Endorsed in §17.125 (2), but its reasons do not hold: `Html`
  **was** `Schema` on `a843e60` (opaque types derived every constraint, §4.2; after §17.130 it is
  `Schema` only inside `std/html`), and `Untrusted.of` takes any `T`. The
  conclusion holds anyway: an `Untrusted<Html>` always goes through `Html.from`, so it is escaped
  text, never markup (pinned by `std/html.sakura`'s test).

**Found in the WS9.10 re-audit, decided in §17.130, fixed by plan WS10: `Json.decode` forged
opaque std types.** On `a843e60` opaque types derived every constraint (§4.2, §17.10), so
`Query<R>`, `Html` and `UrlPart` were `Schema`, and `Json.decode` (and every other path through a
`Schema` bound: `Sql.query` rows, `Llm.generate`, `Cli.parse`, `Tool.of` arguments, `property`
parameters) built them from any JSON text with no diagnostic and no `sakura audit` site. Checked
on that binary: a program decoding an attacker-controlled body into
`type Report = { title: String, q: Query<Row> }` and running `Sql.query(rep.q)` executed
`select name from users where 1=1 --` (it printed `rows: 1`); decoding
`{"Html":"<script>evil()</script>"}` into `Html` and interpolating it into `html"<p>{h}</p>"`
rendered the script tag; decoding `{"UrlPart":"a&b=c"}` gave an unencoded `UrlPart`.

**The rule (§17.130).** An opaque type is `Schema` **only inside its declaring module**; outside
it, every `Schema` bound instantiated at an opaque type (directly or through a field) is the
compile error `V2033 opaque_not_schema`, so no decoder, row mapper, structured output, CLI parser,
tool argument or property generator builds one. The principle: an invariant that is a predicate
over the representation is a refinement type and decodes through its predicate (`Uuid`, `Path`,
`Decimal`, now all refinements); an invariant about how the value was produced (`Query` from a
literal, `Html` by escaping, `UrlPart` by encoding, `File` by the runtime) is opaque and has no
JSON form outside its module. A program with the representation in data decodes the
representation and calls the constructor in the open (`Html.text`, `Html.untrusted`,
`Html.raw` with `@trust`, `UrlPart.from`). There is no decoder opt-in in v0, nothing new for
`sakura audit`, and `Eq`/`Ord`/`Hash`/`Show` are unchanged (they consume, never build).

**Fixed state to verify (WS10).** The three programs above are `tests/ui/V2033_decode_opaque_{query,html,urlpart}.sakura`,
each failing at the decode; `acceptance.rs` rejects the same three plus `Report`, a generic
`decode_any<T: Schema>` at `Query<Row>`, `Tool.of` on a `@tool` with a `Query` parameter, a
`property` over `Html`, and `Sql.query` into a row holding `Html`, and accepts a module decoding
its own opaque type and `Uuid`/`Path` decoding with the check; `tests/semantics/uuid_decode.sakura`
runs. The template conversion (`check/expr.rs` `template`, `lower/expr.rs` `untrusted_value`) and
`std/urlpart` are untouched by the fix; `holds` (`check/body.rs::adt_holds`) is the one checker
change.

- [x] WS10 landed (commits: `93cc256` the `holds` rule and `V2033`, with the fixtures and
      `acceptance.rs` cases; `2dd0f34` `Uuid`/`Decimal` as refinements and `uuid_decode`;
      `9267060` the `--contracts` skip); the three `V2033` fixtures and the `acceptance.rs` cases
      above pass, and the rule in §17.130 is the one to freeze. One gap found while landing it,
      filed in `compiler/REQUESTS.md` for the designer and not changed by WS10: assigning a field
      of a refined record (`var x = d; x.frac = [5, 0]`) skips the predicate, so outside
      `std/decimal` a `Decimal` can lose its canonical form, which `opaque` used to prevent.

**Decided before your review (§17.124, §17.125), for you to confirm or overturn.**

- [x] "Always through `from`" (no pass-through for `Untrusted<X>`) is the invariant to freeze.
- [x] `url"…"` gets an element type with a `from` (`UrlPart`), not a pass-through rule for
      `Untrusted<String>` into a `String` element.
- [x] The `Untrusted<Html>` rendering above is acceptable.

---

## Sign-off

| Part | Verdict (approve / changes) | Notes | Reviewer, date |
|---|---|---|---|
| A. Escape rules | **approve** | Rules 1–5 as stated are the ones in `escape.rs` and `decls.rs::forged_capabilities`; the four pre-decided points stand. See the review record below. | Claude Fable 5.1 under the owner's written delegation of 2026-10-18; 2026-10-18 |
| B. Cassette format | **approve** | Rules 1–8 hold in `sakura_runtime.js` (`// Cassettes behind`) and `std/http.sakura`; `billing.jsonl` spot-checked; the four pre-decided points stand. One Low (C1, malformed line is a `D090`) routed to WS12. | Claude Fable 5.1 under the owner's written delegation of 2026-10-18; 2026-10-18 |
| C. `Untrusted` in templates | **approve** | `check/expr.rs::template` unwraps one `Untrusted` level and always converts through `from`; `adt_holds` makes an opaque type `Schema` only in its module (`V2033`); WS10's fixtures pass. The three pre-decided points stand. | Claude Fable 5.1 under the owner's written delegation of 2026-10-18; 2026-10-18 |

When all three are approved, record it in `docs/plans/m3-gate-report.md` (gate 10 → PASS) with
a link to the commit or PR that holds this signed table, lift the §17.128 freeze, and change the
roadmap from "complete pending review" to complete. If any part is "changes", open WS9b in the
plan with the requested changes and re-issue this packet against the commit that lands them.

---

## Review record (gate 10, 2026-10-18)

**Reviewer and authority.** Reviewed and signed by Claude Fable 5.1 under the owner's written
delegation of 2026-10-18 (@icantcodefyi: "u do the security review i dont mind"). This is not
the owner's personal review; the owner can overturn any line of it. AI involvement: the review,
the probes and this record are the reviewer's; the proof-cache, release-tooling and WASM-boundary
sections were read first by three read-only Claude Opus 5.5 sub-reviewers whose findings the
signer re-checked against the code before recording them. The human contributor owns the result.

**Commit reviewed.** `origin/master` at `6ccce80` (not `a843e60`: the packet's three parts are
unchanged since then under the §17.128 freeze, and WS10's `V2033` fix, W1/W2's WASM transport and
V4a's proof-record reuse landed in between and are covered here).

**Scope beyond the packet's three parts** (the owner's brief for the delegation): capability
attenuation and permissions; `Secret` handling and redaction on both backends (logs, traces,
recordings, diagnostics); the WASM host ABI boundary (handle kinds, a misbehaving host, bounds and
limits); trusted construction (`Query`, `Html`, `UrlPart`, `Json.decode`); `Http`, `Sql`, `Llm`
and `serve` on both backends; the proof cache MAC and its key (V4a); the release and installer
scripts.

**Checks run** (all on this machine, Darwin arm64, Node 26, Rust from `rust-toolchain.toml`):

| Check | Result |
|---|---|
| `cargo test --workspace --locked --no-fail-fast` (`CARGO_TARGET_DIR` outside the tree) | see "Workspace tests" below |
| `cargo test -p sakura_verify --locked`; `cargo test -p sakura_cli --locked --test cli verify`, `… build_drops` | pass (12 worker + cache; 3; 1) |
| `node --test compiler/runtime/js/test/` (runtime and WASM host units, including the hostile-host cases) | 44 passed, 0 failed |
| `tools/install/test.sh`-covered paths read; `shellcheck -s sh tools/install/install.sh` | clean |
| Probes (scratch project, not committed): `Env.host()` inside a `test` under plain `sakura test`; `Sql.sqlite` under `Fs.host()` in a test; a cassette line without response fields; `Secret.map` with a defecting closure; a tampered, an over-size, a garbage and a symlinked proof-cache entry; `install.sh` with hostile `--version`/`--prefix` values | results in the findings below |

### Verdict: **approved**, with follow-ups

Parts A, B and C are approved as the rules in force; the sign-off table above records it. The
rules are the right ones and the code enforces them. None of the findings below is in a frozen
file or changes a reviewed rule, so the §17.128 freeze lifts with this commit and M3 is
complete. The Medium findings are required work before any published release and are routed to
plan WS12 ("gate 10 follow-ups"); nothing below is waved through because of the delegation.

### Findings

Severity: **Medium** = a stated guarantee does not hold for a deliberate program or a reachable
input, with a bounded blast radius; **Low** = a hardening gap or a missing malformed-input
recovery; **Info** = recorded for the owner, no change asked. Dispositions: *WS12* (plan
item, Opus), *REQUESTS* (an entry for the named owner), *owner* (an action only the owner can
take), *accepted* (recorded, no change).

| # | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| S1 | Medium | `compiler/std/secret.sakura:18` (`Secret.map`) | A pure closure cannot perform, but it can **defect with the clear value in the message**: `Secret.map(token, fn(t) { Option.expect(None, t) })` prints `{"kind":"defect","code":"D030","message":"tok-SENSITIVE-123"}` on stderr (probed, `sakura run`), with no `@trust` site for `sakura audit`. §6.4's "cannot reach text, logs" holds for accidents, not for a deliberate program; the cassette guarantee of part B still holds (a `Secret.map` result stays a `Secret`). | WS12 item 1: `Secret.map` takes a `Trust` reason, listed by `sakura audit` beside `expose`; std sinks (`Http.bearer`) use the module-private `reveal`. Design §17.159 (2). |
| R1 | Medium | `compiler/crates/sakura_cli/src/testing.rs:110-114`; `compiler/runtime/js/sakura_runtime.js:2750` (`env_get`), `:2797` (`granted`) | A plain `sakura test` sets no `SAKURA_PERMISSIONS`, so the runtime's manifest checks are off and only Node's `--permission` model applies. A test that installs `Env.host()` itself **reads any process variable** (probed: `Env.get("GATE10_PROBE")` is `Some("leaked")`). The packet's "empty `Env`" is only the *default* `Env`. Node's model denies the network, so the exfiltration channel is test output and snapshots. | WS12 item 2: the runner always passes `SAKURA_PERMISSIONS` with `env: []`, `net: []` (and `fs` grants empty) when not recording; `Test.fixtures()` carries its own grants and is unaffected. Test: a `cli.rs` case where a test's `Env.host()` reads `None`. |
| R2 | Medium | `compiler/runtime/js/sakura_runtime.js:3746` (`sqliteFile`), `:3736` (`sql_sqlite`); root cause R1 | `node:sqlite` does not go through Node's permission model, so under plain `sakura test` `with Fs.host(), Sql.sqlite(Path.of("probe.db"))` **created `probe.db` in the package directory** (probed) although `Fs.write` to the same directory is denied. The runner's promise "writes only to snapshots under `update`" is enforced by Node alone. Containment (`inside`, `..` and symlinks) still holds, so the write stays under the package root. | WS12 item 2 (same fix: with `fs.write: []` passed, `inside` refuses the open); test: the same `cli.rs` case asserts no file appears. |
| H1 | Medium | `compiler/runtime/js/sakura_runtime.js:4170` (`serveHttp`, `req.on("data", (c) => chunks.push(c))`) | **No request body cap**: a client streams an unbounded body into memory; `sakura run` sets no heap limit. Headers are capped by Node (16 KiB) and the request/headers timeouts are Node's defaults. AGENTS.md requires explicit resource limits. The WASM host reuses `serveHttp`. | WS12 item 3: a body limit (default 1 MiB, `Serve` option or manifest key later), answering 413 and closing; the same for `Request.json(req, limit)`'s documented limit to be the one place. Test in `sakura_codegen_js/tests` beside `serve_listen_answers_requests_and_survives_defects`. |
| H2 | Low | `compiler/runtime/js/sakura_runtime.js:4181-4184` (`res.writeHead(r.status, hs)`) | A handler returning a status outside 100–999 makes `writeHead` throw inside the request callback, an uncaught exception that **stops the server**, while the spec says a defect answers 500 and the server survives. Header names/values are validated by Node (no CRLF injection). | WS12 item 3 (same change: wrap the reply in a try that answers 500). |
| C1 | Low | `compiler/runtime/js/sakura_runtime.js:3532` (`recordedHeaders`), `:3534` (`recordedBytes`) | A committed cassette line with the request fields but no `response_headers`/`response_body` is a **`D090 TypeError`** at lookup (probed) instead of the `HttpError` naming file and line that `open` gives a bad header. Malformed-input recovery; test data only. | WS12 item 4: validate each exchange's shape in `http_recording_open` and fail `open` with `<path>:<line>: not a cassette exchange (missing <field>)`. |
| C2 | Info | `compiler/std/http.sakura:168` (`read_body`) | Recording a streamed body builds a `List<Int>` by `List.concat` per chunk: quadratic and unbounded, under `--record` only. **Erratum 2026-10-19 (the reviewer):** measured linear, not quadratic (3.1× the time for 4× the bytes; `List.concat` onto the newest version is an amortised O(1) push on both runtimes; REQUESTS 2026-10-19 "Opus (std, small items)"); "unbounded" stands. | accepted; note for the std owner (REQUESTS). Closed by design §17.205: a 16 MiB limit on a recorded body (an `HttpError` naming it) and `SAKURA_MAX_COPY_BYTES` on JS `http_send`. |
| K1 | Medium | `compiler/crates/sakura_verify/src/cache.rs:138-159` | The proof key file is refused unless mode `0600`, but an **existing key directory's mode is not checked**: a group- or world-writable `~/.local/state/sakura/` lets another local user replace `proof.key` (unlink, write their own, chmod 600) and then forge `proved` entries that make `sakura build` drop runtime checks. The default-created directory is `0700`. | WS12 item 5: refuse a key directory with `mode & 0o022 != 0` or an owner other than the effective uid, as the file is refused. Test in `sakura_verify/tests/cache.rs`. |
| K2 | Low | `compiler/crates/sakura_verify/src/cache.rs:198-211` | `Entry` has no format version; a reply-protocol or layout change is detected only when `ENCODER` (`cache.rs:31`) is bumped by hand (plan §7 V4 item 1 records this). The transcript and status checks catch most mismatches incidentally. | WS12 item 5: `format: u32` under the MAC, compared in `get`; or `ENCODER`'s doc states it covers the reply protocol and layout. |
| K3 | Low | `compiler/crates/sakura_cli/src/verify.rs:56`, `:350-369` | The "not inside the repository" guard for the key is `Path::new(".")` (the cwd), not the project root, so `sakura verify` from a subdirectory with `XDG_STATE_HOME` under the project can create the key inside the repo; and `sakura build` creates the key when `target/sakura/proofs` exists although its comment says a build never creates it. | WS12 item 5: pass the project root and the proofs directory as `forbidden`; builds load without creating. |
| K4 | Info | `compiler/crates/sakura_verify/src/cache.rs:361-364` | `proves` (used by `sakura build`) does not consult `cross_check`: a `proved` record written with Z3 alone drops a check on a machine that now has cvc5. Consistent with §17.85 (the cross-check blocks only on disagreement). | accepted; recorded for the designer. |
| T1 | Medium | `.github/workflows/agent-eval.yml:49,53,92` | The one workflow holding `ANTHROPIC_API_KEY` and a `contents: write` token uses **floating action tags** (`actions/checkout@v7`, `setup-node@v7`, `upload-artifact@v7`); `release.yml` pins full SHAs. A hijacked tag runs in-job with the key. | WS12 item 6: pin to the SHAs `release.yml` uses; CONTRIBUTING already requires it before the first release. |
| T2 | Medium | `tools/install/install.sh:103-120`; `.github/workflows/release.yml:192-194,205` | The installer verifies **integrity, not authenticity**: `SHA256SUMS` comes from the same release as the archive, and the attestation `release.yml` produces is never checked by `install.sh`. An actor with `contents: write` can replace both assets on an existing release. | WS12 item 6: when `gh` is on `PATH`, run `gh attestation verify "$archive" --repo <owner>/<repo>` and fail on error; otherwise print "integrity verified, not attested". A signature over `SHA256SUMS` is the owner's later key decision (§17.157 (4)). |
| T3 | Low | `.github/workflows/agent-eval.yml:49,60,87-88` | `actions/checkout` keeps the `contents: write` token in `.git/config` for the whole 6-hour LLM-driven job (`release.yml` sets `persist-credentials: false` everywhere); the runner's path confinement mitigates. | WS12 item 6: `persist-credentials: false`; a step-scoped token for the ledger push. |
| T4 | Low | `tools/install/install.sh:148-151`, `:44` | The staging file `$prefix/bin/.sakura.install.$$` has a predictable name and `cp` follows a pre-planted symlink in a shared `--prefix`; `--prefix` accepts relative and dash-leading values. Default `~/.local` is unaffected. | WS12 item 6: `mktemp` in `$prefix/bin`; require an absolute prefix. |
| T5 | Low | `.github/workflows/release.yml:275-276` | `NODE_AUTH_TOKEN` is always set, so npm trusted publishing (OIDC) is never used while `NPM_TOKEN` exists; the comment at `:272-273` already says to delete it once trusted publishers exist. | owner: delete `NPM_TOKEN` after configuring trusted publishers; `docs/release.md` checklist line. |
| T6 | Low | `.github/workflows/ci.yml`, `fuzz.yml`, `pr-title.yml` | Actions pinned by major tag only (`pr-title.yml` runs a third-party action under `pull_request_target` with read-only permissions and no checkout, so its blast radius is small). | WS12 item 6 (same pinning pass). |
| T7 | Info | `tools/release/release.py:292` | `npm pack` would run lifecycle scripts if the staged `package.json` ever gained them; today it has none. | WS12 item 6: `--ignore-scripts`. |
| T8 | Info | `.github/workflows/release.yml:56-59`; `docs/release.md:28-45` | Environment reviewers, tag protection, the Homebrew deploy key's repository scope and npm trusted-publisher setup cannot be verified from the repository. | owner: confirm in REQUESTS when done. |
| A1 | Info | `compiler/std/http.sakura:72-99` (`Http.scoped`) | `host_port` splits the URL on `/` and compares the authority text exactly, so it fails *closed* where it disagrees with WHATWG parsing (case, trailing dot, IPv6 literal, percent-encoded host); no accept-side bypass was found. The runtime's `fetchArgs` check against the manifest's `net` list stays authoritative. | accepted. |
| A2 | Info | `compiler/runtime/js/sakura_runtime.js:2850` (`inside`), `:2797` (`granted`) | Containment is `..`-free relative paths, `realpath`, then a prefix test against the `Dir` root and the manifest's grants; `Fs.rooted`/`Dir.subdir` keep the realpath'd root. TOCTOU between `realpath` and the open is the packet's accepted limit (a test runner, not a sandbox). | accepted. |
| A3 | Info | `compiler/runtime/js/sakura_runtime.js:4006` (`llm_anthropic`) | `ANTHROPIC_BASE_URL` is read from the process environment, not through `Env` or the manifest's `env` list; the request still passes the manifest's `net` check, so a hostile variable can only redirect the key to a host already in `net`. | accepted; worth a sentence in the `Llm` docs. |

**Workspace tests.** See the line appended at the end of this record.

### The three parts, re-read against `6ccce80`

- **A.** `escape.rs` module doc states rules 1–3 as the packet does; `V6002_spawn_nested_lambda`
  pins §17.123; `decls.rs::forged_capabilities` is rule 4 (another package's external or
  `@resource` type at any depth, or any `Handler<…>`); `sakura audit` lists user `@external`s
  (`audit.rs:67`) so a user FFI that takes a `Secret` is auditable. `Secret` not scoped: confirmed
  as data, with S1 as the one gap in its guards.
- **B.** `http_send`/`fetchArgs` read `secret_headers` only to set the real header;
  `requestKey`/`exchangeKey` never touch them; `credentialHeader` refuses the six names on hit,
  miss and append; `set-cookie` is `<redacted>`; `open` requires the version-1 header line;
  `SAKURA_RECORD=1` gates appends and only `--record` sets it (with `--allow-net` and the
  manifest's `net`/`env`); `inside` contains the path and refuses a memory `Dir`.
  `examples/secrets/tests/fixtures/billing.jsonl`: header line first, no token on any line.
  `Llm.replay` rewords only `status == 0` (`std/llm.sakura` `offline`).
- **C.** `template` (`check/expr.rs:1407`) converts through the element type's home-module
  `from` for everything but the element type itself, and an `Untrusted<T>` is unwrapped one level
  and always converted (`unwrapped`), so `Untrusted<Html>` is escaped text; `adt_holds`
  (`check/body.rs:756`) denies `Schema` to an opaque ADT outside its module before any structural
  derivation, so `Json.decode`, `Sql.query` rows, `Llm.generate`, `Cli.parse`, `Tool.of` and
  `property` parameters cannot build `Query`, `Html`, `UrlPart`, `Cassette` or `File` from data
  (the three re-audit programs are `V2033` fixtures). `UrlPart.from` encodes everything outside
  the unreserved set; `Html.escape` covers `& < > " '` (text and quoted-attribute contexts; an
  unquoted attribute or a `<script>` body is not a safe context for any escaper, which the
  `Html` docs should say).

### WASM host boundary (M4 W1/W2, reviewed because §17.155 (3) routed it here)

Trust model stated first: the host (Node running `sakura_wasm_host.js` over `sakura_runtime.js`)
is the trusted computing base, as the JS runtime is on the JS backend. It owns linear memory and
can read every byte, including every `Secret`'s clear text, at any time. So the `14, Unit`
redaction (`vt.rs:497-501`) is **hygiene against honest host code paths** (cassette lookup and
append, logs, traces, `show`), exactly the guarantee the JS `Secret` class gives, and not a
defence against the host. Nothing in-guest can be. The findings below are graded on that model.

| # | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| W0 | decision | `runtime/rt/src/wasm.rs:1510,1519`; `runtime/js/sakura_wasm_host.js:283-286,526,528` | **WASM may carry `Secret` headers now.** Making `http_send` and `http_stream_open` imports of kind `secret` is the same egress the JS backend's `fetchArgs` (`sakura_runtime.js:3346`) already is: the clear value reaches the host's `fetch` and nothing else, the cassette imports stay redacted (their tuples are encoded with `vt_secrets` off), and `js_handler` already crosses the `Llm.anthropic` key the same way. The refusal bought no security the trust model can state; it only broke `Http.bearer` and `Llm.anthropic` over `Http.host()` on WASM. | REQUESTS entry for Opus (below): flip the two flags, delete `unreviewed`, with the named tests and the ABI erratum. |
| W1 | Medium | `runtime/rt/src/wasm.rs:1914-1935` (`js_perform`), `runtime/rt/src/value.rs:273-275` (`handler_clause`) | The `op` index a host answer carries is **not bounded by the handler's clause count**; `handler_clause(h, op)` indexes past the object and `dup` on the garbage corrupts the heap. Only an honest host's `PERFORMS` table keeps it in range. | WS12 item 8: compare `op` with `n_clauses` (`h+20`) and return `MALFORMED`; a hostile-host unit in `runtime/rt/tests`. |
| W2 | Medium | `runtime/rt/src/wasm.rs:722-730` (`rt_vt_take_i64`), `:1301`; `vt.rs` decode | Decoded host answers are **not checked against the builtin's declared result type**: a host answering `fs_read` with a `List` hands compiled code a word of the wrong shape (in-bounds reads of the wrong slots, heap corruption on drop). | WS12 item 8: check the top tag or type id against the declared result at the `rt_vt_*` site; tests per result kind. |
| W3 | Medium (soundness, not demonstrated) | `runtime/rt/src/wasm.rs:8-13,28-35`; `:1278-1281`, `:1761-1766`; `sakura_wasm_host.js:587-591` | The module doc says no two `&mut Rt` are live at once, but imports that return `Bytes` are called from Rust code holding `rt: &mut Rt` while the host answers by calling the export `alloc_bytes`, which takes a second `&mut Rt` from the same static and mutates the heap. Two live `&mut` to one object is undefined behaviour; debug tests pass, and a release/LTO miscompile was not shown. | WS12 item 8: re-fetch the runtime through a raw pointer after every `host::*` call that may allocate, or an out-slot shape where the guest allocates (ABI §8.2); a comment stating the discipline; `--release` parity run as acceptance. |
| W4 | Medium | `sakura_wasm_host.js` (no `SAKURA_MAX_*`); `runtime/rt/src/wasm.rs:934-936,1483-1486,1632-1635` | ABI §8.5/§10's **copy budget (`SAKURA_MAX_COPY_BYTES`) and the host-side heap check are not built**: a huge response is `alloc_bytes` → 0 → `D096` on WASM while JS succeeds (a backend divergence); the pending-request limit is handled three ways (`Err` for HTTP, `D090` for `serve_listen`, return-at-once for `clock_sleep`). | WS12 item 8: a byte cap in `copyOut`/the async reply giving `Err("response too large")` for `http_*` and a `D09x` elsewhere; one policy for a full request table. |
| W5 | Medium (framing) | `runtime/rt/ABI.md:452-454` (§8.1); `wasm.rs:2524-2526`; design §17.155 (3) | §8.1 says the kind-`secret` imports are "`http_send`'s `secret_headers` and `secret_expose`", that "the host zeroes its copy" and "the LIR validator refuses" other `Secret` pointers. Built: `js_handler` is the only one, redaction is dynamic, there is no LIR check, and a JS string cannot be zeroed. The clear key also sits as an un-zeroable string in `this.jsHandlers[i].h` for the process lifetime (with W7). | WS12 item 7 (ABI 1.0.3 erratum with W0): list the kind-`secret` imports as built, say the host drops its references and that the redaction is hygiene under the stated trust model. |
| W6 | Low | `runtime/rt/src/object.rs:208-214`, `heap.rs:144-160`, `wasm.rs:1761`, `scratch.rs:57-68`; `sakura_wasm_host.js:350` | Guest copies of a secret (the transport `Bytes`, `StrBuf` growth copies) are freed without zeroing (blocks are zeroed on reuse only); the host's `copyIn` slice is dropped to GC unzeroed. | WS12 item 7: zero the transport payload before `drop` when `vt_secrets` was set; `.fill(0)` the host slice after decoding. |
| W7 | Low | `sakura_wasm_host.js:372-376` (`retire_handle`), `:857-864`, `:486`, `:412,428`, `:452` | **`callback` handles are never retired** (`retire_handle` looks in `handles`, callbacks live in `callbacks`), and `jsHandlers`, `sql` and `props` only grow: a `Serve.host()` program building `Llm.anthropic` or `Sql.sqlite` per request leaks a `Secret`-holding handler or an open `DatabaseSync` per request. | WS12 item 8: retire kind 5 from `callbacks`; a release import called from the handler's destructor for JS-handler and SQL instances. |
| W8 | Low | `sakura_wasm_host.js:921-926` | A handle's kind is checked against the registry but **not against the operation's expected kind**: a hand-made module passing a `jsvalue` where `fs_read` expects a `Dir` raises a `TypeError` that escapes as a `D090` trap. No authority is gained (the object is one the module already held). | WS12 item 8: `decode` takes the expected kind for `Dir`/`Body` arguments. |
| W9 | Low | `sakura_wasm_host.js:1240-1244,1274`; `exec.rs:1129,1146` | After a `HostFault` inside an import, `runTests` continues on the same instance with `sched.running` left `true`; ABI §8.6 says teardown. | WS12 item 8: re-instantiate or stop after a trap. |
| W10 | Low | `crates/sakura_cli/tests/parity.rs:181-193`; `tests/wasm.rs:263-277` | Fixtures with per-backend stdout skip the trace comparison entirely, and the `Sql.sqlite` case compares traces without task numbers (the open §17.155 question), so a host that reordered records only there would pass. | WS12 item 8: compare traces for `separate` fixtures too; decide the `Fs.root()` task-number question (design, below). |
| W11 | Info | `runtime/rt/src/wasm.rs:111-114,1657-1705` | `complete`/`invoke` trust the host's `payload` pointer; `_len` and the resume `status` are ignored. Inherent under the trust model; a cheap defence is a `tag == BYTES && rc == 1` check. | WS12 item 8 (same change). |
| W12 | Info | `sakura_wasm_host.js:685-689`; `:190-192`; ABI §8.4 | Generation is a constant 1 and slots are never reused, so the generation test is dead code; `file` ids are plain `Int`s, not handles, as on JS. ABI §8.4 describes neither. | WS12 item 7: ABI erratum. |

### The WASM `Secret` decision (for Opus, also filed in `compiler/REQUESTS.md`)

**Yes.** Flip exactly two flags and remove the refusal:

1. `compiler/runtime/rt/src/wasm.rs:1510` and `:1519`: `async_hosted(rt, frame, &args, false, …)` → `true` in `rt_k_http_send` and `rt_k_http_stream_open`; reword their doc comments.
2. `compiler/runtime/js/sakura_wasm_host.js:283-286`: delete `unreviewed`; `:526` and `:528` pass `rt.http_send` and `rt.http_stream_open` directly.

Tests that must accompany the change, in the same commit: (a) `crates/sakura_cli/tests/wasm.rs::http_host_sends_on_wasm_as_on_js` asserts WASM stdout equals JS stdout (`201 true 1` first line) and keeps the `s3cr3t` grep over WASM stdout, stderr and trace; (b) `runtime/rt/tests/vt.rs`: the tuple `rt_k_http_send` encodes carries the header's inner value and the tuple `http_recording_lookup`/`http_recording_append` encode carries `14, Unit`; (c) a `tests/wasm.rs` case running a package test with `Http.recording` under `Http.scripted([])` and `Http.bearer` on `--target wasm` asserting the token appears in neither output nor the fixture file (`examples/secrets` already runs on WASM; make the assertion explicit); (d) `runtime/js/test/wasm_host.test.js`: a request whose `secret_headers` decode to a real `rt.Secret` reaches `fetchArgs` with the header set. Docs in the same commit: ABI 1.0.3 erratum (W5, W12), `website/content/docs/roadmap.mdx:53` and `tour/memory.mdx:159` (drop "refused until the owner's security review"), CHANGELOG. The round-11 rule stays: **no further kind-`secret` import without a review entry here**; after this change the list is `js_handler`, `http_send`, `http_stream_open`.

### Workspace tests

`cargo test --workspace --locked --no-fail-fast` at `6ccce80`: exit 0; 102 test binaries `ok`, 0 failed, 557 tests passed, 2 ignored. The suites named in parts A–C (`escape.rs`, `tests/ui` `V6001`–`V6003`/`V2031`–`V2033`, `semantics` with `sql_untrusted_param`, `url_untrusted_component`, `uuid_decode`, `cli.rs` `test_record_*`, `wasm.rs`, `vt.rs`) are in that run.

### Signature

Gate 10: **passed**. Reviewed and signed by Claude Fable 5.1 under the owner's written delegation
of 2026-10-18. The §17.128 freeze is lifted with the commit that lands this record; the follow-ups
are plan WS12 and the REQUESTS entry of 2026-10-18 "gate 10". The owner may overturn any line by
editing this file; the recorded decision is the reviewer's, not the owner's.
