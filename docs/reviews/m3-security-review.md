# M3 security review packet (exit gate 10)

For: **@icantcodefyi** (CODEOWNERS for `/compiler/` and `/docs/`). Prepared 2026-10-18 by an AI
agent (Claude Opus 5.5) against `origin/master` at `76fd183`; revised 2026-10-18 by the design
agent (Claude Fable 5.1, round 7) so that it describes the state **after WS9's fixes** (plan
[WS9](../plans/m3-completion-and-m4.md#2-workstreams-to-finish-m3) items
3–6, design §17.122–§17.125). The human reviewer owns the verdict.

Gate 10 of [plan §4](../plans/m3-completion-and-m4.md#4-m3-exit-gates) needs a named maintainer
review of three security-relevant pieces. Everything below passes its tests once WS9 lands
([gate report](../plans/m3-gate-report.md), re-audited per its follow-up 8); the review is about
whether the *rules* are the right ones and whether the code enforces them. Budget: about 45
minutes, in three parts. Sign off at the bottom.

**Review the commit that WS9.10's re-audit names in the gate report**, not `76fd183`. Until the
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
   (v0: `std/fs.File`, `std/http.Body`), or contains one. A function value is also scoped when
   it may capture a scoped local, followed through the locals it flows into.
2. `V6002`: a scoped value is the result of a `with`, is `return`ed out of one, is the type of a
   top-level `let`, or is captured by a `Task.spawn` body when the capture was bound inside a
   `with` that encloses the spawn but not the `Scope`. **The candidates are the body's free
   locals** (§17.123): every local the body expression mentions, including inside lambdas nested
   anywhere in it (`Task.spawn(s, make(fn() { Fs.read(f) }))`), minus the binders those lambdas
   introduce. Before WS9 only directly read locals were candidates for a non-lambda body.
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
- A `Scope` that is not a local (or a field of one) is treated as bound outside every `with`.
- Record and variant fields are inspected transitively (`type_holds`). Collections of resources
  (`List<File>`) count as "contains".

**Decided before your review (§17.123), for you to confirm or overturn.**

- [ ] The nested-lambda gap is **closed** (rule 2 above), not deferred to M4.
- [ ] `Secret<T>` is **not** scoped: it is data, not authority; it may be stored and returned, and
      its guards are `V2031` and the audited `Secret.expose`. (`Trust` is external but excluded
      here because `V6001` has its own rule for it.)
- [ ] Deferring affinity **stands**: the runtime closes a `File` at the callback boundary, so a
      late use is `FsError`, not authority (§8.4).
- [ ] `V6001` **is** the only FFI guard: a std `@external` is in the trusted computing base
      (§6.6) and listed by `sakura audit`.

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

1. The file is JSON Lines. Its first line is `{"schema_version": "1.0", "kind": "cassette"}`;
   then one exchange per line:
   `{method, url, headers, body, status, response_headers, response_body | response_body_base64}`.
   A missing or empty file is a cassette with no exchanges and gets the header on its first
   append. A file whose first non-empty line has no `schema_version`, or a major other than `1`,
   fails `open` with `HttpError` naming the file and `sakura test --record`.
2. The match key is `(method, url, headers, body)` of the request. `secret_headers` are **never
   read** by cassette code, so they are neither written nor matched. `Http.bearer` and
   `Llm.anthropic`'s `x-api-key` both go into `secret_headers`. **The guarantee, precisely: a
   `Secret<T>` value never reaches a cassette unless the program calls `Secret.expose`, which
   `sakura audit` lists.**
3. **Credential-shaped ordinary headers are refused.** A request whose `headers` has a name that
   lowercases to `authorization`, `proxy-authorization`, `x-api-key`, `api-key`, `x-auth-token`
   or `cookie` fails under `Http.recording` with
   `HttpError { reason: "header <name> belongs in secret_headers (Http.bearer); not recorded" }`,
   on hit and miss alike and before any forwarding, so a plain-string token is never written.
   `Http.host` and `Http.scripted` are not recorders and are unchanged.
4. `set-cookie` in `response_headers` is stored as `["set-cookie", "<redacted>"]` and replays as
   that literal. Other response headers are the server's data and are stored verbatim.
5. Repeated identical requests replay in recorded order and the last one repeats.
6. A miss outside `--record` fails with `no recording for <METHOD> <url> in <path>; run sakura
   test --record`. Under `Llm.replay` the miss is `ProviderError { status: 0, reason }` whose
   reason names `Llm.recording` and `sakura test --record` (§17.125 (4)).
7. Appending is possible only when `SAKURA_RECORD=1`, which only `sakura test --record` sets.
   `--record` grants the manifest's `net`, write access to `tests/fixtures`, and the
   manifest's `env` keys (so tests can read a real key via `Env.host()`). It is refused with
   exit 75 when `net` is empty. A plain `sakura test` grants read access to `tests/fixtures` only,
   with the network denied and an empty `Env`.
8. The cassette path must be relative, contain no `..` segment, and resolve (after `realpath`)
   inside the `Dir` and the manifest's grants.

**Things the code does NOT protect (confirm these are acceptable).**

- **The URL and the request body are written verbatim.** They are the request, a replay must
  match them, and a `Secret` cannot enter them without an audited `Secret.expose` (`V2032` in
  `url"…"`, `V2031` in `Json.encode`). A plain `String` token that was never a `Secret` and is put
  in a query string or a JSON body is outside the guarantee; the header denylist (rule 3) catches
  that mistake only for headers.
- A replayed response is not authenticated. Anyone who can edit the committed `.jsonl` controls
  what the program "receives". Response bodies reach user code as `Untrusted<String>` through
  `Response.text`, as live ones do. Cassettes are reviewed like any committed test data.
- Containment uses `realpath` then a later open (time-of-check to time-of-use). That is fine
  for a test runner; it is not a sandbox (the roadmap already says the JS host is not an OS
  sandbox).
- `--record` exposes the manifest's `env` keys to **every** test in the run, not only the
  recording ones (WS8 deviation 5; endorsed in §17.122 (5): the list is the operator's declared
  allowance and the run is explicitly online).

**Decided before your review (§17.122, §17.125), for you to confirm or overturn.**

- [ ] Rules 1–4 above: typed guarantee plus a header denylist that **refuses** rather than
      redacts, `set-cookie` redacted, URL and body verbatim, `schema_version` header line.
- [ ] Exposing manifest `env` keys to all tests under `--record` is acceptable.
- [ ] The public std names `Cassette`, `Http.cassette(path)`, `Cassette.open(c)`, `Http.replaying(c)`
      (WS8 deviation 3) are acceptable.
- [ ] Spot-check: open `compiler/examples/secrets/tests/fixtures/billing.jsonl`, confirm its
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
| `compiler/tests/semantics/sql_untrusted_param.sakura` | `' or 1=1 --` returns nothing |
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
its shown form, never as markup. `Secret<String>` in any template is still `V2032`.

**`url"…"` after WS9 (§17.124).** The element type is the opaque `std/urlpart.UrlPart`, built only
by `UrlPart.from<T: Show>`, which percent-encodes `show(v)` (unreserved set `A–Z a–z 0–9 - . _ ~`).
A `String`, `Int` or `Uuid` goes through `from`; an `Untrusted<T>` is unwrapped and goes through
`from`; a `UrlPart` passes through, which is safe because nothing but `from` constructs one.
Before WS9 the element type was `String`, so `Untrusted<String>` was `V2001` and `Int` was `V2032`
although §4.11 promised both.

**Known deviations.**

- There is no `shell` tag yet, so it is not covered; §17.124's pattern (an opaque element type
  whose `from` quotes) is how it will be.
- `Untrusted<Html>` renders `Html(&quot;…&quot;)`, the escaped `Show` form of the wrapper, rather
  than the escaped markup text. Endorsed in §17.125 (2): `Html` is opaque and not `Schema`, so
  the value arises only from `Untrusted.of(html"…")` in a test, and the invariant holds.

**Decided before your review (§17.124, §17.125), for you to confirm or overturn.**

- [ ] "Always through `from`" (no pass-through for `Untrusted<X>`) is the invariant to freeze.
- [ ] `url"…"` gets an element type with a `from` (`UrlPart`), not a pass-through rule for
      `Untrusted<String>` into a `String` element.
- [ ] The `Untrusted<Html>` rendering above is acceptable.

---

## Sign-off

| Part | Verdict (approve / changes) | Notes | Reviewer, date |
|---|---|---|---|
| A. Escape rules | | | |
| B. Cassette format | | | |
| C. `Untrusted` in templates | | | |

When all three are approved, record it in `docs/plans/m3-gate-report.md` (gate 10 → PASS) with
a link to the commit or PR that holds this signed table, lift the §17.128 freeze, and change the
roadmap from "complete pending review" to complete. If any part is "changes", open WS9b in the
plan with the requested changes and re-issue this packet against the commit that lands them.
