# WS-P2 and WS-P2b review packet (handler dispatch)

For: **@icantcodefyi** (CODEOWNERS for `/compiler/`). Prepared 2026-10-18 by an AI agent (Claude
Opus 5.5) at design §17.181 (3). The human reviewer owns the verdict.

**Status: awaiting review. Not approved.** Signing the table at the bottom closes plan
[`performance-r1.md`](../plans/performance-r1.md) §5 item 6 for WS-P2 and WS-P2b together. The
earlier WS-P2 approval (plan §8, WS-P2 item 12) predates WS-P2b and does not cover it. This
packet changes no code.

Commits under review:

| Item | Commits | Design |
| --- | --- | --- |
| WS-P2: bound performs, tail-direct dispatch, the current-task register | `0d30acf5` (`67155a04`, one-use temporaries, landed with it and moves no dispatch code) | §17.113, §17.156 (1) |
| WS-P2b: resumable `for (;;)` loops for unbound performs | `0df074aa`, `f2965cae`, `0c723feb` | §17.156 (2) |
| WS-P2b step bound: at most 8 CPS steps per resumable loop body | the `perf(codegen)` commit that adds `loop_resume_bound` | §17.181 (1) |

Why the review: neither change reorders an observable effect, but both move the code that
makes an operation a cancellation point (§8.6) and writes its §17.80 trace record. The
question for the reviewer is whether, on every path, that code still runs exactly once per
operation and before the clause.

---

## 1. Where the cancellation test and the trace record run

Before WS-P2, every perform called `$rt.find(ctx, E, op)` and then the clause through the CPS
ABI. `find` (`compiler/runtime/js/sakura_runtime.js`) does both duties, in this order: throw
the cancellation marker if `ctx.t.cancelled`; walk the handler list (innermost wins); append
the trace record if tracing is on. WS-P2 and WS-P2b add three paths beside it.

**(a) Bound perform (WS-P2, §17.113 (a)).** A perform lexically inside `with h { … }` over a
handler literal, whose clause is direct, calls the clause's twin on the instance. There is no
`find`, so the emitter writes one guard first:

```js
if ($T.t.cancelled || $tr) $rt.bound("main::m::Counter", 0);
```

`$rt.bound` throws the cancellation marker if the current task is cancelled, then writes the
same trace record `find` writes. `$T` is the current-task register `$rt.T`. The driver
(`drive`) sets it on every task start and resume and restores it in `finally`. It holds task
identity only; handlers are never looked up through it. `$tr` is `$rt.TRACE`, which is true when
tracing was on at load. `bound` checks the trace file again, so after the WASM host's
`stopTracing` a bound perform takes the slow call and writes nothing. Performs inside a lambda
or a callback are never bound (§17.113 (a)), so they keep path (b) or the original `find`.

**(b) Tail-direct dispatch (WS-P2, §17.113 (b)).** An unbound perform in CPS code whose
operation takes no function-typed parameter calls `find` exactly as before, which does the
cancellation test and the trace. Then, if the clause has a twin, it calls the twin inline;
otherwise it takes the CPS clause with `$rt.cctx`. Both paths share one continuation. The
order of the two duties relative to the clause does not change: `find` runs first.

**(c) Resumable loop (WS-P2b, §17.156 (2)).** In a join-point loop whose only CPS steps are
unbound performs or `cps` builtins, each perform is path (b) with the continuation split:

- On the *fast path* the twin's result continues the iteration inside `jL: for (;;)`, and the
  back edge is `continue`. Each perform still calls `find` once, so the cancellation test and
  the trace record happen once per operation, in program order.
- On the *slow path*, when the clause is CPS or the builtin suspends, the continuation is
  `$sN(live…)`, emitted once after the loop. It runs the rest of the iteration as ordinary CPS
  code, and its back edge calls `jL`, which counts one gauge frame and may bounce.

A fast iteration counts no frame and never bounces. Removing bounces cannot change scheduling:
the driver resumes the same task after a bounce (`drive`'s `Bounce` arm), and only `Suspend`
yields to another task. A task is cancelled only while it is suspended (§17.156 (1)), so the
first point that observes a cancellation is the resumption, and the next perform's `find`
tests the flag again.

**(d) The step bound (§17.181 (1)).** The code after CPS step k is emitted k + 1 times: once
inline, and once in the slow path of each step up to k, because each `$sN` runs the rest of
the iteration as ordinary CPS code. The body's last statements therefore appear at most
nine times under the bound. The emitted `eight` in `loop_resume_bound` has eight slow paths
and 36 `$rt.find` sites (1 + 2 + … + 8). A loop body with more than 8 CPS steps keeps the
pre-WS-P2b shared-continuation form, a join-point function under the gauge. `--dump direct` prints
`<def> loop jN shared(too-many-steps(n))` for it, and `<def> loop jN resumable` for a loop that
keeps the loop form. The bound is a count of steps in the source, not a byte budget, so
eligibility does not change with unrelated edits to the body. Neither form changes where the
cancellation test or the trace record runs: both are path (b).

Unchanged: `--no-direct` emission (every perform uses `find` and the CPS clause), the WASM
backend's dispatch, finalizers (no direct clause registers one), `Check` binds and blame sites.
Each is emitted exactly as before on every path.

## 2. Invariants and the fixtures that pin them

Every `tests/semantics` fixture runs under the direct ABI and under `--no-direct`. Both must
meet the expectations, and the two effect traces (§17.80) and, for programs, stderr must be
byte-identical (`compiler/crates/sakura_cli/tests/semantics.rs`). `tests/parity.rs` holds WASM
to the same traces. Each fixture below is therefore also a trace-parity check.

| Invariant | Source | Pinned by |
| --- | --- | --- |
| A cancelled task never runs a bound perform's clause; every bound perform is a cancellation point | §17.156 (1), plan §5 item 3 | `runtime.test.js` "a bound perform of a cancelled task is a cancellation point"; `handler_cancel_bound` (cancellation, finalizer and trace records equal to the unbound form) |
| The driver sets and restores the current-task register per segment | §17.113 (a) | `runtime.test.js` "the driver sets the current-task register for each segment and restores it" |
| A cancellation in a resumable loop unwinds at the resumption | §17.156 (1), (2) | `handler_cancel_in_loop` (`spin` is `resumable`), `handler_cancel_bound` (`main$1` is `resumable`) |
| Lookup order and nested instances: innermost wins, each install starts fresh | §17.113 invariants | `million_operations` (`resumable`), `handler_state` (nested `with counter()`), `runtime.test.js` "nested instances: innermost wins …" |
| A clause that performs its own effect reaches the next outer handler | §17.113 invariants | `handler_forwarding`, `loop_resume_twice` (the forwarding arm), `runtime.test.js` "a clause performing its own effect reaches the next outer handler" |
| Callbacks run under the performer's handlers; re-entry through a callback is CPS | §17.113 invariants, §5.5 | `handler_callback_context`, `loop_resume_edges` (a callback-typed operation in a resumable loop) |
| A clause that may `fail` has no twin | §17.113 invariants | `handler_clause_fail`, `loop_resume_edges` (`fail` in a resumable loop) |
| Handler state written by a clause is visible to the next read on both paths | §17.113 invariants | `handler_state`, `handler_lexical_bind`, `loop_resume_twice` (state after each of two suspensions in one iteration) |
| Loop state, straight-line and branching inner joins survive a resumption | §17.156 (2) | `loop_resume_twice`; `loop_resume_edges` (early `return`, `match`, `break`, `continue`) |
| Only loops within the bound take the loop form; the rest keep the shared form with the same output and trace | §17.181 (1) | `loop_resume_bound` (nine steps: `shared(too-many-steps(9))`; eight: `resumable`); `dump_direct_names_loops_over_the_step_bound` in `sakura_cli/tests/cli.rs` |
| `handled_fwd` and `handled_suspend` emit the loop form | §17.156 (2) acceptance | emitted-JS snapshots `source__handled_fwd.snap`, `source__handled_suspend.snap` (`sakura_codegen_js/tests/source.rs`) |
| Finalizers are untouched | §17.113 invariants | `handler_cancel_bound` (finalizer records) |

## 3. Known limits and open notes

- The cancellation branch of a bound perform cannot be reached from a program today. A running
  JS task cannot become cancelled, so only the runtime unit test reaches it. This is the
  reason for §17.156 (1)'s rewording, not a gap in the fixtures.
- WS-P2b deviation 2 calls `loop_resume_twice`'s `spin` a five-step loop. Counted as the bound
  counts (each unbound perform or `cps` builtin call in the body), it has six: two
  `Counter.next`, two `Clock.sleep`, `Clock.now` and `Console.print`. It emits six `$sN`
  functions and stays `resumable` under the bound of 8.
- The bound limits emitted size; it is not a measured optimum. Plan §5 item 4 (the 2× module
  budget on `micro.sak`) remains the size gate. Outside `loop_resume_bound`, no semantics
  fixture, example or benchmark loop has more than six steps (`loop_resume_twice`).
- Performance numbers (WS-P2b deviations 5, 7 and 8) are advisory under host load. The quiet
  window decides them (§17.153). Nothing here was blessed.

## Sign-off

| Item | Reviewer | Date | Verdict |
| --- | --- | --- | --- |
| WS-P2 and WS-P2b handler dispatch (sections 1–2) | | | |

Requested changes go to the implementer as a new plan item. This packet is then re-issued
against the fixing commit.
