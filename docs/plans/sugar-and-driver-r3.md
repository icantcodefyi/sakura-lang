# Plan: `@llm` bodies, `Tool.of`, driver and std follow-ups (round 3)

Status: ready to implement. Design authority: `docs/design.md` §2.5, §5.9, §7.4, §10.4, §10.7,
§11.1, §11.2, §12.1, §12.2, §15.7–§15.9, §17.57, §17.58, §17.63–§17.66, §17.69. Each section names
its owner; sections are independent unless stated. Checks, commit format and push procedure as in
`docs/plans/row-params-on-types-r3.md`; `sakura_cli` tests must stay green (`cargo test -p
sakura_cli --locked` runs `sakura test --std` and the example corpus).

---

## 1. `@llm` bodies (§11.1): done

D lowered bodiless `@llm` functions in `sakura_core` (`ed3d2d9`, ABI §7 "`@llm` functions"); it
matches §11.1 and §17.65 endorses it. Nothing for B. Optional later: report a missing or non-literal
`system:` and a `@llm` function with a body as `V1010` from attribute validation in `sakura_hir`
(today the lowering rejects them as unsupported).

## 2. B — `Tool.of(f)` (`sakura_hir`, §11.2)

`Tool.of(<expr>)` is resolved as the home-module function of `std/tool` and then rewritten:

- `<expr>` must be a direct reference to a top-level function declared with `@tool("description")`;
  otherwise `V1010` with "`Tool.of` takes a function declared with `@tool`" (a lambda, a local or an
  un-annotated function all fail the same way).
- For each `@tool` function `f(p1: T1, …, pn: Tn) -> R ! Row` synthesize **once per module** a
  record type `f$Args = { p1: T1, …, pn: Tn }` (a real `TypeDef` with a name no source can write,
  not exported, hidden from `symbol_query`/`sakura doc`/hover completion; constraints derive as for
  any record, so `Schema` requires `Schema` parameters, which is the `@tool` rule). Its `Schema` title
  is `f`.
- Rewrite the call to `Tool.from("f", "description", fn(args: f$Args) { f(args.p1, …, args.pn) })`
  where `"f"` is the function's bare name and `"description"` the attribute's string literal
  (`V1010` if absent or not a literal).
- The lambda's row is inferred from `f` as usual, so `Tool<e>` (or `Tool<F>` until
  `docs/plans/row-params-on-types-r3.md` lands) takes `f`'s row.
- Tests: `examples/agent_loop` checks clean (its only error today); a lowering snapshot; `tests/ui/`
  negative cases for a non-`@tool` argument and a lambda argument; `sakura doc` output does not list
  `f$Args`.

## 3. E — driver (`sakura_cli`)

- `sakura new api` (§17.63), `[permissions]`: `net = ["127.0.0.1:8080"]`. `src/main.sakura`:

  ```sakura
  import std/request.{Request}
  import std/response.{Response, Status}
  import std/router.{Router}
  import std/serve.{Address, Serve, ServeError}

  /// The routes of this service.
  pub fn routes() -> Router<{}> {
    Router.new().get("/health", fn(_req) { Response.status(200, "ok") })
  }

  pub fn main() -> Unit ! {Serve, Fail<ServeError>} {
    Serve.listen(routes(), Address.of("127.0.0.1", 8080))
  }

  test "health answers 200" {
    expect_eq(Router.handle(routes(), Request.of(Get, Url.from("/health"))).status, Status.from(200))
  }
  ```

  (add `import std/request.{Get, Request}` and `import std/url.{Url}` as the std surface requires;
  until row parameters on types land, write `Router<fn(Request) -> Response ! {}>` and say so in the
  template comment.) The `new.rs` error for `api` goes away; `sakura new api` is covered by the CLI
  test that scaffolds and runs `sakura test` on every kind.
- `agent` template (§17.58, §17.69): `fn answer(question: Untrusted<String>) -> Answer ! {Llm,
  Fail<LlmError>}`, `main … ! {Console, Env, Http, Fail<LlmError>}`; drop `BudgetExceeded` from the
  imports.
- `sakura run` sets `SAKURA_PERMISSIONS` to the parsed table as JSON (D's shape in ABI §7) when
  starting `node __main__.js`, with paths relative to the package root and the working directory set
  to it; `sakura test` never sets it. If this is already done, a CLI test that runs a `main`
  declaring `Fs` with `fs.read = ["./data"]` and reads outside it (expecting `FsError`) is the only
  deliverable.
- `--frozen-specs`: nothing to change (§17.62 documents what is built); add the `--since` flag to
  the `sakura fix` help text if it is missing.
- Hole record (§17.61): nothing to change on E's side; when C's structured row lands, emit it under
  `allowed_effects.row`.

## 4. D — runtime

- Property runner (`sakura_runtime.js`, the `run(i)` loop): the value-reuse heuristic (`877deae`)
  stays; additionally a property fails when `discards > Math.floor(0.9 * RUNS)` after the loop,
  with the message `property gave up: N of M inputs discarded by requires` (the all-discarded case
  uses the same message). Covered by a `sakura_codegen_js` test with a property whose `requires`
  holds for about 5% of inputs.
- `http_send`: send every `req.secret_headers` entry as a header (value exposed from the `Secret`
  at send time), after the plain headers; never log them. `llm_anthropic`: build its internal
  `Request` with `x-api-key` in `secret_headers`, not `headers`. `Http.scripted` runs in Sakura and
  needs nothing.
- `LlmError` gaining `BudgetExceeded` (§17.69) needs no runtime change: the clause already raises
  the `std::budget::BudgetExceeded` value.

## 5. F — std and examples

- `std/request`: replace `bearer: Option<Secret<String>>` with `secret_headers: List<(String,
  Secret<String>)>` (`Request.of` starts it empty); `std/http.bearer(req, token)` pushes
  `("authorization", Secret.map(token, fn(t) { "Bearer " + t }))`. Update doc comments and INDEX
  (deviation 12 → resolved, §17.57).
- `std/llm`: `pub type LlmError = Refused | InvalidOutput | Truncated | RateLimited | Timeout |
  ProviderError | BudgetExceeded` (import `BudgetExceeded` from `std/budget`, a dependency `llm`
  already has). `std/agent.run`'s row may stay as written.
- `examples/llm_extract`: `extract_ticket ! {Llm, Fail<LlmError>}`, `main … Fail<LlmError>`, tests
  drop `Budget.unlimited()` (no `Budget` is performed under `Llm.scripted`). `examples/agent_loop`:
  `research ! {Budget, Http, Llm, Fail<LlmError | ToolError>}`, `main … Fail<LlmError | ToolError>`.
  `examples/property`: the membership property per design §15.9 (`List.push(xs, x)`, no
  `requires`). `examples/secrets`: `"charging {Decimal.to_string(amount)}"`.
- `std/env`: delete the `EnvScope` placeholder and the "until `[permissions]` scoping lands" note
  (§17.59; the runtime scopes `env_get`). INDEX "Planned" paragraph: drop `Env.host(scope)`.
- `compiler/std/INDEX.md` deviations: mark 11–21 resolved with their §17 numbers (56, 57, 59, 60,
  68, 53, 49, 51, 49/55, 52, 48, 54) or delete them; keep the list short.
