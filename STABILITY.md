# Stability and compatibility

## Today: pre-alpha / design phase

Sakura Lang has a published `0.1.0` preview and a source-buildable compiler,
but no stable language/API contract.

The next release is **`v1.0.0-beta.1`**, a beta of 1.0 and not for production (design
§17.291 and its 2026-10-21 addendum). Its promise is deliberately loose: **it may have bugs
and it may change before 1.0.0**, including the contracts the inventory below lists; every
breaking change between the beta and 1.0.0 is listed in the [CHANGELOG](CHANGELOG.md) with a
migration. RFC 0001's final comment period does not gate the beta; its guarantees bind from
the release at which the owner accepts it, and in full from `v1.0.0`. Release-specific platform evidence is listed
on the [install page](website/content/docs/install.mdx).
[The design](docs/design.md) is provisional; examples are illustrative unless
explicitly backed by a runnable test. Research and performance goals do not imply
verified soundness, production readiness, or measured efficiency.

## Before 1.0

The first preview was `0.1.0-alpha.1`; the current release is `0.1.0`.
Preview behavior may change between prereleases.
[Draft RFC 0001](docs/rfcs/0001-stability-guarantees.md) proposes stronger guarantees
for owner acceptance after its final comment period, which does not gate `v1.0.0-beta.1`;
it does not change this policy until accepted.
Once numbered non-prerelease versions exist, reserve **minor releases** for
deliberate breaking language/API changes and **patch releases** for compatible
fixes. Describe affected programs, rationale, and migration steps in release notes.
Security or correctness fixes may exceptionally break behavior in a patch; explain
why and document the smallest practical migration.

Experimental features must be opt-in and explicitly labeled outside compatibility
guarantees. RFC acceptance is not stabilization. Record implementation and
stabilization decisions separately and list the scope of any promised contract.
Internal compiler crates remain unpublished implementation details until an SDK
is deliberately designed; sharing a repository does not make their APIs stable.

The development toolchain is pinned in `compiler/rust-toolchain.toml`, and
the minimum supported Rust version (MSRV) is `workspace.package.rust-version`
in `compiler/Cargo.toml`. The required `msrv` CI job installs that toolchain,
checks all workspace targets with the lockfile, and runs workspace tests on Linux.
MSRV increases may occur in pre-1.0 minor releases and must be announced in notes.
Supported host/target and OS/ABI minimums will be stated only after native smoke
tests exist; there are no supported target guarantees yet.

The compiler caches std's analysis on disk, one file per compiler build, in
`<cache>/sakuralang/std/`: `<cache>` is `$SAKURA_CACHE_DIR`, else `$XDG_CACHE_HOME` or
`~/.cache` on Unix and `%LOCALAPPDATA%` on Windows. `SAKURA_NO_CACHE=1` turns it off. The
files are per-build artefacts with no compatibility promise: their format, name and location
may change in any release, any of them may be deleted at any time, and a missing, damaged or
foreign file only costs the analysis it would have saved.

## Draft public-interface inventory

**Draft until RFC 0001 is accepted.** These are proposed classifications,
not stability guarantees for the current preview. `stable` means intended for the
RFC's compatibility contract; `experimental` means no compatibility promise;
`internal` means an implementation or development interface. The first-honouring
column is conditional on acceptance: `1.0.0-beta.1` starts the machine-contract rules, which
may still change during the beta with a CHANGELOG migration (owner, 2026-10-21), and a
listed-migration promise for source and std; `1.0.0` starts full source/std compatibility
within a major. `—` makes no stabilization commitment.

This inventory follows design §17.232 and the actual parser, emitters and hosts.
The [generated CLI baseline](compiler/contracts/cli.txt) pins commands, arguments,
accepted values, defaults, hidden switches, literal environment reads and driver
exit constants. [Regeneration and scope](compiler/contracts/README.md) explain the
drift test. Human wording, diagnostic prose, help layout and generated code bytes
are not contracts. Flags below belong only to their named command; positionals
and built-in help commands are also captured in the baseline.

| Surface | Item (implemented unless explicitly qualified) | Draft level | First honouring version |
| --- | --- | --- | --- |
| CLI | Root `-h`/`--help`, `-V`/`--version`; `help [COMMAND]`; per-command `-h`/`--help` | stable | 1.0.0-beta.1 |
| CLI | `check [PATHS]`: `--json`, `--frozen-specs`, `--since` | stable | 1.0.0-beta.1 |
| CLI | `fix [PATHS]`: `--preview`, `--apply`, `--frozen-specs`, `--since`, `--json` | stable | 1.0.0-beta.1 |
| CLI | `build [PATHS]`: `--json`, `--target js\|wasm`, `--out-dir`, `--manifest` | stable | 1.0.0-beta.1 |
| CLI | `build --emit js\|lir`, `--layout tagged\|niche` (`niche` accepted by clap but unimplemented) | experimental | — |
| CLI | `build --dump direct\|core\|owned`; hidden `build --no-direct`, `--wasi` | internal | — |
| CLI | `run [PATH] [-- ARGS]`: `--target js\|wasm`, `--out-dir` | stable | 1.0.0-beta.1 |
| CLI | `verify [PATHS]`: `--json`, `--target js\|wasm`, `--rlimit`, `--solver` | stable | 1.0.0-beta.1 |
| CLI | `test [PATHS]`: `--json`, `--seed`, `--filter`, `--update-snapshots`, `--record`, `--contracts`, `--std`, `--target js\|wasm`, `--out-dir` | stable | 1.0.0-beta.1 |
| CLI | Hidden `run --no-direct`, `test --no-direct` | internal | — |
| CLI | `fmt [PATHS] --check`; `audit [PATHS] --json`; `doc [SYMBOL] --search --json`; `explain CODE`; `prompt --budget` | stable | 1.0.0-beta.1 |
| CLI | `new KIND NAME --setup`; `init --kind`; `setup --client --apply` (kinds `cli`, `lib`, `agent`, `api`; clients `claude`, `codex`, `cursor`, `all`) | stable | 1.0.0-beta.1 |
| CLI | `lsp`; `mcp serve --root` | stable | 1.0.0-beta.1 |
| Native | Native backend and Cargo `native` feature; `--target native` on `build`, `run` and `test` is accepted only by a compiler built with that feature, and release binaries are built without it; a native `main` that declares `Http`, `Serve`, `WebSocket` or `Llm` exits 75 at startup, and a native worker whose entry declares one ends at its start the same way (RFC 0002 §7) | experimental | — |
| JSON Lines | `diagnostic`, `hole`, `summary`: `schema_version = "1.0"` in [diagnostic rendering](compiler/crates/sakura_diagnostics/src/json.rs), used by check/build/fix, pre-verification errors and a `test` run that does not compile; one `kind` per closing shape, every `summary` field required (design §17.242 (1)) | stable | 1.0.0-beta.1 |
| JSON Lines | `audit`, `audit_summary`: `1.0`; `obligation`: `1.1`; verify's post-solver `diagnostic`: `1.1`; `verify_summary`: `1.0` | stable | 1.0.0-beta.1 |
| JSON Lines | Test `test`, `test_summary`: `1.0` on JS and WASM; `regression` `1.0`, the record `test --contracts` stores as `tests/regressions/<fn>.json` and replays | stable | 1.0.0-beta.1 |
| JSON Lines | `doc --json`: one `symbol_query` record, `1.0`; `build --manifest`/verify's manifest file: `manifest`, `1.1`. Usage failures are human stderr with exit 2 even with `--json` (design §17.242 (3)); `fix --preview --json` prints a human preview | stable (`symbol_query`, `manifest`); usage text is not a contract | 1.0.0-beta.1 |
| Runtime JSON | `defect` on stderr at exit 70: `1.0`, with `code`, `message` and, when the runtime knows the site, `spans` of `uri`, `start_byte`, `end_byte`, `role` (no `content_hash`) | stable | 1.0.0-beta.1 |
| Runtime instrumentation | Effect trace and WASM statistics/coverage output formats, separate from CLI `--json` | internal | — |
| `sakura.toml` | `[package]` with `name`, `version`, `edition`; `[permissions]` with `fs.read`, `fs.write`, `net`, `env`; `[verify]` with `rlimit` | stable | 1.0.0-beta.1 |
| `sakura.toml` | `[permissions] process` reserved: `process = []` is accepted and grants nothing, the same as leaving the key out; no std effect starts processes. Any other list is `V8006` with the message "`process` grants nothing in this version: no std effect starts processes; the key is reserved" (design §17.253 (3)); a `Process` effect needs an RFC | stable (`[]` only) | 1.0.0-beta.1 |
| `sakura.toml` | `[dependencies]` reserved: any form (empty, populated, inline, dotted) is refused with `V8005 dependencies_not_supported` before anything runs (RFC 0001, design §17.232); packages arrive additively in 1.x | stable | 1.0.0-beta.1 |
| `sakura.toml` | Any other table or key, a value of the wrong type, a missing `[package] name` or malformed TOML is `V8006 manifest_invalid` from `check`, `build`, `run`, `test`, `verify` and the LSP: `--json` as any `V`, exit 1, primary span at the parser's place in `sakura.toml` (design §17.239); the message is the parser's text and is not promised | stable | 1.0.0-beta.1 |
| Std modules | `std/{agent,blob,budget,bytes,channel,cli,clock,compress,console,core,crypto,decimal,duration,encoding,env,float,form,fs,gen,graph,headers,html,http,instant,int,json,list,llm,log,map,option,path,prompt,random,request,response,result,retry,router,searchparams,secret,serve,set,sql,string,task,test,tool,unit,untrusted,url,urlpart,urlpattern,stream,uuid,websocket}`: public source signatures in the [generated std index](compiler/std/INDEX.md); target support remains as documented there | stable | 1.0.0-beta.1 (listed migration); 1.0.0 within major |
| Std modules | `std/worker` and the `@worker` attribute ([RFC 0002 §2.9](docs/rfcs/0002-web-platform-apis.md)): `spawn`, `send`, `recv`, `Grants`, `WorkerError` and `close`, `Worker.inline()`, topics and `Worker.host()`'s live workers on every target (threads on JS and on WASM under Node, OS threads natively) | experimental | — |
| Diagnostic identity | Allocated `V0xxx` lex/parse, `V1xxx` resolution, `V2xxx` types, `V3xxx` effects, `V4xxx` patterns, `V5xxx` contracts/refinements, `V6xxx` capabilities, `V8xxx` package/ABI/frozen specs, `V9xxx` compiler failures | stable | 1.0.0-beta.1 (non-reuse) |
| Diagnostic identity | Allocated `W1xxx`, `W4xxx`, `W5xxx`, `W7xxx`, `W8xxx` warnings; `D001`–`D004`, `D010`–`D011`, `D020`–`D021`, `D030`–`D031`, `D090`–`D096` runtime defects; retired `V5002` remains reserved. Family labels do not allocate absent codes; the [registry](compiler/crates/sakura_diagnostics/src/codes.rs) lists exact identities | stable | 1.0.0-beta.1 (non-reuse) |
| ABI | WASM runtime [ABI 1.16.0](compiler/runtime/rt/ABI.md): version negotiation, layouts/tags, imports/exports, outcomes, ownership and optional-export rules | stable | 1.0.0-beta.1 (RFC enforcement; existing ABI revision rules apply) |
| ABI | JS [ABI v0](compiler/runtime/js/ABI.md), generated JS/LIR/Core representations, Rust compiler crate APIs and per-build caches | internal | — |
| ABI | Native runtime `offset32-v1` and native embedding/host interfaces | experimental | — |
| LSP | Stdio initialization/shutdown/exit; full document sync (`didOpen`, `didChange`, `didClose`, `didSave`), watched-file notifications, `publishDiagnostics`, hover, definition, completion, formatting, code actions; negotiated UTF-8/UTF-16 positions and versioned workspace edits | stable | 1.0.0-beta.1 |
| LSP extension | `sakura/holeContext` and Sakura diagnostic/fix data, implemented in [the server](compiler/crates/sakura_lsp/src/lib.rs) | experimental | — (extension schema decision pending) |
| MCP | Stdio tools `compiler_info`, `check`, `symbol_query`, `hole_context`, `docs_query`, `explain_diagnostic`, `format`, `check_patch`, `apply_patch`, `test`; snapshot/content-hash and patch-id protocol in [MCP documentation](docs/mcp.md); the tool results are the `--json` records of the same name (design §17.242 (5c)) | stable | 1.0.0-beta.1 |
| Environment | `SAKURA_CACHE_DIR`, `SAKURA_NO_CACHE` (cache disabled for any nonempty value other than `0`, not only `1`); `SAKURA_NODE` (Node executable override) | stable | 1.0.0-beta.1 |
| Environment | `SAKURA_MAX_COPY_BYTES` (JS/WASM host copy budget); `SAKURA_TRACE` (trace destination) | stable | 1.0.0-beta.1 |
| Environment | Native host reads `SAKURA_MAX_COPY_BYTES`, `SAKURA_TRACE`, `SAKURA_PERMISSIONS`; native-host behavior remains experimental | experimental | — |
| Environment | `SAKURA_PERMISSIONS`, `SAKURA_RECORD` (driver-to-host controls); `SAKURA_WASM_STATS`, `SAKURA_WASM_COVERAGE`; `SAKURA_BLESS` (snapshot development helper) | internal | — |
| Build environment | `SAKURA_GIT_COMMIT`, `SAKURA_BUILD_ID`, `SAKURA_STD_HASH` (compile-time reads, last two generated by the build script) | internal | — |
| Runtime limits | `SAKURA_MAX_HEAP`, `SAKURA_MAX_REQUESTS` are names used in ABI prose. The JS and WASM hosts do not read these from the environment (the WASM memory maximum is fixed at build); the experimental native driver reads `SAKURA_MAX_HEAP` and `SAKURA_MAX_REQUESTS`. Host/runtime option fields and defaults must not be mistaken for implemented env switches | experimental | — |
| Exit codes | `0` success; `1` diagnostics/errors, disproved verification, formatting differences, or failed `main`; `2` usage/setup/I/O invocation failures; `3` test failures; `70` runtime defect; `75` startup permission denial. `run` forwards the child program's status; these are driver meanings, not an exhaustive set of possible program exit values | stable | 1.0.0-beta.1 |

The table deliberately records current gaps: verify's diagnostic schema differs; native
help labels and the history gate are separate work. Acceptance must settle the
experimental LSP extension and unconsumed manifest keys before claiming that
these are stable. No package resolution, native CLI target, or environment switch
is promised merely because it appears in a design document.

## Contracts to define at 1.0

[RFC 0001](docs/rfcs/0001-stability-guarantees.md) drafts these guarantees for
owner acceptance after its final comment period, which does not gate `v1.0.0-beta.1`. It
remains Draft and does not change this policy.

Before 1.0, enumerate guarantees for source semantics, standard-library APIs,
CLI flags and exit codes, configuration formats, structured diagnostic schemas,
and LSP extensions. Version machine protocols independently when necessary.
Diagnostic codes, structured fields, and fixes are machine interfaces; human
wording and presentation should remain improvable without implying a schema break.

For planned post-1.0 removals, provide deprecation warnings and a documented
migration for **at least two minor releases and six months**, satisfying both
conditions, before removal in a major release. An urgent security/correctness
exception must be explicitly justified, announced, and accompanied by mitigation.

## Evolution and distribution

Changes to these promises require the [RFC process](docs/rfcs/README.md).
Language editions are deferred until incompatible evolution needs coexistence;
there is no edition interoperability promise today. Compiler versions, any future
language editions, and machine protocol versions are distinct contracts.

Publish only tested immutable release artifacts, with checksums, license notices,
and documented targets. Never overwrite an existing version's binaries: issue a
corrected version. Programs do not acquire the compiler's license merely by being
compiled. Any embedded runtime code will need explicit licensing and compatibility
documentation before distribution.

New tagged releases require Developer ID signing and Apple notarization for macOS
binaries, pinned to team `P62B65A837`. The Homebrew archive and npm Darwin companion
contain the same signed binary. A bare Mach-O cannot carry a stapled ticket;
Gatekeeper may need network access to verify notarization. Ordinary workflow dry
runs skip signing and say so; Linux and Windows binaries remain unsigned. This policy does not
retroactively change existing immutable releases or establish language stability.
See [the release procedure](docs/release.md#macos-signing-and-notarization).
