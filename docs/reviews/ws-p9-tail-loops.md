# WS-P9 review packet (mutual tail loops)

For: **@icantcodefyi** (CODEOWNERS for `/compiler/`). Prepared 2026-10-19 by an AI agent (Claude
Opus 5.5) at design §17.192 (3). The human reviewer owns the verdict.

**Status: awaiting review. Not approved.** Signing the table at the bottom closes plan
[`performance-r1.md`](../plans/performance-r1.md) §5 item 6 for WS-P9. The work is merged and
pinned by fixtures; nothing waits on the sign-off. This packet changes no code.

Commits under review:

| Item | Commits | Design |
| --- | --- | --- |
| WS-P9: mutual tail calls become jumps into one loop per def | `ad6e2e19` (measured before rebase as `bd869ef`) | §17.160 (5), endorsed as built in §17.192 (1) |
| Plan notes and the advisory `countdown` delta | `1276088c` | plan §8 WS-P9 items 1–8 |

Why the review: WS-P9 changes no output, effect order or defect message, but it moves the code
that decides which call a contract defect blames. Before WS-P9 a call such as `down(n - 2)` in
`up` was an ordinary call, and the callee's `requires` blamed it through the site parameter
(§17.115). After WS-P9 that call is a jump inside a copy of `up`'s body placed in `down`, and
the blame is set by a binding the transform inserts. The question for the reviewer is whether,
on every path, a defect still names the call as written in the source.

---

## 1. What moved

All of it is in `sakura_core::opt` (`compiler/crates/sakura_core/src/opt.rs`: `merges`,
`nest`, `tail_calls`, `rewrite`). Both backends read the result as ordinary Core.

**(a) Copied bodies.** For a def `f` in a cycle of tail calls, `merges` picks the other members
that `f` may copy. A member is copyable when it is a monomorphic top-level def (no type or row
variables), returns `f`'s type, and contains no `Check` bind (`has_check`). `tail_calls` then
puts each copy into `f` as a join point of `f`'s own self-tail loop, with its locals and labels
renumbered (`renumber`), and turns every tail call of a member into a jump to that member's
label. `f` keeps its def, signature, `DefId` and export. Callers outside the cycle still call
`f`.

**(b) Blame inside copied code.** A def whose checks blame its caller (`requires`, `T.from`'s
`where`; `blames_caller`) takes the call site as a parameter (§17.115). Inside its loop, a jump
back to the def's own label is preceded by

```
let $site: Unit = ()        // at the position of the call it replaces
jump j<f>(args…)
```

`rewrite` inserts this binding for every tail call of the host itself, including calls that sit
inside a copied member's body. The binding's position is the replaced call's span, and a copy
keeps its source spans when renumbered, so `down(n - 2)` inside the copy of `up` carries the
span of `down(n - 2)` in `up`'s source. Jumps between copied members get no binding, because a
copied member has no checks to blame from.

The backends read `$site` as follows:

- **JS** (`sakura_codegen_js/src/emit.rs`, `simple_bind`): `$site = <site of pos>;` before the
  jump. The host's checks read `$site` on the next iteration.
- **LIR/WASM** (`sakura_lir/src/lower.rs`, the `SITE_MARK` statement arm): the pseudo-local
  `$site` is rebound to the site group `cx.group(host, pos)`, and the host's checks read it.

On entry the host's checks blame the external caller through the site parameter, as before.
Every later iteration blames the member call that re-entered the loop.

**(c) What is never copied.** A def with any `Check` (`requires`, `ensures`, `where`) is never
copied into another def. A copy's defect message would then name the wrong def: JS emits
``precondition `nonneg` of <def> violated`` with the emitting def's name. Such a def can still
host. A def with `ensures` has no tail calls at all: Core binds the result to the
postcondition's join (`jump j1(r)`), so a copied member's `ret` never skips a host's
postcondition. Calls inside `Handle` or `Try` regions are not tail calls (`tail_targets`).

**(d) Copied code under a different interval context.** A copy is reachable only through the
host's loop, so the JS interval pass (`ranges`, design §17.114) sees it under the host's facts.
In `mutual_tail_blame`, `up`'s `n - 2` inside `down` is marked `proved`, because it is reached
only after `down`'s `requires n >= 0` and `n != 0`. This only drops the overflow check on
`Int` arithmetic the facts bound, so it is sound. Outside a host it would be unproved.

**(e) Limits.** At most `MERGE_MEMBERS` = 4 defs per cycle (the host included) and
`MERGE_NODES` = 1,024 copied Core expressions per def. A member's body sees only the labels of
the members around it and of the next one, so `nest` searches an order (first fit in `DefId`
order). A cycle with no fitting order, such as a three-def clique where each def calls both
others, keeps its calls. WS-P9b (§17.192 (2)) will take that shape.

Unchanged: `--no-direct` emission (the Core is the same for both ABIs), non-tail calls, generic
cycles, handler regions and the ABI. No def, export or ABI entry is added.

## 2. Invariants and the fixtures that pin them

Every `tests/semantics` fixture runs under the direct ABI and under `--no-direct`. Both must
meet the expectations, and the two effect traces (§17.80) and stderr must be byte-identical
(`compiler/crates/sakura_cli/tests/semantics.rs`). `tests/parity.rs` holds WASM to the same
stdout, defect record (code, message and spans, §17.162) and trace. Each fixture below is
therefore also a trace and defect parity check.

| Invariant | Source | Pinned by |
| --- | --- | --- |
| A defect raised through a merged loop blames the call as written: message names the host, primary span is the member's call, the clause is the definition span | §17.115, §17.192 (1) | `requires_in_a_mutual_tail_loop_blames_the_member_call` in `sakura_codegen_js/tests/m3.rs` (both ABIs; spans `down(n - 2)` and `n >= 0`); `mutual_tail_blame` (`D020` at 10^6 iterations, JS and WASM, spans compared by `parity.rs`); the LIR corpus print `corpus__print__mutual_tail_blame.snap` (inside `down`'s loop the copied call rebinds `$site` to its own group, `const 4`) |
| A def with a contract check is never copied, but may host | §17.160 (5), §17.192 (1) | `mutual_tail_blame` (`down` has `requires` and hosts `up`; `up` keeps its call of `down`) |
| Self-tail blame from before WS-P9 is unchanged (the same `$site` binding, now inserted by `rewrite`) | §17.115 | `requires_in_a_self_tail_loop_blames_the_self_call` in `sakura_codegen_js/tests/m3.rs` (both ABIs, direct and through a function value); `arith_ranges_requires_violated` (`D020` blaming `down2(n - 2)`, JS and WASM) |
| Effects in a merged loop run in program order with equal traces | §17.80, §17.192 (1) | `mutual_tail_loop` (`ping`/`pong` perform `Console.print` from the loop) |
| Merged cycles run in constant stack on both backends | §17.160 (5) | `mutual_tail_loop` (`is_even`/`is_odd` and a three-state cycle at 10^6); `bench_counters` (`countdown` n=1000 frames 1,006 → 4 direct, 1,007 → 7 all-frame) |
| Non-tail mutual calls, generic cycles and a five-def cycle keep their calls | §17.160 (5) limits | `mutual_tail_calls_share_one_loop` in `sakura_codegen_js/tests/source.rs` and its snapshot; `mutual_tail_loop` (`count_even`/`count_odd`, `left`/`right`) |
| `--dump direct` and the LIR agree on direct facts | §17.118 | `sakura_lir/tests/direct.rs` (the `ping`/`pong` "recursion" reason case now uses a non-tail cycle, since a tail cycle no longer forms a component) |

## 3. Known limits and open notes

- No fixture runs the same program with and without WS-P9. There is no switch to turn the
  transform off, and both ABIs and both backends see the merged Core. Trace and defect parity
  above therefore compares merged programs with each other. The comparison with the unmerged
  program is the expected stdout and defect record, and the span assertions in
  `requires_in_a_mutual_tail_loop_blames_the_member_call`, which name the source call.
- Blame names the call that re-entered the loop on the iteration that fails. It does not name
  the external call that first entered it. This is the same rule §17.115 set for self tail
  calls, extended to calls in copied bodies.
- The k² code growth is bounded by the two limits in 1(e), not measured against plan §5 item 4's
  2× `micro.sak` budget. `micro.sak` has no mutual tail cycle.
- Performance (plan §8 WS-P9 item 7) is advisory under host load. The quiet window decides it
  (§17.153). Nothing here was blessed.

## Sign-off

| Item | Reviewer | Date | Verdict |
| --- | --- | --- | --- |
| WS-P9 mutual tail loops (sections 1–2) | | | |

Requested changes go to the implementer as a new plan item. This packet is then re-issued
against the fixing commit.
