# Session record, 2026-10-19 evening

**Status note, not a spec.** What the lead agent (Claude Opus 5.5) and its agents did on the
evening before the `v0.1.0` tag, what is held, and what is left. The decision log
([design.md §17](../design.md)), [road-to-1.0.md](road-to-1.0.md), [STABILITY.md](../../STABILITY.md)
and [release.md](../release.md) win over this note. It continues
[the 2026-10-19 handoff brief](handoff-2026-10-19.md).

Roles followed the owner's rule (§17.255): Claude Opus 5.5 wrote all code; Claude Fable 5.1
made the design decisions (rounds 29–33); GPT-6-Astra did research and wrote briefs;
GPT-6.1-Sol did one docs chore. Every agent worked in its own worktree and pushed to `main`.

## 1. State before the session

- (a) The §17.254 wording fixes were already on `main` as `a91f4209`. Its `ci.yml` run
  39240285071 was fully green on every job.
- (b) The §17.249 release flags were already on `main` as `74cf5a4e`. Its release dry run
  39238699212 built and install-smoked all five `targets.json` rows.
- WS-P13b was already on `main` (`2d808283`..`0530f328`) and ships in 0.1.0. Only the owner's
  review (checklist item 13) is left.

## 2. Landed on `main` tonight

Base `a91f4209`, head `b770e639` at writing, plus the commits named in §3.

| Commit(s) | What | Author |
|---|---|---|
| `1b51934c` | The wording fix's CHANGELOG entry moved into `[0.1.0]`. The plans say native is off the 0.2.0 and 1.0 gates, confirmed by the owner | Opus (lead) |
| `c73a5751` | Docs diagrams: code wraps inside the failure diagram's `catch` box, `memory_counter(0)` no longer breaks on phones, "complete at M4" became "complete" | Opus |
| `839a2b79` | Round 29, §17.255: how the dogfood corpus works under the role rule. Also records two owner confirmations: native off the gates, tokens rotated | Fable |
| `2d50f07b` | Handoff brief lines updated for §17.255 | Sol |
| `d96f4f9b` | Round 30, §17.256: no milestone names on any user-facing page; the cookbook's harness header is stripped from the page | Fable |
| `ecc34f37` | `research/14-spec-semantics-brief.md`: 102 semantic claims, 63 covered by fixtures, 30 partly covered, 9 missing; 26 gaps found | Astra |
| `1a0ccfa8` | `dogfood/<program>/BRIEF.md` ×6 plus `dogfood/website-gaps.md` (13 gaps), written from the public website only | Astra |
| `c2f00d23` | Round 32, §17.257: the JSON baseline's one sanctioned rewrite, plus the split's five smaller readings | Fable |
| `616480ee` | §17.256 carried out: page wording, `std/INDEX.md`, landing stage "WASM and verification", llms.txt, the cookbook directive stripping, and a milestone guard in `check-doc-sweep.py` with tests | Opus |
| `540cdd6e`, `9bf972d7`, `2cb1c3e9` | RFC 0001 series: tests for `prepare`'s untested refusals; the `compat` job (`release.py compat-base`, `sakura_cli/tests/compat.rs`, `compat.yml`; vs alpha.1: 291 same, 0 differ); the cross-version ABI fixture harness | Opus |
| `318e69af` | Round 33, §17.258: compat base, ABI fixtures, `V8007 abi_export_missing`, counters, the deprecation `since` check | Fable |
| `fc290adb`, `12e2c5e0` | Round 31, §17.259–§17.261: all 26 spec gaps decided. `Int.min_value`/`Int.max_value` as top-level `let`s | Fable |
| `476215e8`..`b770e639` | §17.243 and §17.253: digit-separator rule (`V0006`); `sakura test` relays the runner's 0/2/3/70, tested through the driver; the WASM host reads the ABI version before linking (`V8003` before a `LinkError`, `D090` for a link failure); `V0015 source_not_utf8`; `[permissions] process` reserved (non-empty list is `V8006`) | Opus |

These commits are before the preparation commit, so all of them ship in `v0.1.0` (§17.250 (4)).
The lexical commits added entries under CHANGELOG `[Unreleased]`, and the preparation commit
should fold them into `[0.1.0]`.

## 3. In flight or held

- **The JSON kind split (§17.242, §17.257) is held** on branch `worktree-agent-a23b26c374cdda278`
  (`14fa1648`, `e84eae00`, `071d88a2`). §17.257 makes it the first commit after the `v0.1.0`
  tag, and its `baseline 0.2.0` line assumes that. Its checks pass:
  - `check_json.py --base 318e69af` reads "reset to 0.2.0: 337 lines replace 315";
  - fmt and clippy pass;
  - only the Node 26 tests fail.
  The lead pushes it after the tag unless the owner overrides §17.257.
- **The `exec` differential fuzz target landed** after this note was first written: `e806d8ad` (the target)
  and `72eba074` (`fuzz.yml`'s `max_total_time` input, corpora carried between runs as artifacts, and
  the refreshed `compiler/fuzz/Cargo.lock` that made the 2026-10-19 nightly fail at `cargo fetch`).
  - **What it compares:** mutated `tests/semantics` programs on the reference evaluator, JS, the LIR
    evaluator and WASM. JS and WASM must also agree on defect site and effect trace.
  - **Not run on Windows:** libFuzzer doesn't link on Windows MSVC, so the first real run is the
    nightly or a manual dispatch. A local driver ran 1,342 inputs.
  - **Two bugs found.** The nightly `exec` job fails until both are fixed (Opus, each with a
    `tests/semantics` regression):
    1. the reference evaluator stops on any `Float % Float` (`Internal("bad operands for FloatRem")`);
    2. `opt::ranges` is reported proving an operation that then defects with `D003` on
       `Console.print("{-9007199254740992}")`.
  - **The seven-clean-night streak** (§17.245 (1)) starts when the job first runs clean.
  - **Designer questions:**
    - whether libFuzzer `timeout-`/`oom-` artifacts count against a clean night (the reference
      evaluator counts a builtin as one step);
    - §17.225 (4c)'s `sakura verify` replay leg isn't built.

## 4. Questions for the owner

1. **Agent-DX eval (§17.255 (4)).** Fable reads the role rule as not covering eval submissions,
   since they never enter the repository, so the GPT eval numbers stay the cross-family
   evidence. Fable recommends confirming. If you don't, the rc notes will say the second
   family is untested.
2. **Dogfood compiler.** alpha.1 has no Windows build, this machine has no WSL distribution, and
   Node here is 24 (26 is required). The options:
   - (a) start against the v0.1.0 Windows build after the tag and install Node 26 (recommended);
   - (b) install a WSL distribution and start against alpha.1;
   - (c) use another machine.
3. **Node 26 on this machine.** It's needed for the six local test failures below and for
   dogfood option (a).
4. **G05 (§17.259).** `MIN % -1` is `0` in the log and in the verifier, but WASM and native treat
   it as a defect. Fable's fix removes that check; you confirm at the 0.2.0 window.
5. **The JSON split** stays held per §17.257 unless you say otherwise (§3).

## 5. Next work (Opus unless named)

After the tag:

- Push the JSON split.
- Run `verify-published.yml` on 0.1.0 and record the per-row results on the install page
  (§17.248).
- Add the Windows `core.autocrlf=true` `fmt --check` case (§17.248 (4)).

Now or soon:

- **§17.258:**
  - the compat-base rule (newest final tag; a prerelease only while no final exists);
  - the deprecation `since` check in `prepare`;
  - capture alpha.1's ABI module under `1.1.0/` (needs macOS or Linux);
  - v0.1.0's module under `1.4.0/` after the tag;
  - `V8007` and the export table in `abi.rs` (ABI 1.4.1, after the tag; the lead reviews).
- **§17.259:**
  - `Int.min_value`/`Int.max_value` with `S/int_min_value.sak`;
  - the G05 `IntRem` change (needs the owner's confirmation).
- **§17.260 and §17.261:**
  - JS finalizer failure becomes `D093` (G22);
  - the fixtures A06, A11, A14, A17, A19, A21, A30;
  - re-check the three milestone-M5 fixtures (G25);
  - rename `literal_refinement_passes_once.sak` (G26);
  - the fixture comment in G08.
- **Spec semantics prose** in `docs/spec/`, from Astra's brief and §17.259's evidence rule (G01).
  Astra rewrites grammar G09/G10 and `gaps.md` now that the lexer change has landed (G13), and
  writes the ABI errata.
- **The Sql transaction stall (§17.241)** is native-only, so it's lowest priority.

Dogfood:

- Six fresh Opus sessions with no repository checkout, after the owner answers §4 (2).
- Parcel Quotes needs a public WASM-embedding page first (Astra's gap list).

Designer queue:

- **The lexical agent's four questions:**
  - snake_case codes with digits (`source_not_utf8`), already widened in the registry test and
    `check_history.py`;
  - a manifest that is not UTF-8;
  - hole records in a file that is not UTF-8;
  - a stripped module's version mismatch now shows as `D090`.
- **Plan file names** (`m3-completion-and-m4.md`) still named in the skill and llms.txt, under
  §17.254 (3).

## 6. Local environment notes (Windows)

- System Bun is 1.3.14 and can't read `bun.lock`. The website gates ran with an npx copy of
  1.4.3 at `%LOCALAPPDATA%\npm-cache\_npx\aef7621c2333c54c\node_modules\bun\bin`. All five gates
  pass on `b770e639`.
- Node is 24.19. Six workspace tests fail for that reason alone:
  - five `--record` tests use `--allow-net`;
  - `node_conformance_suite_passes` runs `node --test <dir>`.
  Every agent saw the same six; CI runs Node 26.
- The pinned Rust 1.99.1 toolchain broke mid-session (a concurrent rustup repair). The milestone
  sweep agent reinstalled it with rustfmt, clippy and wasm32, and it's healthy now.
- Not available here: Z3 (the verify contract cases were skipped), actionlint (workflows were
  only YAML-parsed), symlink privilege (the README quickstart step of `check-doc-sweep.py`), and
  libFuzzer runs. `check-doc-links.py` builds routes with backslashes on Windows, a checker bug.
- CI was not watched at the owner's request, so no run result is claimed here.
