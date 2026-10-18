# M3 security review packet (exit gate 10)

For: **@icantcodefyi** (CODEOWNERS for `/compiler/` and `/docs/`). Prepared 2026-10-18 by an AI
agent (Claude Opus 5.5) against `origin/master` at `76fd183`; the human reviewer owns the verdict.

Gate 10 of [plan §4](../plans/m3-completion-and-m4.md#4-m3-exit-gates) needs a named maintainer
review of three security-relevant pieces. Everything below already passes its tests
([gate report](../plans/m3-gate-report.md)); the review is about whether the *rules* are the right
ones and whether the code enforces them. Budget: about 45 minutes, in three parts. Sign off at the
bottom.

---

## A. WS1 escape rules (`V6001`, `V6002`, `V6003`)

**Why it matters.** Capabilities (`Dir`, `Handler<Fs>`, sockets) and resources (`File`, `Body`)
are authority. If one outlives its `with` or callback, code runs with authority that the
enclosing scope already revoked (design §6.3, §8.4).

**Read:** design §17.70, §17.89 (round 4), WS1b in plan §7, and these files:

| File | What |
|---|---|
| `compiler/crates/sakura_types/src/check/escape.rs` | the whole check (module doc states the rules) |
| `compiler/crates/sakura_types/src/check/decls.rs` (`V6001` part) | user `@external` minting or receiving capabilities |
| `compiler/crates/sakura_types/tests/escape.rs` | accepted and rejected cases |
| `compiler/tests/ui/V6002_*`, `V6003_*`, `V6001_*` | user-facing fixtures |

Commits: `28a80bf` (escape check), `02928e6` (`File` is `@resource`), `d9566ab` (`V6001`),
`d059abd` (fix), WS1b follow-ups in `sakura_types` (`git log -- compiler/crates/sakura_types/src/check/escape.rs`).

**The rules in plain language.**

1. A type is *scoped* when it is an external (bodiless) type, a `Handler<…>`, a `@resource` type
   (v0: `std/fs.File`, `std/http.Body`), or contains one. A function value is also scoped when
   it may capture a scoped local, followed through the locals it flows into.
2. `V6002`: a scoped value is the result of a `with`, is `return`ed out of one, is the type of a
   top-level `let`, or is captured by a `Task.spawn` body when the capture was bound inside a
   `with` that encloses the spawn but not the `Scope`.
3. `V6003`: a callback that receives a resource returns something holding that resource, or a
   resource is stored in handler `var` state. This applies to *any* function or operation whose
   parameter is a callback whose type contains a resource, so a user wrapper around
   `Fs.with_file` is checked at its callers.
4. `V6001`: a user `@external` that takes or returns another package's capability, a `@resource`
   type, or any `Handler<…>`, which would let FFI forge authority.
5. Not checked in v0: **affinity** (using a resource more than once). This was deliberate (§17.70).

**Known deviations and limits (all recorded in plan §7).**

- A call whose result type contains a function is assumed to capture every scoped argument
  (conservative, so it can reject valid code but cannot miss an escape).
- `Task.spawn` with a non-lambda body: only locals it reads directly are candidates; lambdas
  nested inside a call argument are **not walked** (noted in `escape.rs`). *Possible miss: decide
  whether this is acceptable for M3.*
- A `Scope` that is not a local (or a field of one) is treated as bound outside every `with`.
- Record and variant fields are inspected transitively (`type_holds`). Collections of resources
  (`List<File>`) count as "contains".

**Questions for the reviewer.**

- [ ] Is the scoped-type definition complete? In particular, should `Secret<T>` count? (`Trust`
      is external but is excluded here because `V6001` has its own rule for it.)
- [ ] Is the `Task.spawn` nested-lambda gap acceptable until M4, or should it be closed now?
- [ ] Is deferring affinity acceptable, given that `File` close is handler-driven (§8.4)?
- [ ] Is it right that `V6001` is the only FFI guard, so a std `@external` stays trusted (listed by `sakura audit`)?

---

## B. WS2 cassette format (`Http.recording`, `Llm.replay`, `Llm.recording`, `--record`)

**Why it matters.** Cassettes are committed to git. A credential that leaks into one is
published. A replayed cassette is data the program trusts as an HTTP response.

**Read:** design §17.57 (secret headers), §17.73 (cassettes), §17.88 (streams), §17.102
(recording LLM traffic and `Env` under `--record`); plan §7 WS2 items 1, 2, 5, 6 and WS8 items 3–6.

| File | What |
|---|---|
| `compiler/runtime/js/sakura_runtime.js`, `// Cassettes behind` (≈ line 2995–3110) | key, file format, lookup, append |
| `compiler/runtime/js/sakura_runtime.js`, `function inside` (≈ line 2467) | path containment for the cassette `Dir` |
| `compiler/runtime/js/sakura_runtime.js`, `test_env` (≈ line 2380) | `Env.host()` only under `--record` |
| `compiler/std/http.sakura` (`Cassette`, `recording`, `replaying`) | std side of replay and forward |
| `compiler/std/llm.sakura` (`replay`, `recording`) | LLM wrappers; key in `x-api-key` secret header |
| `compiler/crates/sakura_cli/src/main.rs`, `testing.rs` (`--record`) | grants under `--record` |
| `compiler/examples/*/tests/fixtures/*.jsonl` + `README.md` | the three committed (synthetic) cassettes |

Commits: `b63106f`, `6848fef`, `3271928`, `db0dbbc`, `77bb106`, `c049072`.

**The format and rules in plain language.**

1. The file is JSON Lines with one exchange per line:
   `{method, url, headers, body, status, response_headers, response_body | response_body_base64}`.
2. The match key is `(method, url, headers, body)` of the request. `secret_headers` are **never
   read** by cassette code, so they are neither written nor matched. `Http.bearer` and
   `Llm.anthropic`'s `x-api-key` both go into `secret_headers`.
3. Repeated identical requests replay in recorded order and the last one repeats.
4. A miss outside `--record` fails with `no recording for <METHOD> <url> in <path>; run sakura
   test --record`. Under `Llm.replay` the miss is `ProviderError { status: 0, reason }`.
5. Appending is possible only when `SAKURA_RECORD=1`, which only `sakura test --record` sets.
   `--record` grants the manifest's `net`, write access to `tests/fixtures`, and the
   manifest's `env` keys (so tests can read a real key via `Env.host()`). It is refused with
   exit 75 when `net` is empty. A plain `sakura test` grants read access to `tests/fixtures` only,
   with the network denied and an empty `Env`.
6. The cassette path must be relative, contain no `..` segment, and resolve (after `realpath`)
   inside the `Dir` and the manifest's grants.

**Things the code does NOT protect (check these are acceptable).**

- **Ordinary `headers`, the URL and the request body are written verbatim.** A token put in a
  normal header, a query string (`?api_key=`) or a JSON body ends up in the cassette. Only
  `secret_headers` are safe.
- **`response_headers` are written verbatim**, including any `set-cookie` or session token a
  server returns.
- A replayed response is not authenticated. Anyone who can edit the committed `.jsonl` controls
  what the program "receives". Response bodies reach user code as `Untrusted<String>` through
  `Response.text`, as live ones do.
- Containment uses `realpath` then a later open (time-of-check to time-of-use). That is fine
  for a test runner; it is not a sandbox (the roadmap already says the JS host is not an OS
  sandbox).
- `--record` exposes the manifest's `env` keys to **every** test in the run, not only the
  recording ones (WS8 deviation 5).

**Questions for the reviewer.**

- [ ] Is "only `secret_headers` are redacted" the contract you want, or should the recorder
      refuse (or redact) well-known credential headers (`authorization`, `x-api-key`, `cookie`)
      and `set-cookie` when they appear in ordinary headers?
- [ ] Should the cassette carry a `schema_version` line before the format is frozen (every other
      JSON record in Sakura Lang has one)?
- [ ] Is exposing manifest `env` keys to all tests under `--record` acceptable?
- [ ] Spot-check: open `compiler/examples/secrets/tests/fixtures/billing.jsonl` and confirm it
      has no token (its test is named "the recorded charge replays without the token").

---

## C. WS8 item 2: `Untrusted<T>` in templates (§17.101), a trust boundary

**Why it matters.** This decides whether attacker text that is interpolated into SQL or HTML
becomes a bound parameter or escaped text, or whether it can become code.

**Read:** design §17.101; plan §2 WS8 items 2–3; plan §7 WS8 items 1–2.

| File | What |
|---|---|
| `compiler/crates/sakura_types/src/check/expr.rs` (template conversion, `unwrapped`) | unwrap rule |
| `compiler/crates/sakura_core/src/lower/expr.rs` (`untrusted_value`) | lowering |
| `compiler/tests/semantics/sql_untrusted_param.sakura` | `' or 1=1 --` returns nothing |
| `compiler/std/sql.sakura`, `compiler/std/html.sakura` tests | std-level pins |
| `compiler/crates/sakura_types/tests/acceptance.rs` (§17.101 cases) | accepted and rejected |

Commits: `61f84e7`, `e31946a`, `4d1c21c`.

**The rule in plain language.** In a tagged template whose element type `X` has a home-module
`from` (`sql` → `SqlValue.from`, `html` → `Html.from`), an `Untrusted<T>` argument is unwrapped to
`T` and **always** goes through `X.from`, never the pass-through, even when `T` is already `X`. So
in SQL it becomes a bound parameter and in HTML it becomes escaped text. `Untrusted<Html>` in
`html"…"` renders as the escaped text of its shown form, never as markup. `Secret<String>` in
`sql` is still `V2032`.

**Known deviations.**

- `url"…"`'s element type is `String`, which has no `from`, so `Untrusted<String>` in `url"…"`
  is still `V2001` although §4.11 says `url` encodes it. Closing this is a design decision
  (plan §7 WS8 item 1).
- There is no `shell` tag yet, so it is not covered.

**Questions for the reviewer.**

- [ ] Agree that "always through `from`" (no pass-through for `Untrusted<X>`) is the invariant to
      freeze?
- [ ] Decide `url"…"`: a `from` for its element, or a pass-through rule for `Untrusted<String>`?

---

## Sign-off

| Part | Verdict (approve / changes) | Notes | Reviewer, date |
|---|---|---|---|
| A. Escape rules | | | |
| B. Cassette format | | | |
| C. `Untrusted` in templates | | | |

When all three are approved, record it in `docs/plans/m3-gate-report.md` (gate 10 → PASS) with
a link to the commit or PR that holds this signed table.
