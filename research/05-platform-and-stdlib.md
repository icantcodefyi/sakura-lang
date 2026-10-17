# Toolchain, standard library, and application platform

Research date: **2026-10-17**. Primary-source survey and design recommendations for an agent-first general-purpose language, with a Rust compiler and JS-first, then WASM/native targets. `lang` is a placeholder CLI name. All proposed syntax and guarantees below are design sketches, not implemented features. Version-specific references are identified because several surveyed ecosystems are changing substantially.

## 1. Summary

- Ship one toolchain, one formatter, one manifest, one lockfile, one test runner, and machine-readable diagnostics. An agent should not assemble a build system before solving the application problem.
- Make the distribution comprehensive while keeping the compiler core small: stable fundamental types/effects, portable standard modules, and separately versioned official application packages.
- Use typed effects as the common integration surface for networking, storage, configuration, telemetry, LLMs, and workflows. Handlers supply production implementations, test doubles, and capability restrictions.
- Adopt immutable content-addressed packages, explicit dependency updates, compiler-checked API compatibility, and separate build/runtime permissions. Effect requirements describe authority needed; they do not grant it.
- Build one schema representation that generates codecs, validators, documentation, SQL row decoders, RPC contracts, structured LLM outputs, and MCP tool schemas.
- Prefer SQL-first checked queries plus explicit migrations. Distinguish database-checked SQL from query-builder types and runtime row validation.
- Start web development with HTTP APIs and server-rendered HTML; add typed client islands and an Elm-style state model. Server calls must retain visible network/error semantics.
- Make BAML-like LLM functions a first-class library/compiler integration: typed outputs, explicit model dependencies, partial-stream types, bounded retries, evaluations, and tools derived from selected functions.
- Derive durable replay from effect boundaries, but never promise arbitrary external actions execute exactly once. Require idempotency, transactional checkpointing, versioning, and durable budget accounting.
- Treat npm/FFI as necessary escape hatches with explicit trust boundaries. JS portability, capability security, and native memory efficiency are different guarantees.

## 2. Per-area survey: what to steal and avoid

### 2.1 Integrated toolchains and package management

| System | What to steal | What to avoid or qualify |
| --- | --- | --- |
| Go | A conventional command surface for build, run, test, format, vet, docs, and modules; predictable project structure. | A coherent CLI is not a hermetic build guarantee. Code generation and external tools still need declared inputs. [Go command implementation](https://go.dev/src/cmd/go/main.go) |
| Deno | Formatter, checker, runtime, dependency management, integrity lockfile, and deny-by-default I/O. Permissions can restrict hosts, paths, and environment variables. | Runtime permissions are not automatically package-specific isolation. Native FFI and subprocesses can escape the runtime sandbox. [Security](https://docs.deno.com/runtime/fundamentals/security/), [dependencies](https://docs.deno.com/runtime/packages/) |
| Bun | Runtime, installer, test runner, and bundler in one distribution; readable lockfiles and controlled dependency lifecycle scripts. Current docs also provide `bun check` and check-before-run. | Keep execution and checking guarantees explicit; require checking by default for the new language. [Toolkit](https://bun.sh/docs), [lockfile](https://bun.sh/docs/pm/lockfile), [installation](https://bun.sh/docs/pm/cli/install), [type checking](https://bun.sh/docs/runtime/check) |
| Gleam | A compiler-owned CLI covering package creation, builds, tests, formatting, documentation, and publishing. | Target availability remains a constraint: a library implemented only for one backend cannot become portable through tooling. [CLI](https://gleam.run/documentation/command-line-reference/), [externals](https://gleam.run/documentation/externals/) |
| Cargo | Workspaces, stable lock selections, explicit updates, dependency-tree inspection, feature/target resolution. | Duplicate major versions complicate public type identity; feature unification can create surprising combinations. Build scripts need a stronger isolation story. [Resolver](https://doc.rust-lang.org/cargo/reference/resolver.html), [workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) |
| Zig build | A typed build DAG with caching, cross-compilation, and integrated native dependencies. Package metadata can be inspected without executing build code; its original package design hashes extracted contents. | Do not require arbitrary executable build programs for ordinary packages. Make dynamic build steps an explicit advanced facility. [Build system](https://ziglang.org/learn/build-system/), [package design](https://ziglang.org/download/0.11.0/release-notes.html) |
| uv | Tool/runtime management, workspace execution, fast caching, and a human-readable universal lockfile spanning platform/Python-version conditions. | Universal resolution still needs platform-specific artifacts and execution environments. Copy the conditional graph idea, not Python environment complexity. [Project structure and lockfile](https://docs.astral.sh/uv/concepts/projects/layout/) |

**Go MVS deserves precise treatment.** Dependencies specify minimum versions; minimal version selection chooses the highest required minimum for each module in the traversed graph. It does not choose the oldest available package arbitrarily, nor automatically chase the newest release. Major versions have distinct module identities. `go.sum` records checksums, including potentially unused versions; it is not an exact dependency lockfile. The checksum database adds globally checkable consistency beyond trusting whichever proxy served a download. Steal graph stability and transparency; remember that a stable selection can preserve a vulnerable version until deliberately updated. [Go modules reference](https://go.dev/ref/mod)

**Elm's package manager uses exported API/type differences to enforce semantic version increments.** This is unusually valuable for agent-generated libraries: make compatibility a computed artifact. It does not establish unchanged behavior, performance, security, or correctness. Our API comparison should also include effect rows, error variants, wire schemas, and capability requirements. Adding a possible failure can break exhaustive callers even when the success type stays the same. [Elm's enforced semantic versioning](https://elm-lang.org/)

For actual **per-package capabilities**, LavaMoat is closer prior art than a process-wide permission switch: its policies regulate dependencies' globals, builtins, native modules, and imports; SES compartments enforce boundaries. Steal explicit, reviewable authority graphs. Avoid interpreting an automatically generated policy as proof that those powers are appropriate. [Policies](https://lavamoat.github.io/guides/policy/), [runtime compartments](https://lavamoat.github.io/about/runtime-environment/)

### 2.2 Standard-library scope

Go demonstrates the productivity of standard HTTP, TLS/crypto, encoding, filesystem, process, testing, and database interfaces. Its `database/sql` abstraction does not mean every database driver is in the standard library. Deno's `@std` demonstrates another useful boundary: official, separately distributed utilities alongside host APIs. [Go standard packages](https://pkg.go.dev/std), [Deno standard library](https://docs.deno.com/runtime/reference/std/)

Elixir/Phoenix suggests a third distinction: a platform can feel integrated without placing its web framework in the language core. Mix, OTP application conventions, Phoenix, LiveView, and database packages cooperate through conventions. Steal structured application startup, supervision concepts, and deliberate runtime configuration; avoid implementing BEAM-like fault isolation as if ordinary JS tasks already provided it. [Elixir application configuration](https://hexdocs.pm/elixir/config-and-distribution.html), [Phoenix LiveView](https://phoenix-live-view.hexdocs.pm/Phoenix.LiveView.html)

**Recommendation:** core owns types, effects, resources, concurrency semantics, and schema derivation. `std` owns portable foundational APIs. Official `platform`, `web`, `sql`, and `ai` packages provide a tested application distribution with independent release cycles. Provider SDKs and cloud integrations must update without a compiler release. “Included” means maintained, documented, tested together, and available through templates—not loaded into every binary.

### 2.3 Backend and full-stack frameworks

| Prior art | Useful mechanism | Design consequence |
| --- | --- | --- |
| Encore | Typed infrastructure declarations drive local resources, deployment provisioning, wiring, and application topology. | Derive resource requirements from code, but keep deployment topology, sizing, region, and IAM policy explicit. Inferred `Sql` use cannot decide all operational requirements. [Platform](https://encore.dev/docs/platform/introduction), [service boundaries](https://encore.dev/docs/app-structure) |
| Wasp | A declarative application layer integrates React/Node/Prisma, operations, and full-stack types. | Steal working application templates and generated glue. Avoid a second overlapping application language when the new language can express the same declarations. [Overview](https://wasp.sh/docs), [actions](https://wasp.sh/docs/features/data/operations/actions) |
| Convex | Query, mutation, and action distinctions separate reactive reads, transactional database writes, and outside interactions. | Encode these restrictions as effects. Database transaction retries must not replay arbitrary network actions. Avoid assuming reactive subscriptions emerge automatically for arbitrary SQL. [Functions](https://docs.convex.dev/functions/overview) |
| Phoenix LiveView | Server-owned state with incremental browser updates. | Excellent for forms/admin/collaboration; less suitable as the universal answer for offline use and latency-sensitive local interactions. [LiveView](https://phoenix-live-view.hexdocs.pm/Phoenix.LiveView.html) |
| tRPC | Inferred client contracts plus explicit input parsers. | Derive both endpoint codecs and client types, not only shared compile-time declarations. Preserve authorization and transport errors. [Quickstart](https://trpc.io/docs/quickstart) |
| Effect Platform/RPC | Platform services model success, errors, and dependencies; RPC declares payload, success, failure, and streaming schemas. | Native effects can remove much of the TypeScript type machinery while retaining replaceable handlers. [HttpClient](https://effect.website/docs/v3/api/platform/HttpClient), [Rpc](https://effect.website/docs/v3/api/rpc/Rpc) |
| Gleam Wisp/Lustre | Wisp uses ordinary request handlers/middleware; Lustre spans HTML, client applications, Web Components, and server components. | Keep handlers ordinary functions and client interactivity optional. Do not equate an ecosystem framework's backend support with the language's backend support. [Wisp](https://wisp.hexdocs.pm/index.html), [Lustre](https://github.com/lustre-labs/lustre) |
| Elm architecture | Explicit model, messages, pure update, and view. | A small state-transition vocabulary is easy to generate, test, and replay. Avoid forcing every local animation or input field through application-global messages. [Architecture](https://guide.elm-lang.org/architecture/buttons) |
| Leptos/Dioxus | Shared-language server/client code, server functions, SSR/hydration; fine-grained signals in UI state. | Steal typed boundaries and incremental rendering. Server/client execution phases and hydration still require explicit semantics. [Leptos signals](https://book.leptos.dev/15_global_state.html), [hydration phases](https://book.leptos.dev/server/28_async_quick_reference.html), [Dioxus full stack](https://dioxuslabs.com/learn/0.7/essentials/fullstack/) |
| HTMX | HTML requests and fragment replacement provide interactivity with little application JS. | Make typed forms and escaped HTML the first web milestone. Avoid promising rich offline state through a server-roundtrip architecture. [Documentation](https://v1.htmx.org/docs/) |

Version note: the requested `@effect/platform`, `@effect/rpc`, `@effect/sql`, and `@effect/ai` names refer to the v3 package organization. Current Effect documentation also exposes v4 APIs, including consolidated AI/SQL modules marked unstable. Treat the architectural patterns as prior art, not those package paths as a permanent compatibility contract. [v3 package index](https://effect.website/docs/v3/api), [v4 SQL](https://effect.website/docs/v4/api/effect/sql/SqlClient)

### 2.4 SQL and migrations: different meanings of “typed”

| Tool | Guarantee actually offered | What to steal / avoid |
| --- | --- | --- |
| Rust sqlx | Literal queries checked against a database at build time or previously generated offline metadata; inferred parameter/result types have dialect/nullability limitations. | Adopt offline query metadata and CI freshness checks. Do not require production credentials to compile, or claim arbitrary dynamic SQL is checked. [Query macro](https://docs.rs/sqlx/latest/sqlx/macro.query.html) |
| sqlc | Generates typed access code from SQL/schema; `verify` checks recorded queries against schema changes and currently uses sqlc Cloud. | Check previous deployed queries as well as the new application. Query generation alone does not execute migrations. [Schema verification](https://docs.sqlc.dev/en/latest/howto/verify.html), [CI](https://docs.sqlc.dev/en/latest/howto/ci-cd.html) |
| Prisma | Prisma 7 TypedSQL generates typed functions from SQL against a live database; v7 Migrate produces editable SQL histories. Current v8 documentation instead describes contract artifacts and TypeScript migration plans with checks. | Steal one schema contract and reviewable migrations; keep version-specific workflows distinct. Do not bake one ORM's evolving schema language into the compiler. [TypedSQL v7](https://www.prisma.io/blog/announcing-typedsql-make-your-raw-sql-queries-type-safe-with-prisma-orm), [v7 migrations](https://docs.prisma.io/docs/orm/v7/prisma-migrate), [v8 migration model](https://www.prisma.io/docs/orm/migrations/how-migrations-work) |
| Kysely | Query-builder inference from a declared/generated database interface; driver runtime values are not validated or converted by Kysely. | Good compositional query ergonomics; avoid mistaking a stale TS interface for a database proof. [Types and runtime values](https://www.kysely.dev/docs/getting-started) |
| Drizzle | TypeScript schema definitions drive query typing and migration generation. | Reuse schema metadata; generated migrations still need review and actual database verification. [Schema declaration](https://orm.drizzle.team/docs/sql-schema-declaration) |
| Effect SQL | Injectable clients, transaction scopes, dialect adapters, and schema-based request/result validation. `SqlSchema.findAll` decodes unknown rows at runtime. | Combine typed effects and row codecs with a separate SQL checker. Schema decoding itself does not prove SQL column names or syntax. [Pinned v3 SqlClient source](https://raw.githubusercontent.com/Effect-TS/effect/effect%403.19.0/packages/sql/src/SqlClient.ts), [SqlSchema source](https://raw.githubusercontent.com/Effect-TS/effect/effect%403.19.0/packages/sql/src/SqlSchema.ts) |

Proposed default: parameterized SQL quotations, one dialect per database resource, and metadata keyed by query text, database-engine version, extensions, and migration/schema digest. Generate metadata against a disposable database built from migrations; compile offline thereafter. Represent nullable values explicitly and check runtime decoding. Dynamic queries use a typed builder or a clearly unchecked SQL API with runtime validation.

Migration planning must detect destructive operations, drift, ordering conflicts, and incompatibility with the previous deployed application. Use expand–backfill–contract for rolling deployments. A migration's successful type check cannot establish acceptable lock duration or safe data conversion on production-sized data. Keep data backfills resumable; never run production migrations implicitly during application startup.

### 2.5 Durable execution

| System | Mechanism to borrow | Important boundary |
| --- | --- | --- |
| Temporal | Recorded event history, deterministic workflow replay, durable timers, external activities. | Workflow code changes must remain replay-compatible or use versioning. Outside effects belong in activities. [Workflow replay](https://docs.temporal.io/workflows) |
| Restate | Durable services, keyed virtual objects, workflows, journaled operations, deterministic time/random helpers. | `ctx.run` records completed results; arbitrary external writes still need retry-safe protocols. [Services](https://docs.restate.dev/concepts/services/), [durable steps](https://docs.restate.dev/develop/ts/durable-steps) |
| Inngest | Memoized steps, event triggers, waits, concurrency/rate controls, and idempotency keys. | Deduplicating an invocation and deduplicating its external action are different. Its own guidance identifies the action-before-checkpoint crash window. [Concepts](https://www.inngest.com/docs/durable-execution/concepts), [external action idempotency](https://www.inngest.com/blog/your-agent-just-learned-to-spend-money) |
| DBOS | Database-backed recovery and transactional steps that commit application writes with checkpoint output. | Ordinary steps are at least once; exactly-once database commits depend on transaction integration. [Guarantees](https://docs.dbos.dev/python/tutorials/workflow-tutorial), [transactional steps](https://docs.dbos.dev/java/tutorials/step-factory-tutorial) |
| Cloudflare Workflows | Named steps, persistence, timers, retries, and event waits in the Workers environment. | Provider limits and retry/idempotency semantics remain application concerns. [First workflow](https://developers.cloudflare.com/workflows/get-started/guide/), [rules](https://developers.cloudflare.com/workflows/build/rules-of-workflows/) |
| Vercel Workflow | `use workflow`/`use step` expose resumable control flow through compiler transformations. | A directive does not remove storage, serialization, deployment, or external-side-effect requirements. [Workflow SDK introduction](https://vercel.com/blog/introducing-workflow) |

Effects make journaling **systematic**, not free. A durable handler can intercept nondeterminism and record results; replay substitutes those results while re-running pure computation. This requires all relevant nondeterminism to cross controlled boundaries, including clocks, randomness, races, FFI, and model calls. A compiler can reject effects without a durable interpretation inside a workflow.

Checkpointing must record stable operation IDs, arguments/results, schema versions, completion order, and terminal errors. Persist large bodies as referenced blobs. A crash after sending a payment but before checkpointing remains ambiguous: use a downstream idempotency key, an atomic database transaction, or an outbox/inbox protocol. A read-before-write check alone is insufficient under concurrency. Compensation is application logic, not automatic rollback of the world.

### 2.6 AI and agent libraries

**BAML is the closest language-design precedent.** It declares typed LLM functions, prompts, model clients, and tests, then generates callable clients. Its structured streaming uses partial types; assertions/checks attach runtime constraints. Steal the explicit function contract, prompt preview, fixture-based evaluations, and distinction between partial and complete values. Avoid treating schema-valid output as factually correct, or silently accepting repaired/coerced values without reporting the transformation. [Functions](https://docs.boundaryml.com/guide/baml-basics/prompting-with-baml), [tests](https://docs.boundaryml.com/ref/baml/test), [streaming](https://docs.boundaryml.com/guide/baml-basics/streaming), [checks/asserts](https://docs.boundaryml.com/guide/baml-advanced/checks-and-asserts)

Vercel AI SDK supplies provider abstractions, text/structured streaming, tools, and an agent interface. Effect AI separates schemas/tool definitions from effectful handlers and injectable model providers. Steal interchangeable providers and typed tool errors; avoid reducing all providers to the least-common-denominator API or hiding unsupported structured-output modes. [AI SDK architecture](https://ai-sdk.dev/docs/getting-started/navigating-the-library), [agent interface](https://ai-sdk.dev/docs/reference/ai-sdk-core/agent), [Effect AI](https://effect.website/docs/v3/ai/introduction), [tool handlers](https://effect.website/docs/v4/api/effect/ai/Toolkit)

Mastra demonstrates integrated agents, workflows, suspension/resumption, and tool approvals. LangGraph distinguishes checkpointed execution state from long-term stores; an in-memory checkpointer does not survive process restart. Steal inspectable state and human-resumable workflows; avoid requiring graph-node boilerplate for a simple function/loop. [Mastra approval/suspension](https://mastra.ai/docs/agents/human-in-the-loop), [LangGraph persistence](https://docs.langchain.com/oss/python/langgraph/persistence)

DSPy separates typed task signatures from execution strategies and optimizes programs against metrics. Instructor couples output models with validation and bounded retries. Steal offline evaluation/optimization and structured validation feedback; never run paid prompt optimization during normal compilation or mistake another LLM's judgment for a proof. [DSPy](https://dspy.ai/current/), [optimizers](https://dspy.ai/3.2.0/learn/optimization/optimizers/), [Instructor](https://python.useinstructor.com/)

MCP standardizes tool discovery/invocation and input/output schemas, rather than proving tool trustworthiness. Generate a protocol adapter from explicit tool exports. Support protocol-version compatibility, authorization, cancellation, errors, and transport limits; a tool's annotations are descriptive hints, not authority. [MCP tools specification, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)

### 2.7 Observability, configuration, and secrets

OpenTelemetry provides the common vocabulary for traces, metrics, logs, and propagated context. Instrument standard effect handlers so HTTP, SQL, RPC, workflow steps, and LLM usage correlate automatically. Keep domain spans available; tracing every allocation or pure function would be expensive and noisy. Default to local development visibility and explicit production exporters, not unsolicited external telemetry. [Signals](https://opentelemetry.io/docs/concepts/signals/), [concepts](https://opentelemetry.io/docs/concepts/)

Use typed configuration descriptions with replaceable providers, as in Effect, and distinguish build configuration from runtime secrets, as Elixir does. Parse required configuration once at startup and report all missing/invalid keys together. `Secret<T>` must lack ordinary display, JSON, browser-export, and journal encoders; explicit privileged access is still necessary and redaction is not encryption. Exclude raw prompts, headers, SQL parameters, tool payloads, and secrets from default spans. [Effect configuration](https://effect.website/docs/v3/configuration), [Elixir runtime configuration](https://hexdocs.pm/elixir/config-and-distribution.html)

## 3. Proposed toolchain and package manager

### CLI

| Command | Behavior | Milestone |
| --- | --- | --- |
| `lang new api\|cli\|agent`, `lang init` | Create runnable, documented projects with locked official dependencies and sample tests. | v0 |
| `lang fmt`, `lang check --json`, `lang fix --preview` | Canonical formatting; types/effects/target checks; reviewable compiler-generated edits. | v0 |
| `lang test`, `lang test --seed N`, `lang bench` | Unit/property tests, deterministic handler fixtures, reproducible failures, benchmarks. | v0; benchmark polish v1 |
| `lang verify` | Discharge supported contracts; report proved, disproved, unknown, and timeout separately. Never label an unchecked program verified. | Limited v0, expand v1 |
| `lang run`, `lang dev`, `lang build --target js` | Check before execution; watch/reload; emit ESM, source maps, declarations, and a resource manifest. | v0 |
| `lang add`, `remove`, `update --plan`, `sync --locked` | Deliberate dependency edits; immutable installation from the lockfile. `--frozen` additionally forbids network access. | v0 |
| `lang deps tree\|why\|audit\|capabilities`, `lang vendor` | Explain dependency/authority paths, vulnerabilities, and offline artifacts. | Basic v0; full audit v1 |
| `lang doc`, `lang explain CODE`, `lang lsp` | Searchable offline docs, structured explanations, editor/agent semantic services. | v0 |
| `lang db prepare\|check\|migrate plan\|migrate apply` | Generate checked query metadata, verify drift/compatibility, review and explicitly execute migrations. | SQLite v0; PostgreSQL v1 |
| `lang ai preview\|eval\|replay` | Preview rendered requests; paid evaluation only on invocation; replay recorded responses without network calls. | v0 |
| `lang api export`, `lang mcp serve` | Generate OpenAPI/TS clients and explicitly export tools over supported MCP transports. | v1 |
| `lang workflow inspect\|replay\|resume\|cancel` | Inspect durable history, test compatibility, resume approvals, request cancellation. | v1 |
| `lang deploy plan\|apply`, `lang package` | Export deployment/resource plans and distributable artifacts; apply a reviewed plan through a provider adapter. | v1 |
| `lang build --target wasm\|native`, `lang bind npm\|wit\|c` | Additional backends and audited binding generation. | npm v0; others later |
| `lang publish --dry-run`, `lang publish` | Validate APIs, documentation, effects, capabilities, provenance, and archive contents before release. | v1 |

Every command needs stable exit categories and versioned JSON output. Diagnostics should include source spans, expected/actual types, inferred effects, a path explaining leaked effects, related locations, and edit preconditions. Deterministic formatting and a documented diagnostic protocol are more valuable to agents than conversational compiler messages.

### Package design

Use `lang.toml` plus `lang.lock`. Package identity includes registry, namespace, name, and compatibility major; never resolve the same short name opportunistically across public/private registries. Lock exact versions, content hashes, target variants, compiler/runtime versions, and provider adapters. Commit locks for applications and library development; a library's development lock does not dictate its consumers' whole graph.

For native-language packages, prefer **MVS-like lower-bound resolution within each major** and require published minimum-version checks. Ordinary builds retain locked selections. `update --plan` proposes new floors/selections and shows API, capability, and vulnerability differences. Explicit root overrides handle emergency fixes and remain visible. Keep npm's range/peer-dependency solver in a separate namespace/subgraph; report incompatible shared runtime singletons rather than pretending all npm constraints follow MVS.

Cache packages by normalized content digest; record source provenance separately. Hashes detect content changes, not malicious maintainers. Add signed provenance and a transparency log; registry trust and signer trust remain separate decisions. Pin git dependencies by commit and digest. Support vendoring, mirrors, yanked releases, and offline installs without silently replacing locked artifacts.

Publishing compares exported types, effects, errors, serialized schemas, and target support. Structural breaks require the appropriate version increment; behavioral claims remain test/review obligations. Compile public contracts against supported target profiles and declared dependency minima. Do not equate compatible API shapes with equivalent behavior.

Packages declare required capabilities; applications grant narrower capabilities at composition time. For example, a dependency may request an HTTP client while its handler only permits one API host. Grants attach to package instances and cannot expand on update without a visible policy change. Build-time network/filesystem/process access is separately declared and sandboxed. Ordinary installation runs no package scripts. Hostile npm code needs compartments or process isolation; typed declarations cannot constrain ambient JS globals on their own.

## 4. Standard library roadmap and API sketches

Notation: `! {E}` is an effect row; `Fail<X>` is recoverable typed failure. `E` is a polymorphic row. Effects on stream consumption are represented in `Stream<T, E>`. Handlers remove an abstract effect and may introduce their own transport/storage failures. Cancellation and defects are distinct runtime outcomes, not silently disguised as domain errors.

### v0: a deployable HTTP API, CLI, and LLM call

| Layer | Modules and responsibilities |
| --- | --- |
| Core | `option`, `result`, `never`, `error`, `effect`, `resource`, `schema`; algebraic data, effect rows, scoped cleanup, derived boundary codecs. |
| Portable std | `text`, `bytes`, `collections`, `iter`, `math`, `decimal`, `time`, `duration`, `url`, `json`, `encoding`; explicit Unicode/byte distinctions and stable serialization. |
| Runtime std | `task`, `stream`, `clock`, `random`, `capability`, `fs`, `path`, `process`, `console`; structured concurrency, bounded buffers, cancellation, scoped authority. |
| Official platform | `cli`, `config`, `secret`, `http.client`, `http.server`, `http.router`, `log`, `trace`, `metrics`; safe defaults, streaming/SSE, graceful shutdown, Node adapter first. |
| Official data/AI | `sql`, `sql.sqlite`, `sql.migrate`, `ai.model`, `ai.prompt`, `ai.structured`, `ai.tool`, `ai.budget`; one production model adapter, fake handlers, usage accounting. |
| Verification/testing | `test`, `property`, `snapshot`, `contract`, `test.clock`, `test.http`, `test.model`; reproducible fixtures and explicit proof limits. |

```rust
fn decode<T: Schema>(input: Bytes) -> T ! {Fail<DecodeError>}
fn read(path: Path) -> Bytes ! {FsRead, Fail<FsError>}
fn spawn(cmd: Command) -> Exit ! {Process, Fail<ProcessError>}
fn now() -> Instant ! {Clock}
fn random_bytes(n: Size) -> Bytes ! {Entropy, Fail<EntropyError>}
fn parse_args<T: CliSchema>(args: List<String>) -> T ! {Fail<CliError>}
fn load<T: ConfigSchema>() -> T ! {Config, Fail<ConfigError>}
fn get(url: Url) -> Response ! {Http, Fail<HttpError>}
fn json<T: Schema>(response: Response, limit: ByteSize)
  -> T ! {Http, Fail<HttpError | DecodeError>}
fn serve(router: Router, address: Address)
  -> Unit ! {Listen, Fail<ServeError>}
fn query<P, R>(q: CheckedQuery<P, R>, params: P)
  -> List<R> ! {Sql, Fail<DbError>}
fn generate<T: Schema>(prompt: Prompt<T>)
  -> T ! {Model, Budget, Fail<ModelError | ValidationError | BudgetExceeded>}
```

`Http` is the injected client capability; its real implementation introduces `Net`, while a fake handler requires no network. Serving composes route dependencies before accepting connections. HTTP success statuses, transport failures, malformed data, and business errors remain distinguishable. Limits cover request bodies, response decoding, timeouts, concurrent requests, and stream buffers. Cryptography comes from audited host/standard implementations, never newly invented algorithms.

```rust
effect Catalog {
  fn lookup(id: ItemId) -> Option<Item> ! {Fail<CatalogError>}
}

fn show(id: ItemId) -> Item ! {Catalog, Fail<CatalogError | NotFound>} {
  Catalog.lookup(id).or_fail(NotFound)
}

test "missing item" {
  with Catalog = catalog.fake(empty()) {
    expect_failure(show(ItemId("missing")), NotFound)
  }
}
```

Resource scopes close files, HTTP bodies, DB cursors, and child tasks on success, error, or cancellation. Stream APIs must prevent use after scope exit through their type/scoping discipline. This is necessary even without a Rust-style borrow checker; “no borrow checker” cannot mean untracked resource lifetimes.

### v1: complete application platform

Add `web.html`, `web.form`, `web.router`, `web.ui`, `web.signal`, `web.rpc`, `web.session`, `auth`, `sql.postgres`, `sql.builder`, `cache`, `object_store`, `queue`, `schedule`, `workflow`, `mcp.client`, `mcp.server`, `ai.agent`, `ai.embedding`, `ai.eval`, `otel`, and `deploy`. Ship maintained auth/session mechanisms and identity-provider adapters rather than a new identity protocol.

```rust
fn transaction<T, E>(body: fn() -> T ! {Sql, E})
  -> T ! {Sql, E, Fail<DbError>}
fn publish<T: Schema>(topic: Topic<T>, value: T)
  -> MessageId ! {Queue, Fail<QueueError>}
fn render(view: Html) -> String ! {}
fn call<I: Schema, O: Schema, X: Schema>(rpc: Rpc<I, O, X>, input: I)
  -> O ! {RpcClient, Fail<X | TransportError | DecodeError>}
fn embed(text: String) -> Vector
  ! {EmbeddingModel, Budget, Fail<ModelError | BudgetExceeded>}
fn resume<I: Schema>(run: RunId, input: I)
  -> Unit ! {WorkflowStore, Fail<ResumeError>}
```

A retryable transaction combinator should accept a narrower row than the general transaction above: only database operations and replay-safe computation. General transactions may contain other effects but cannot promise safe automatic retries.

### Later

Add native/WASM adapters, WIT components, C bindings, advanced distributed workflows, incremental reactive SQL, offline synchronization/CRDTs, desktop/mobile UI, GPU/tensor interfaces, multimedia, and wider protocol/database coverage. Establish interfaces before standardizing implementations. These are official ecosystem commitments, not compiler keywords.

```rust
fn invoke<I: Schema, O: Schema>(component: Component<I, O>, input: I)
  -> O ! {ComponentHost, Fail<ComponentError>}
fn sync<T: Mergeable>(document: Document<T>)
  -> Stream<Change<T>, {Net, Fail<SyncError>}> ! {SyncStore}
```

## 5. Proposed full-stack web and AI/agent story

### Full-stack web

One workspace has explicit `shared`, `server`, and `client` modules. Shared code contains pure domain types/validation. Server modules own secrets, database handles, and privileged effects. Client compilation rejects server-only imports; it generates stubs only for explicitly exported RPC functions. This follows server-function prior art while making placement a checked property. [Dioxus server functions](https://dioxuslabs.com/learn/0.7/essentials/fullstack/server_functions/)

```rust
@server @rpc(GET, "/items/:id")
fn item(id: ItemId) -> ItemDto
  ! {Catalog, Session, Fail<NotFound | Forbidden | CatalogError>}

// Generated client contract; server dependencies are not exported.
fn item_remote(id: ItemId) -> ItemDto
  ! {RpcClient, Fail<NotFound | Forbidden | ServiceError |
                     TransportError | DecodeError>}

fn update(msg: Msg, model: Model) -> (Model, Cmd<Msg>) ! {}
fn view(model: Model) -> Html<Msg> ! {}
```

Generate runtime input/output validation, stable wire tags, public error mappings, OpenAPI, and TS clients from the same declarations. Do not serialize internal database errors. Every endpoint validates authorization server-side; calling it through generated code is no security credential. Version negotiation handles old browser bundles after a deployment. Retries distinguish idempotent reads from mutations with explicit deduplication keys.

Progression: v0 escaped HTML plus standard forms; v1 SSR with typed interactive islands. Use Elm-style messages/reducers for application state and scoped signals for local reactive rendering. Signals are a library implementation strategy, not a second unrestricted global mutation model. Effects cannot execute during pure rendering; commands run under lifecycle scopes with cancellation. LiveView-style remote components and richer offline synchronization come later. Templates may include CSS/assets without making users write JS; being a single-language platform need not ban SQL or CSS quotations.

### Typed LLM functions and tools

```rust
type Ticket = {
  title: NonEmptyString,
  priority: Low | Normal | High,
  evidence: List<SourceSpan>
}

@llm(model = Models.Extractor)
fn extract_ticket(text: Untrusted<String>) -> Ticket
  ! {Model, Budget, Fail<ModelError | ValidationError | BudgetExceeded>} {
  prompt { system: "Extract only supported facts."; user: text }
}

@tool(description = "Look up an item visible to the current user")
fn lookup_item(id: ItemId) -> ItemDto
  ! {Catalog, Session, Fail<NotFound | Forbidden | CatalogError>}

let tools = toolset(lookup_item)
let server = mcp.export(tools)
```

The LLM declaration desugars to ordinary effectful code. The compiler derives supported JSON Schema, prompt interpolation checks, codecs, and a final validator. Schema-inexpressible refinements are checked locally. Model refusal, invalid output, truncation, timeout, rate limits, and unavailable capabilities are explicit outcomes. Configure model/provider selection through handlers rather than permanently embedding model IDs into business functions.

Streaming yields `Partial<T>` events and eventually a separately validated `T`; partial arguments cannot trigger a tool. Retry/repair behavior is explicit, bounded, charged to the same budget, and recorded. Factual evidence checks and task evaluations remain necessary after type validation. A prompt preview must show actual provider payloads with secret redaction.

Tool derivation includes argument/result schemas, public error mapping, descriptions, and required capabilities. Opt-in exports prevent accidentally exposing every public function. The model chooses among offered tools but never supplies its own authorization context. Validate arguments and permissions before invoking the function. MCP imports are dynamic untrusted schemas until validated against a pinned contract; local implementations can use the same toolset for LLM calls, MCP export, and direct tests.

### Durable agent loops and budgets

Offer a small ordinary-control-flow agent loop, backed initially by an official durable-runtime adapter rather than a new distributed database. Restate is a strong candidate for JS-first durable calls/keyed agents; Temporal is another deployment adapter. Keep portable workflow APIs above their operational differences.

```rust
@workflow(version = 1)
fn investigate(task: Task) -> Report
  ! {Model, Tools, DurableClock, Journal, Budget, Approval,
     Fail<AgentError | BudgetExceeded | WorkflowError>} {
  with budget = Budget(tokens = 40_000, cost = usd("2.00"),
                       tool_calls = 20, wall_time = minutes(10)) {
    agent.run(task, tools, stop = ReportReady)
  }
}
```

Only effects with declared durable interpretations are allowed. Classify handlers as pure, recorded read, idempotent external action, transactional action, or unsupported. Stable step identity includes logical site, iteration/child identity, and workflow version; source line numbers are insufficient. Persist model responses before acting on them; do not re-ask a model for an already-recorded decision during replay. Code upgrades require compatibility checks, version routing, or explicit state migration. Suspended approval requests retain principal, operation, and expiry.

Budgets are capabilities backed by a ledger, not prompt instructions. Reserve estimated maximum cost/tokens before dispatch, reconcile actual usage, and atomically split allocations among parallel children. Charge retries and uncertain attempts; deduplicate replay accounting by operation ID. Provider billing delays and uncertain cancellation mean hard financial guarantees require provider-enforced limits or conservative reservations. A local timeout cannot retract a request already billed remotely. Persist budget state with workflow decisions so crashes cannot reset spending limits.

Default agent traces show decision/step IDs, tool names, latency, usage, retry count, and public error categories. Full prompts and outputs require explicit retention policy. Evaluation commands compare recorded datasets and model/prompt versions; fixture replay tests control flow without implying live-model determinism.

## 6. Interop strategy

**JS/npm first.** Emit standard ESM, source maps, and `.d.ts` exports. Gleam demonstrates target-specific externals; PureScript distinguishes pure/effectful/async FFI; ReScript's genType demonstrates typed export interoperability. Elm's ports/flags/custom elements offer a safer message boundary at a higher integration cost. Adopt both direct audited bindings and isolated message adapters. [Gleam](https://gleam.run/documentation/externals/), [PureScript](https://book.purescript.org/chapter10.html), [ReScript](https://v13.rescript-lang.org/docs/manual/typescript-integration/), [Elm](https://guide.elm-lang.org/interop/)

```rust
@external(js, "npm:some-sdk", "fetchRecord")
fn raw_fetch(id: String) -> JsValue
  ! {Foreign<"some-sdk">, Fail<ForeignError>}
```

TypeScript declarations are binding-generation hints, not proofs about runtime values or effects. Generated wrappers validate data, translate throws/rejections into typed failures, preserve abort signals, and model resource disposal. Unchecked declarations cannot claim purity. Bindings expose verified subsets; overloads, `any`, callbacks, mutation, and prototype behavior require explicit handling. Module initialization can perform effects too, so a safe wrapper alone does not sandbox a dependency. Hardened compartments/processes and constrained host imports provide enforcement; otherwise label the dependency trusted.

**C FFI later.** Use generated ABI declarations, opaque handles, explicit allocation/free conventions, scoped callback lifetimes, and audited wrappers. Prevent unwinding across incompatible boundaries. Untrusted native libraries require process isolation; in-process C can violate memory and capability guarantees. The Rust compiler's implementation language does not make emitted FFI calls safe. [Rust FFI guidance](https://doc.rust-lang.org/nomicon/ffi.html?highlight=panic)

**WASM components.** Map records, variants, lists, results, and resources to WIT interfaces/worlds. Imported interfaces express host requirements; host wiring decides granted capabilities. Generate bindings and pin the supported component/WASI feature profile. WIT does not encode all effect polymorphism, refinements, or application authorization. Canonical ABI lifting/lowering may allocate/copy; do not promise universal zero-copy interop or universal browser support for a selected component feature set. [WIT](https://component-model.bytecodealliance.org/design/wit.html?highlight=commen), [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html)

Publish target support in package metadata: JS-browser, JS-server, WASM-host profile, native-OS/architecture. Reject unsupported imports at compile time. JS uses host GC; achieving Rust-like native efficiency without a borrow checker requires a separate measured ownership/region/RC strategy. Keep representation decisions behind stable codecs/FFI boundaries.

## 7. Open questions and risks

1. **What is proved?** Define a trustworthy frontier: type/effect safety, schema validation, selected contracts, and verified modules. None alone proves product requirements, distributed correctness, or LLM truthfulness. Verification artifacts must record assumptions and foreign dependencies.
2. **Which authority model?** Effect labels describe operations; capabilities confer authority. Decide how handlers receive attenuated capabilities, how package boundaries preserve them, and what happens when a privileged handler becomes a confused deputy.
3. **Which resource discipline?** Scoped effects, affine handles, or regions must prevent resource escape without imposing pervasive borrow-checker ergonomics. Multi-shot continuations interacting with resources need particular restrictions.
4. **How much infrastructure inference?** A resource graph is useful; automatic production sizing, topology, IAM, and data residency are separate decisions. Export portable plans and avoid mandatory cloud accounts for local work.
5. **Which SQL source of truth?** Prefer migrations plus checked metadata initially. Supporting a second schema DSL risks drift; choosing only handwritten SQL increases parser/dialect scope. Start with SQLite and PostgreSQL, with explicit backend differences.
6. **How are upgrades handled?** API semver, wire compatibility, database migrations, workflow replay, persisted schemas, and provider model changes are separate compatibility axes. One version number cannot explain them all.
7. **Can v0 stay small?** HTTP/CLI/SQLite/one-model support already requires substantial runtime work. Defer a bespoke UI engine and distributed workflow engine; publish measurable end-to-end milestones before expanding.
8. **Can the memory goal coexist with JS-first?** Benchmark allocations, latency, bundle size, cancellation cleanup, and handler overhead separately by target. Avoid marketing native guarantees based on the compiler being written in Rust.
9. **Who maintains the batteries?** Official packages need compatibility matrices, security response, deprecation policy, and funding. Broad but stale packages are worse than a small coherent platform plus usable interop.

## 8. Sources consulted

Primary sources are linked at their relevant claims above. The grouped index below provides the principal entry points; pinned/versioned sources are retained where documentation generations differ.

- Toolchains: [Go commands](https://go.dev/src/cmd/go/main.go), [Go modules/MVS/checksums](https://go.dev/ref/mod), [Deno security](https://docs.deno.com/runtime/fundamentals/security/), [Deno dependencies](https://docs.deno.com/runtime/packages/), [Bun](https://bun.sh/docs), [Bun lockfiles](https://bun.sh/docs/pm/lockfile), [Bun install](https://bun.sh/docs/pm/cli/install), [Bun type checking](https://bun.sh/docs/runtime/check), [Gleam CLI](https://gleam.run/documentation/command-line-reference/).
- Resolution/builds: [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html), [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [Zig build](https://ziglang.org/learn/build-system/), [Zig package design](https://ziglang.org/download/0.11.0/release-notes.html), [uv lockfiles](https://docs.astral.sh/uv/concepts/projects/layout/), [Elm semantic versioning](https://elm-lang.org/).
- Library/security boundaries: [Go stdlib](https://pkg.go.dev/std), [Deno std](https://docs.deno.com/runtime/reference/std/), [Elixir config](https://hexdocs.pm/elixir/config-and-distribution.html), [LavaMoat policies](https://lavamoat.github.io/guides/policy/), [LavaMoat runtime](https://lavamoat.github.io/about/runtime-environment/).
- Application platforms: [Encore platform](https://encore.dev/docs/platform/introduction), [Encore services](https://encore.dev/docs/app-structure), [Wasp](https://wasp.sh/docs), [Wasp actions](https://wasp.sh/docs/features/data/operations/actions), [Convex functions](https://docs.convex.dev/functions/overview), [LiveView](https://phoenix-live-view.hexdocs.pm/Phoenix.LiveView.html), [tRPC](https://trpc.io/docs/quickstart), [Wisp](https://wisp.hexdocs.pm/index.html), [Lustre](https://github.com/lustre-labs/lustre).
- UI: [Elm architecture](https://guide.elm-lang.org/architecture/buttons), [Leptos state/signals](https://book.leptos.dev/15_global_state.html), [Leptos execution phases](https://book.leptos.dev/server/28_async_quick_reference.html), [Dioxus full stack](https://dioxuslabs.com/learn/0.7/essentials/fullstack/), [Dioxus server functions](https://dioxuslabs.com/learn/0.7/essentials/fullstack/server_functions/), [HTMX v1 documentation](https://v1.htmx.org/docs/).
- Effect: [v3 packages](https://effect.website/docs/v3/api), [HttpClient](https://effect.website/docs/v3/api/platform/HttpClient), [Rpc](https://effect.website/docs/v3/api/rpc/Rpc), [v3 SqlClient source](https://raw.githubusercontent.com/Effect-TS/effect/effect%403.19.0/packages/sql/src/SqlClient.ts), [v3 SqlSchema source](https://raw.githubusercontent.com/Effect-TS/effect/effect%403.19.0/packages/sql/src/SqlSchema.ts), [v4 SqlClient](https://effect.website/docs/v4/api/effect/sql/SqlClient), [configuration](https://effect.website/docs/v3/configuration).
- SQL: [sqlx](https://docs.rs/sqlx/latest/sqlx/macro.query.html), [sqlc verification](https://docs.sqlc.dev/en/latest/howto/verify.html), [sqlc CI](https://docs.sqlc.dev/en/latest/howto/ci-cd.html), [Prisma TypedSQL v7](https://www.prisma.io/blog/announcing-typedsql-make-your-raw-sql-queries-type-safe-with-prisma-orm), [Prisma migrations v7](https://docs.prisma.io/docs/orm/v7/prisma-migrate), [Prisma migrations v8](https://www.prisma.io/docs/orm/migrations/how-migrations-work), [Kysely](https://www.kysely.dev/docs/getting-started), [Drizzle](https://orm.drizzle.team/docs/sql-schema-declaration).
- Durability: [Temporal](https://docs.temporal.io/workflows), [Restate services](https://docs.restate.dev/concepts/services/), [Restate steps](https://docs.restate.dev/develop/ts/durable-steps), [Inngest concepts](https://www.inngest.com/docs/durable-execution/concepts), [Inngest idempotency analysis](https://www.inngest.com/blog/your-agent-just-learned-to-spend-money), [DBOS guarantees](https://docs.dbos.dev/python/tutorials/workflow-tutorial), [DBOS atomic steps](https://docs.dbos.dev/java/tutorials/step-factory-tutorial), [Cloudflare workflow guide](https://developers.cloudflare.com/workflows/get-started/guide/), [Cloudflare rules](https://developers.cloudflare.com/workflows/build/rules-of-workflows/), [Vercel Workflow](https://vercel.com/blog/introducing-workflow).
- AI: [BAML functions](https://docs.boundaryml.com/guide/baml-basics/prompting-with-baml), [tests](https://docs.boundaryml.com/ref/baml/test), [streaming](https://docs.boundaryml.com/guide/baml-basics/streaming), [validation](https://docs.boundaryml.com/guide/baml-advanced/checks-and-asserts), [AI SDK](https://ai-sdk.dev/docs/getting-started/navigating-the-library), [agent interface](https://ai-sdk.dev/docs/reference/ai-sdk-core/agent), [Effect AI v3](https://effect.website/docs/v3/ai/introduction), [Effect Toolkit v4](https://effect.website/docs/v4/api/effect/ai/Toolkit), [Mastra](https://mastra.ai/docs/agents/human-in-the-loop), [LangGraph](https://docs.langchain.com/oss/python/langgraph/persistence), [DSPy](https://dspy.ai/current/), [DSPy optimizers](https://dspy.ai/3.2.0/learn/optimization/optimizers/), [Instructor](https://python.useinstructor.com/), [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools).
- Observability/interop: [OpenTelemetry signals](https://opentelemetry.io/docs/concepts/signals/), [concepts](https://opentelemetry.io/docs/concepts/), [Gleam externals](https://gleam.run/documentation/externals/), [PureScript FFI](https://book.purescript.org/chapter10.html), [ReScript genType](https://v13.rescript-lang.org/docs/manual/typescript-integration/), [Elm interop](https://guide.elm-lang.org/interop/), [C FFI](https://doc.rust-lang.org/nomicon/ffi.html?highlight=panic), [WIT](https://component-model.bytecodealliance.org/design/wit.html?highlight=commen), [Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html).
