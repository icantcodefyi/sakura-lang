# RFC 0001: Stability guarantees

This draft defines the compatibility contract that begins at `v1.0.0-beta.1` and is enforced
for 1.0 (design §17.291, which folds the planned `v0.2.0` into the beta). It does not stabilize today's implementation. [STABILITY.md](../../STABILITY.md)
remains the current policy until the owner records acceptance and updates it.

## Metadata

- Number/title: 0001, Stability guarantees.
- Champion: @icantcodefyi, proposed accountable owner; confirmation pending review.
- Authoring: Astra research role, drafted with OpenAI GPT-6 Astra through Codex.
  The human contributor owns acceptance and the resulting guarantees.
- Discussion and PR: not opened. The owner requested this draft directly on main.
  The pitch is design §17.225–226, round 23, commit `a7d9e205`.
- Decision status: Draft.
- Implementation status: Partial existing checks; enforcement gaps below remain open.
- Stabilization status: Not stabilized.
- FCP start/end: Not started; the maintainer records seven calendar days under the
  [RFC process](README.md) before acceptance. The FCP does not gate the beta.
- Decision date/owner: Not decided; @icantcodefyi accepts after the FCP, at a beta or at
  `v1.0.0`, whichever release follows its close.

## Motivation

Agents need to upgrade the compiler without rewriting valid programs or guessing
whether a diagnostic code changed meaning. Embedders need an explicit answer to
whether an older host can load a newer WASM module. Contributors need release
checks that reject an incompatible change before publication.

[Design](../design.md) §17.225 (6)–(9) and stage 5 of the
[roadmap](../plans/road-to-1.0.md) require these promises. This RFC preserves the
existing deprecation period of two minor releases and six months. It does not
promise packages, a stable native target, editions, or public Rust compiler APIs.

## Concrete examples and guide-level explanation

These are proposed release-policy examples, not claims about available releases.

- A program using stable 1.2 language features and std APIs must compile unchanged
  on 1.3 and retain its specified behavior. The compiler may produce faster code
  or new warnings. It must not require a newly introduced annotation.
- A JSON consumer that ignores unknown fields can read the same command's 1.3
  output using the 1.2 contract. Adding an optional field is permitted. Renaming
  `code`, changing a byte offset to a character offset, or dropping a required
  field is not.
- Retiring a diagnostic keeps its code reserved forever. A different failure
  needs a new code, including across major compiler releases.
- A host at ABI 1.2 accepts an ABI 1.2 module with a newer PATCH and refuses an
  ABI 1.3 module with `V8003 abi_version_mismatch`. A host probes for `task_free`
  and runs correctly without it, retaining finished root tasks as the documented
  resource cost of its absence.

The following manifest fragment is a proposed rejection example, including when
there are no entries. It is not a working package declaration.

```toml
[dependencies]
```

The compiler must emit `V8005 dependencies_not_supported`, proposed here but not
yet allocated in code, with a message explaining that packages arrive in a later
minor release. It must not silently ignore the table or fetch anything.

## Normative semantics

“MUST” and “MUST NOT” below describe the proposed contract. They take effect only
on the release schedule in this section. Compiler, JSON schema, and runtime ABI
versions are distinct. An ABI major bump does not authorize a source-language
break within a compiler major.

### Release scope and first honoring versions

`v1.0.0-beta.1` is the proposed first release to honor diagnostic non-reuse, additive
JSON evolution, the ABI rules, the explicit dependency refusal, and the CI-tested
MSRV policy; during the beta these may still change, each change a CHANGELOG entry with a
migration (owner, 2026-10-21). It also starts recording deprecations and source migration notes.
These dates are release requirements, not assertions that the checks exist.

Source and std compatibility during the beta is a listed-migration promise: a change that
breaks a program valid on one beta is a CHANGELOG `Breaking` entry with its migration.
Full source compatibility within a major starts at 1.0.0.
The two-minor/six-month removal rule applies to features stabilized at 1.0 or
later; 0.x previews do not acquire that removal delay merely by accepting this
RFC. The installed 0.1.0-to-1.0.0-beta.1 dogfood migration remains a separate evidence
gate under design §17.225 (2), not an unconditional 0.x compatibility promise.

The ABI already has its own revision contract in
[ABI §0](../../compiler/runtime/rt/ABI.md#0-versioning-of-this-document).
This RFC requires its enforcement from 1.0.0-beta.1 without resetting that history.

### Source compatibility

For any two released compiler versions A and B in the same major, with
`1.0.0 <= A < B`, a program valid under A's stable language and std contract MUST
remain valid under B without source edits and retain its specified observable
behavior. This includes types, effects, successful results, specified failures,
and public std signatures. Adding syntax, names, or inference rules MUST NOT
change the meaning or validity of an existing conforming program.

The comparison holds for the same inputs, permissions, supported target/runtime,
and documented resource configuration. It excludes experimental features,
unspecified behavior, reliance on a compiler bug contrary to the specification,
and changes in external services. Performance, generated code text, binary bytes,
internal IR, and exact memory usage are not source contracts. A new warning is
permitted; making that warning an unconditional error is not. A user-requested
warnings-as-errors mode may fail on new warnings.

Planned incompatible changes require a new compiler major and the deprecation
path below. An urgent security or correctness exception requires a named
maintainer decision, the violated contract or vulnerability, affected programs,
a minimal migration or mitigation, and a regression test in the release notes.
This is not an exception for convenience or an unmeasured optimization.

### Diagnostic identity

Every published diagnostic code MUST retain its assigned meaning forever.
A withdrawn code MUST remain listed as reserved in
[`codes.rs`](../../compiler/crates/sakura_diagnostics/src/codes.rs), including
across compiler majors. Codes MUST NOT be renumbered, reassigned, or deleted to
make room for another diagnostic. Correcting detection of the same condition is
permitted; assigning a different condition to an existing code is not.

Human wording, rendering, and the number of diagnosed instances may improve.
Machine fields and span units follow the JSON contract. The existing retired
`V5002` entry is precedent, not proof that historical non-reuse is enforced.

### JSON compatibility

Every public `--json` output MUST have a documented, versioned contract, including
success, diagnostic, summary, and failure records for each command. During a
compiler major, changes to these contracts MUST be additive. The guarantee also
holds from 1.0.0-beta.1 until 1.0, except for a beta change listed in the CHANGELOG with a
migration; 1.0 must explicitly record any transition from the beta contract.

Existing field names, types, requiredness, meanings, units, framing, and documented
ordering MUST remain valid. A producer may add optional fields that old consumers
ignore. It MUST NOT make an old optional field required of a consumer, remove a
field required in output, change nullability, or narrow accepted input. Adding
an enum value or record kind is additive only where the old contract explicitly
requires consumers to tolerate unknown values or kinds. Otherwise it is breaking.
Diagnostic code identifiers form an open set, with unknown codes handled by the
same record contract.

Schema versions MUST distinguish an additive revision from a breaking revision.
A schema-major bump alone does not permit changing the default `--json` contract
inside a compiler major. A future separately selected protocol requires its own
RFC and continued support for the old stable contract. Clients MUST ignore unknown
optional fields. Human prose, object-key order, and whitespace are not contracts
unless a particular protocol explicitly specifies them.

### ABI versioning

The WASM runtime contract is ABI §0, including design §17.222's optional-export
rule. Its `MAJOR.MINOR.PATCH` classification is independent of compiler SemVer.

- MAJOR changes an already shipped byte layout, tag number, import signature,
  outcome code, or ownership rule. Changing or removing an export signature is
  MAJOR as well.
- MINOR adds tags, imports, section fields, or layouts while leaving old ones
  unchanged. Making an export required, so a host refuses a module without it,
  is MINOR under §17.222 and must be documented as such.
- PATCH changes wording or records errata that do not alter shipped ABI
  contracts. An added export is PATCH only when the host probes for it,
  its absence costs resources but never semantics, and ABI §9 documents when it
  appeared and the cost of its absence. `task_free`, optional since ABI 1.2.2,
  is the concrete case. This is not permission to label any added export PATCH.

The module version states what it requires of its host. The host MUST refuse a
module whose ABI MAJOR differs or whose MINOR is greater, with
`V8003 abi_version_mismatch` rather than a trap. PATCH does not cause version
refusal. Export availability MUST be tested by presence, never inferred from a
version. Required-export checks are additional to the version check. Runtime,
std, and user code linked into a module MUST use the same ABI revision.

### Deprecation

A planned removal of a stable feature MUST provide a warning and a documented
migration for at least two minor releases and six calendar months, satisfying
both conditions, before removal in a major release. This exceeds a one-minor
minimum and preserves STABILITY.md and design §17.225 (6).

The release that first ships the warning starts the clock. For example, if 1.3.0
first warns on 1 January, both 1.3 and 1.4 must ship with the old behavior, and
removal cannot precede 1 July or occur before 2.0.0. Prereleases do not count as
minor releases. Metadata MUST record the feature, first warning version/date,
replacement or reason no replacement exists, and earliest eligible removal.
Machine-readable warnings MUST identify the deprecated feature. A fix MUST NOT
grant permissions or weaken a contract. Urgent exceptions use the named decision
and mitigation process above; codes remain reserved even after removal.

### MSRV

The MSRV is exactly `workspace.package.rust-version` in
[`compiler/Cargo.toml`](../../compiler/Cargo.toml), currently `1.99`.
The development toolchain pin is not evidence of MSRV support. Starting with
1.0.0-beta.1, a required job in [ci.yml](../../.github/workflows/ci.yml) MUST read that
value, install it, and run `cargo +<msrv> check --workspace --all-targets --locked`
in `compiler/`. CI MUST fail when the declared compiler cannot check the locked
workspace, including its test targets. The regular development-toolchain tests
remain required; an MSRV check is not a claim that those tests ran on the MSRV.

MSRV increases may ship only in a minor release, with an explicit release-note
line giving old and new versions. Patch releases MUST NOT raise it, including
through transitive dependency updates or lockfile regeneration. MSRV is a build
requirement for compiler contributors, not a Rust requirement for Sakura source
or prebuilt-binary users. CI must use the supported build prerequisites needed by
the workspace; failures must not be bypassed by checking only an easy crate.

### Reserved packages and exclusions at 1.0

At 1.0.0-beta.1 and 1.0, programs are single packages over std. `[dependencies]` and the
existing lockfile name are reserved for milestone P. Any syntactically valid
manifest declaring the table, even empty or in inline-table form, MUST fail with
`V8005 dependencies_not_supported` before resolution, build, or execution. The
message MUST explain that packages arrive in a later minor. Malformed TOML keeps
its parse diagnostic. The refusal must be available through structured output
and must not perform network access or modify the lockfile. Allocating V8005 in
`codes.rs` and design §12.3 is an implementation prerequisite, not done by this RFC.

Packages may arrive additively in 1.x after their own research and plan. Their
absence at 1.0 is explicit; reserved syntax is not a promise of resolution today.

`--target native` remains experimental, opt-in, and outside the stable target and
ABI guarantees at 1.0. M5-core's native parity gate is required evidence, not
stabilization. [M5b](../plans/m5-native.md) remains after 1.0, including N3 Perceus
reuse, N4 borrow inference, §17.177's representation windows, and the plan's native
platform B/C rows. M5b cannot break stable source semantics or use the native
exclusion to evade a stable WASM ABI rule.

Internal compiler Rust APIs, cache formats and locations, exact formatter output,
terminal wording, and unreleased features are not stabilized here. CLI flags,
exit codes, configuration, and LSP extensions need an explicit stable inventory
before 1.0 as STABILITY.md already requires. Being in the monorepo alone does not
make an interface stable. Language editions remain deferred.

## Alternatives and prior art

Sources checked on 2026-10-19. These comparisons support the proposal; they are
not Sakura specifications.

- [Rust RFC 0507](https://rust-lang.github.io/rfcs/0507-release-channels.html)
  separates unstable features from stable releases. The
  [edition guide](https://doc.rust-lang.org/edition-guide/editions/) preserves old
  editions and permits crates using different editions to interoperate.
  [RFC 1105](https://rust-lang.github.io/rfcs/1105-api-evolution.html) classifies
  API changes and acknowledges some minor-change breakage. Sakura adopts explicit
  opt-in instability, but does not invent editions or copy every Rust exception.
- [Go 1 compatibility](https://go.dev/doc/go1compat) promises continued compilation
  and correct execution of unchanged source against the language and standard
  packages. It separates source from binary compatibility and names security,
  unspecified behavior, and compiler bugs as exceptions. Sakura uses that scope
  with explicit MUST requirements and its separately versioned WASM ABI.
- [Gleam 1.0](https://gleam.run/news/gleam-version-1/) enumerates public tools in
  its stable release and prioritizes backward compatibility, with security and
  soundness exceptions. Sakura likewise needs a named inventory, but does not
  claim Gleam's package-manager scope for 1.0.
- [Elm 0.14](https://elm-lang.org/news/0.14) introduced package tooling that enforces
  semantic versioning using API diffs. Sakura can adopt mechanical comparisons
  for codes and schemas now. API diffs alone do not prove semantic compatibility,
  and Elm's package claim is not a promise about Sakura's deferred packages.
- [Zig 0.16.0 release notes](https://ziglang.org/download/0.16.0/release-notes.html)
  document breaking pre-1.0 changes and migration work. That is a useful preview
  practice, not evidence of a post-1.0 stability contract. Sakura retains explicit
  0.x migration notes rather than treating a pre-1.0 number as silent permission
  to break every interface.

Doing nothing leaves agents dependent on snapshots of current behavior. Promising
all interfaces forever would include internals and unfinished native work that
we cannot support. The proposed rule is strict for named stable contracts, with
small, recorded exceptions and release gates. Passing tests is evidence for the
promise; it is not a proof over every possible program.

## Drawbacks and costs

Historical fixtures and schema inventories must be maintained across releases.
Stable names constrain future syntax and std additions. A minimal MSRV check
still adds CI work and requires compatible dependencies. Deprecation keeps old
implementations available longer. No performance improvement is claimed here.

## Compatibility and migration

This commit changes documentation only. STABILITY.md gains a pointer, not new
promises. Its stale release-history wording is outside this draft's scope.
On acceptance, the owner must reconcile that file with this RFC and the actual
release state. Source evolution, retired codes, schema revisions, ABI changes,
and MSRV changes must each be classified separately in release notes.

The dependency refusal intentionally changes today's parser, which accepts a
`dependencies` map in
[`manifest.rs`](../../compiler/crates/sakura_analysis/src/manifest.rs).
Remove that table to use a single package over std. Do not suggest downloading or
vendoring packages as an automatic repair. Old design §9 package goals are future
work under §17.225 (7). Existing lockfiles do not establish dependency support.

## Agent and tooling impact

Agents can match stable diagnostic identities and preserve unknown JSON fields.
Editors must tolerate additive fields, retain source-byte span semantics, and
present deprecations without silently applying authority-changing fixes.
Incomplete programs retain their diagnostic and recovery contracts. Signature
discovery and generated code documentation must distinguish active and reserved
codes. Human wording and formatting can improve without schema churn.

## Security and resource limits

Compatibility does not grant filesystem, network, or process authority. A package
refusal happens before any fetch. Existing malformed-input, nesting, output,
heap, and copy-budget limits remain in force; this RFC adds no execution path or
unsafe code. Published limits and failure codes must be inventoried with their
contracts. A compatible release must not silently lower a documented limit for
the same configuration; a necessary security reduction uses the exception record.

## Tests and validation

The following is an enforcement inventory, not a claim that these tests ran for
this documentation change. Existing tests exercise current behavior; historical
compatibility checks still need implementation.

| Guarantee | Existing evidence | Required addition and owner |
|---|---|---|
| Source and std compatibility | `compiler/tests/semantics`, `compiler/tests/ui`, `sakura_cli` examples/parity targets and CI Rust/Parity lanes | Opus/Sol: preserve a release-indexed source/std corpus, run unchanged programs on each newer compatible compiler, and reject changed outcomes; exercise the separate installed dogfood upgrade from §17.225 (2). Include additions that could shadow existing names. |
| Codes never reused | `crates/sakura_diagnostics/src/codes.rs` registry tests, especially `gate_2_registry_and_constructed_codes_agree`; retired `V5002` | Opus: compare code-to-meaning assignments against a checked-in released baseline, retain retired entries, reject deletion/reassignment, and make reserved status available to generated docs. A uniqueness test at one revision is insufficient. |
| Additive JSON | `crates/sakura_diagnostics/tests/render.rs` tests `diagnostic_record_matches_schema_shape`, `rendered_row_has_the_schema_shape`, `hole_record_shape`; CLI integration tests | Opus/Sol: inventory every command and record, publish versioned contracts, validate positive and failure outputs, compare against released schemas, and test old consumers with unknown optional fields. Mutation cases must reject removed fields, changed units/types/framing, and closed-enum additions. |
| ABI | `crates/sakura_codegen_wasm/tests/abi.rs::the_node_host_implements_the_shared_abi_revision_and_counters`, `crates/sakura_lir/tests/abi.rs`, `runtime/rt/tests/abi_dumps.rs`; Node `runtime/js/test/wasm_host.test.js` version-refusal test | Opus: retain old module/host fixtures and compare exported/imported signatures and layout baselines with the claimed bump. Test an old host/new module and new host/old module with `task_free` absent/present, no semantic difference, and explicit resource accounting. Also test required-export refusal and changed-signature MAJOR classification. |
| Deprecation | Policy text only | Opus/Sol: deprecation metadata, warning fixtures, and a release check that rejects premature removals by version and date, including either boundary being missed. Record exceptions explicitly. |
| MSRV | Manifest declaration and development-toolchain CI only | Sol: required job using the manifest MSRV and locked workspace/all-targets command above; include it in `ci-required`; compare release metadata to reject patch MSRV increases. |
| Reserved dependencies | `Manifest::parse` currently accepts a defaulted map | Opus: allocate V8005 and refuse presence before execution; add `tests/ui` fixture coverage plus manifest/CLI integration tests for absent, empty, populated, inline, malformed, and unknown-key cases, JSON code/span, no fetch, and unchanged lockfile. Extend the UI fixture input mechanism for manifests if necessary. |
| Experimental exclusions | Native plan and existing native/parity lanes in CI | Opus/Sol: pin opt-in refusal and experimental labeling, keep required native parity separate from the stable JS/WASM compatibility corpus, and add release inventory checks so experimental features cannot be labeled stable accidentally. |

Paths in the table are relative to `compiler/` unless they already start with
`compiler/`. CI must fail on a deliberately incompatible baseline mutation, not
merely generate a replacement snapshot. Review every baseline change. Do not
normalize diagnostic identities, source outcomes, ABI bytes, or schema units.

Documentation validation checks relative links and cited sources. There is no
new runnable Sakura example here; the TOML fragment is explicitly a future
rejection test. Compiler and website builds are not required for this prose-only
change. Current CI configuration is evidence of jobs present, not of their last
run having passed.

## Implementation and rollout

1. Owner reviews the draft under the RFC process, records objections and FCP,
   and resolves the questions below. Merging Draft does not accept it.
2. Opus and Sol implement the enforcement rows required for 1.0.0-beta.1, including the
   named dependency refusal and MSRV CI job. Record exact fixture and job links.
3. After the FCP, at the next beta or at `v1.0.0`, the owner records acceptance, the first
   honoring release for each
   guarantee, and any unfinished item as a release blocker. Update STABILITY.md
   separately. Acceptance is not blanket language stabilization.
4. Before `v1.0.0`, complete the stable-interface inventory, fixture-indexed
   spec and historical compatibility checks. The ABI annex is promoted by
   reference under §17.225 (9). Native and packages retain their exclusions.
5. At 1.0, record stabilization separately after the roadmap's release gates.
   An accidental break requires a corrective release and regression fixture;
   never overwrite an immutable release artifact or silently weaken this RFC.

## Unresolved questions

- The owner must confirm the proposed boundary: listed-migration source compatibility
  during the beta, full-major source compatibility from 1.0 (design §17.291 (1b)).
- Opus must confirm V8005 is still free when implementing, allocate it in the
  registry and design, and update this draft if concurrent allocation took it.
  `dependencies_not_supported` is the proposed diagnostic name, not existing code.
- The owner must approve the stable CLI/configuration/LSP inventory and precise
  contracts before 1.0. This RFC cannot claim an inventory that does not yet exist.
- Sol must establish the MSRV build prerequisites and required CI integration.
  The current development pin alone does not establish support.

### Round-25 design decisions (2026-10-19)

Design §17.232 settles the enforcement details so implementation can start; this RFC's
text above is unchanged and the owner then planned to accept at `v0.2.0`, which design
§17.291 folds into `v1.0.0-beta.1`:

- `V8005 dependencies_not_supported` (PACKAGE) is confirmed free and allocated by Opus with
  a project-shaped `tests/ui` fixture directory.
- The JSON contract is the existing per-record `kind` and `schema_version` (`MAJOR.MINOR`
  per kind); the gate is a checked-in field baseline `compiler/contracts/json.txt`, a
  `sakura_cli` test that derives each record's fields from real output, and an append-only
  history check in the `check_history.py` shape. A MAJOR bump is refused inside a compiler
  major. The baseline's history is append-only from the commit that writes its
  `baseline 0.2.0` line, before the tag (design §17.257 (1)).
- Cross-version ABI fixtures are one committed module per shipped ABI version under
  `compiler/runtime/rt/tests/fixtures/abi/`, loaded by the host tests; the refusal case is a
  fixture with its version global rewritten.
- Deprecation metadata is `compiler/deprecations.toml` (empty at 1.0.0-beta.1), checked against
  `codes.rs` and by `release.py prepare`, which also refuses a patch release whose
  `rust-version` changed.
- The historical corpus is the previous tag's `tests/semantics`, `tests/ui` and `examples`,
  run by a `compat` CI job with an allow-list that cites a CHANGELOG anchor.
- The stable-interface inventory is a table in STABILITY.md pinned by a generated
  `compiler/contracts/cli.txt`; experimental items carry `experimental` in `--help`.
- The round-25 boundary (0.2.x within a minor, full within a major from 1.0) is superseded by
  §17.291 (1b): listed migration during the beta, full within a major from 1.0.

## Decision record

Draft requested for roadmap stage 5. No FCP or acceptance has been recorded.
The owner will record disposition, rationale, objections, and implementation and
stabilization references here. The planned acceptance point is the first release after the
FCP closes; the FCP does not gate `v1.0.0-beta.1` (owner, 2026-10-21).
