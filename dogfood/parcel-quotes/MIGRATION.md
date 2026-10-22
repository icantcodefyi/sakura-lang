# Migration: parcel-quotes

One section per upgrade round (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
row 2a): the prediction from that release's CHANGELOG section, written and committed before
the upgrade, then the result against the published compiler.

## v0.1.0 → v1.0.0-beta.1

- Author: Claude Opus 5.5, 2026-10-21 (design §17.255 (3), §17.291 (1b)).
- Source: CHANGELOG `[1.0.0-beta.1]`, "Migration from v0.1.0", items 1–14, read against
  this program's source before any code was changed and before the beta compiler was run on it.
- Compiler: the published `sakuralang@1.0.0-beta.1` from npm,
  `sakura 1.0.0-beta.1 (7aee189efa2dd571a8a7988a91b69f507fc46399)`.

### Prediction (written before the upgrade)

The program is a library over `Http`: `fetch_rates` builds `Request.of(Method.Get, …)` and
streams the body with `Http.stream`/`Http.read_chunk` under `Http.scoped`; tests use
`Http.scripted`, `Http.recording` cassettes, two user `handler Http`s (`raw_body`,
`slow_carrier`) and a scripted 302. `main` is an offline runner over `Http.scripted` with
`Cli.exit`.

| Item | Applies? | Why |
| ---- | -------- | --- |
| 1 `Payload` bodies | Check, no edit predicted | `Response.status(…)` and `Request.of` are listed as unchanged; the 302 test builds `Response { ..Response.status(302, ""), headers: … }`, which writes no `body`. No code reads `Request.body`/`Response.body`: the library reads `incoming.body` from `Http.stream`, a streamed `Body` handle, which the item does not name. The comment above `raw_body` ("a scripted `Response.body` is a `String`") becomes stale. |
| 2 `Method` arms | No | `Method.Get` is constructed, never matched |
| 3 `redirect` | **Yes** | test 25b, "a carrier redirect is an explicit failure, never followed", scripts a 302 and expects `CarrierStatus { status: 302 }`. A scripted 3xx is now followed, so the library would chase `https://rates.example/rates`, which its own `Http.scoped([carrier.invalid:443])` refuses. The brief wants a redirect to fail explicitly, so `fetch_rates` sets `redirect: Manual` on its request ("choose `Manual` to receive the 3xx itself"), and test 25b is unchanged apart from its comment. |
| 4 `Clock`/`Task` operations | No | no user `Clock`/`Task` handler; the user handlers are `Http` handlers |
| 5 tier 1b crypto | No | no `std/crypto` |
| 6 `--json` closing kinds | No | nothing reads `sakura … --json` |
| 7 `Router` patterns | No | no `Router` |
| 8 `Path` with `\` or `:` | No | only `Path.of("carrier_rates.jsonl")`-style names |
| 9 tier 2 crypto | No | no `std/crypto` |
| 10 `ANTHROPIC_BASE_URL` | No | no LLM |
| 11 log fields as JSON | No | no `Log` |
| 12 `Console.eprint` | No | no user `handler Console` |
| 13 failing clause named | No | rejections are typed errors; no `Violation.clause` is printed |
| 14 `W1003 unused_import` | Check | no import looked unused by eye |

Predicted diff: `src/carrier.sak` `fetch_rates` sets `redirect: Manual` (item 3), with the
import of the `Redirect` type (the list does not say which module exports it; `std/request`
is the guess); the test 25b and `raw_body` comments.

### Result (after the upgrade)

Unchanged source on beta.1: `sakura check` passes with one warning, `W1003 \`Quote\` is
imported but never used` (`src/quote_tests.sak:23`); `sakura test` 30/31 on js and on wasm,
the failure being test 25b: `expected CarrierStatus { status: 302 }, got CarrierUnreachable
{ reason: "rates.example:443 is outside Http.scoped" }`, exactly item 3's prediction.

After the upgrade: `sakura check` no problems; `sakura fmt --check` clean; `sakura test`
31/31 on js and 31/31 on `--target wasm`; `sakura build` and `sakura build --target wasm`
succeed; the sample runner (`sakura run -- 1000 domestic < samples/rates.json`) exits 0
with the same quotes on both targets, and 4 (malformed, USD), 2 (weight 0) and 64 (usage) on
both (exit codes on WASM were D090/70 on 0.1.0).

| Hunk | Required? | Predicted by the list? |
| ---- | --------- | ---------------------- |
| `src/carrier.sak` `fetch_rates`: `Request { ..Request.of(…), redirect: Redirect.Manual }` and its doc comment | Yes (test 25b) | Yes, item 3 |
| `src/carrier.sak` import `Redirect` from `std/request` | Yes | Yes, item 3; the list names `Redirect` but not its module (`sakura doc Redirect` gives `std::request::Redirect`) |
| `src/quote_tests.sak` drop `Quote` from the `parcel_quotes/policy` import | No (a warning, not an error) | Yes, item 14. My pre-read missed that `Quote` was unused; the list did not. |
| `src/quote_tests.sak` test 25b comment | No (comment) | Yes, item 3 |
| `src/quote_tests.sak` `raw_body` comment | No (comment) | Yes, item 1 |
| `src/quote_tests.sak` `body_clock()` back to `Clock.virtual()` and its comment | No (removes a workaround) | n/a, not a migration: the Fixed entry "`Clock.virtual()` on WASM … advances virtual time only when no host request is in flight" (§17.293 PQ-2, whose triage said the workaround stays "until this program's migration"). 31/31 on wasm with it. |
| `src/main.sak` header: `Upgraded to:` provenance line | No (provenance) | n/a |

**Migration-list gaps: none.** Every required hunk was on the list. One near-gap in wording:
item 3 says "choose `Manual`" without naming the type's module or the record-update form a
builder needs (`Request { ..Request.of(…), redirect: Redirect.Manual }`); `sakura doc` filled it.

## v1.0.0-beta.1 → v1.0.0-beta.2

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a). Astra was unavailable, so Opus wrote the list, in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.2]` as tagged (`git show v1.0.0-beta.2:CHANGELOG.md`):
  "Migration from v1.0.0-beta.1", items 1–5, and every Added, Changed, Security and Fixed
  entry that reaches this program, read against its source as of `v1.0.0-beta.2`
  (`611dae4f`), which is the beta.1 round's.
- Compiler: the published `sakuralang@1.0.0-beta.2` from npm in a scratch prefix.
- Not blind: this round was written after `dogfood.yml` had run on beta.2 (run 39603442654
  failed in maintenance-triage only; `6bd16751` migrated its cassettes) and had passed on
  beta.3, beta.4 and beta.5. The author knew those outcomes. Each row below says what in the
  section supports it.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 a set `SAKURA_PERMISSIONS` denies every key it leaves out | No | the library and its tests set nothing. (The Node consumer, which does set it, was written after this tag and needs library entries, beta.3.) |
| 2 a binder named `error` is `V0016` | No | `error` appears only in `error` declarations, strings and comments; no parameter, `let`, `var` or pattern binds it |
| 3 an anonymous record literal is `V0017` | No | the program compiled on beta.1, and nothing matches a `V1002` |
| 4 `@worker` of another shape is `V1013` | No | no `@worker` |
| 5 `verify --json` obligation set | No | the program is not verified |
| Added: `Http.sequence` | No (optional) | the tests use `Http.scripted` and two user handlers; not a migration |
| Fixed: `sakura init` names the package | No | the program exists |

Predicted diff: **none**.

### Result (after the upgrade)

`sakura 1.0.0-beta.2 (611dae4faf5e4f9524a8544911b9ecf695525d1b)`, unchanged source: `sakura check` no problems; `sakura fmt --check` clean; `sakura
test` 31/31 on js and 31/31 on `--target wasm`. The Node consumer is not run on beta.2: it needs
library entries, which ship in beta.3 (`dogfood.yml` skips it before beta.3).

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.

## v1.0.0-beta.2 → v1.0.0-beta.3

- Author: Claude Opus 5.5, 2026-10-23 (design §17.255 (3), §17.301 (3d); plan `road-to-1.0.md`
  row 2a), in a commit before the upgrade.
- Source: CHANGELOG `[1.0.0-beta.3]` as tagged (`git show v1.0.0-beta.3:CHANGELOG.md`):
  "Migration from v1.0.0-beta.2", items 1–4, and the Added, Changed and Fixed entries, read
  against the program as of `v1.0.0-beta.3` (`472dea56`), which is the beta.2 round's result
  (`main`'s since `6bd16751`; `dogfood/` has not changed since that tag).
- Compiler: the published `sakuralang@1.0.0-beta.3` from npm in a scratch prefix.
- Not blind: `dogfood.yml` had passed on beta.3 (run 39635001196) before this was written.

### Prediction (written before the upgrade)

| Entry | Applies? | Why |
| ----- | -------- | --- |
| 1 the WASM runtime ABI is 1.16.0; a 1.15 host refuses a beta.3 module | No | `sakura test --target wasm` builds the module and loads the host from the same install; the Node consumer loads `sakura_wasm_host.js` from the same `--out-dir` as `main.wasm`, so host and module are one release |
| 2 a root module's qualifying `pub fn` is a library entry | **Yes, used, no edit** | this is the release the consumer was written for (against `main`'s source build, which already had entries). `quote` (`Untrusted<ParcelInput>` at the top of its parameter, `Outcome` result, row `{Clock, Http, Task, Fail<QuoteError>}`), `render_outcome` and `describe_problem` qualify. `dogfood.yml` runs `consumer/consumer.mjs` on a wasm and a js build from beta.3 on; predicted: "all checks passed" on both, its `--denied` and `--empty` children included |
| 3 `verify` names compound path guards | No | the program is not verified |
| 4 the pipeline runs on a 256 MiB thread | No | nothing to migrate, by the item |

Predicted diff: **none**; the consumer runs from this release.

### Result (after the upgrade)

`sakura 1.0.0-beta.3 (472dea56e094017df4c9313170a1bf249c3878b8)`, unchanged source: `sakura check` no problems; `sakura fmt --check` clean; `sakura test`
31/31 on js and 31/31 on `--target wasm`. The Node consumer, as `dogfood.yml` runs it
(`sakura build --target wasm|js --out-dir …`, then `node consumer/consumer.mjs <dir>`, Node
26.12.0): exit 0 and "all checks passed" on both builds, 24 checks each, the `--denied` and
`--empty` children included. The js build's `__entries__.js` lists `describe_problem`, `quote`
and `render_outcome`.

**No hunk needed.** Prediction and result agree. Migration-list gaps: none.
