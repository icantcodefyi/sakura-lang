# Concurrency

**Draft (normative at v1.0.0).**

Informative scope: this chapter covers structured concurrency (`std/task`), cancellation,
deterministic scheduling, bounded channels (`std/channel`), and the `D090` defect a run ends with
when every task waits.
Its sources are [design §8.6–§8.7](../design.md), [RFC 0002 §2.8–§2.9](../rfcs/0002-web-platform-apis.md)
and design §17.171, §17.259 (G23), §17.280 (4) and §17.296.
Handlers are in [effects](effects.md) and failures in [errors](errors.md).
[Draft conventions](README.md) identify normative material; rules are labelled **CCnn**.

Every backend runs one Sakura instance on one thread (design §17.171): tasks interleave at
effect operations, they do not run in parallel, and no mutable value is shared between them.

## Scopes and fibers

**CC01** `Task.scope(body)` does not return until every fiber spawned in its `Scope` has ended, so its body may finish while children still run. [scope_failure_after_cancelled_sibling](../../compiler/tests/semantics/scope_failure_after_cancelled_sibling.sak), [every stops on cancel](../../compiler/tests/semantics/task_every_stops_on_scope_cancel.sak).

**CC02** `Task.join(f)` waits for the fiber and evaluates to its value, and a child's defect reaches the joiner with its code and message. [tasks_run_concurrently_and_cancel_losers](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [deterministic_tasks_give_input_order_results](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [D030 through join](../../compiler/tests/semantics/defect_D030_joined_fiber.sak).

**CC03** When a child fails, its scope cancels the other children, waits for them, and then propagates that first failure rather than a sibling's cancellation. [scope_failure_after_cancelled_sibling](../../compiler/tests/semantics/scope_failure_after_cancelled_sibling.sak), [every stops on cancel](../../compiler/tests/semantics/task_every_stops_on_scope_cancel.sak), [a failing `Task.all`](../../compiler/tests/semantics/handler_cancel_in_loop.sak).

**CC04** Spawned fibers perform under the handlers installed where they were spawned. [Virtual clock in children](../../compiler/tests/semantics/scope_failure_after_cancelled_sibling.sak), [channel tasks](../../compiler/tests/semantics/channel_bounded_backpressure.sak).

**CC05** `Task.all` evaluates to its bodies' results in input order, `Task.race` to the result of the first body to finish with the others cancelled, and `Task.timeout` fails with `Timeout { limit }` when its action has not finished in time. [tasks_run_concurrently_and_cancel_losers](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [deterministic_tasks_give_input_order_results](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [timeout](../../compiler/tests/semantics/virtual_timeout_stream_read.sak).

## Cancellation

**CC06** Cancellation is cooperative: a cancelled task stops at its next effect operation, including a perform bound to a handler in the same function. [handler_cancel_in_loop](../../compiler/tests/semantics/handler_cancel_in_loop.sak), [handler_cancel_bound](../../compiler/tests/semantics/handler_cancel_bound.sak), [handler_cancel_twin](../../compiler/tests/semantics/handler_cancel_twin.sak).

Informative: a loop that performs nothing is not interruptible (design §8.6).

**CC07** `Task.cancel(f)` unwinds the fiber through its finalizers before it returns, the cancelled fiber's outcome is not a failure of its scope, cancelling a finished or already cancelled fiber does nothing, and a later `Task.join(f)` is the defect `D095`. [Finalizers](../../compiler/tests/semantics/task_cancel_fiber_runs_finalizers.sak), [no-op](../../compiler/tests/semantics/task_cancel_finished_noop.sak), [D095](../../compiler/tests/semantics/task_cancel_then_join_is_d095.sak).

**CC08** `Task.cancelled(s)` is `true` once the scope `s` is cancelling and `false` before. [every stops on cancel](../../compiler/tests/semantics/task_every_stops_on_scope_cancel.sak), [no-op](../../compiler/tests/semantics/task_cancel_finished_noop.sak).

## Determinism and time

**CC09** Under `Task.deterministic(seed)` the interleaving of tasks, and so the program's output and effect trace, is the same on every run with that seed and on every backend. [channel_bounded_backpressure](../../compiler/tests/semantics/channel_bounded_backpressure.sak), [js_and_wasm_agree_on_the_semantics_corpus](../../compiler/crates/sakura_cli/tests/parity.rs), [the task stream matches JS](../../compiler/runtime/rt/tests/generate.rs).

**CC10** Under `Clock.virtual()` time advances only when every live task is waiting on a sleep, and sleepers wake in order of their virtual deadlines. [deterministic_clock_random](../../compiler/tests/semantics/deterministic_clock_random.sak), [virtual_sleeps_wake_in_virtual_time_order](../../compiler/crates/sakura_codegen_js/tests/m3.rs), [a host request in flight](../../compiler/tests/semantics/virtual_timeout_stream_read.sak), [virtual_time_waits_for_a_live_host_request](../../compiler/runtime/rt/tests/exec.rs).

**CC11** `Random.seeded(n)` gives the same sequence of draws on every run and backend. [deterministic_clock_random](../../compiler/tests/semantics/deterministic_clock_random.sak), [js_and_wasm_agree_on_the_semantics_corpus](../../compiler/crates/sakura_cli/tests/parity.rs).

## Channels

**CC12** `with_channel(capacity, fn(tx, rx) { … })` creates a channel of at most `capacity` queued items for the callback, and `with_channel(0, …)` is the caller's defect `D020`. [Backpressure](../../compiler/tests/semantics/channel_bounded_backpressure.sak), [D020](../../compiler/tests/semantics/channel_rejects_capacity.sak).

**CC18** A negative capacity is the caller's defect `D020` too. [Negative capacity](../../compiler/tests/semantics/channel_rejects_negative_capacity.sak).

**CC13** `send` waits while the channel is full, `recv` waits while it is empty, items are received in the order sent, and `try_send` never waits, answering `Err(Full)` instead of queueing. [Backpressure](../../compiler/tests/semantics/channel_bounded_backpressure.sak), [try_send](../../compiler/tests/semantics/channel_try_send_full.sak).

**CC14** After `close`, the items already queued are still received in order, then `recv` is `None` and stays `None`, every later send is `Err(Closed)`, a send waiting on a full channel is woken with `Err(Closed)`, and a second `close` does nothing. [close](../../compiler/tests/semantics/channel_close_drains_then_none.sak), [try_send after close](../../compiler/tests/semantics/channel_try_send_full.sak).

**CC15** A task waiting in `send` or `recv` is a cancellation point: cancelling it wakes it without taking or queueing an item, and the other waiters are served as before. [channel_cancel_wakes_waiters](../../compiler/tests/semantics/channel_cancel_wakes_waiters.sak).

**CC16** A channel's ends are resources, so neither may leave the callback or be held in handler state (`V6003`). [Sender escape](../../compiler/tests/ui/channel_sender_cannot_escape.sak), [handler state](../../compiler/tests/ui/channel_receiver_cannot_be_handler_state.sak).

## When nothing can run

**CC17** When every task is waiting on a channel and nothing can wake one, the run ends with the defect `D090` instead of hanging. [channel_recv_every_task_waiting](../../compiler/tests/semantics/channel_recv_every_task_waiting.sak).

**CC19** The same holds for a `send` waiting on a full channel that nothing drains and for tasks each waiting in `recv` on a channel only another waiting task would send on. [Full send](../../compiler/tests/semantics/channel_send_full_every_task_waiting.sak), [two channels](../../compiler/tests/semantics/channel_two_tasks_wait_on_each_other.sak).

Informative: `D090` is also the runtime's code for an internal fault, such as a malformed host
value ([malformed_values_are_d090_and_leak_nothing](../../compiler/runtime/rt/tests/vt.rs)) or
an operation past a handler's clauses; those are defenses, not reachable from checked source.

## Workers (experimental, informative)

`std/worker` is experimental (STABILITY.md), so this section is informative although each
statement has a fixture: a worker is a second instance with its own heap whose messages are
copied through the `Schema` codec, so a value the codec cannot carry arrives as
`Failed(WorkerError { reason: Codec })`
([worker_rejects_codec](../../compiler/tests/semantics/worker_rejects_codec.sak));
`Worker.inline()` runs the protocol deterministically
([worker_inline_roundtrip](../../compiler/tests/semantics/worker_inline_roundtrip.sak));
at most 16 workers are live per instance
([worker_limit_16](../../compiler/tests/semantics/worker_limit_16.sak));
a worker's grants must be a subset of its parent's
([worker_grants_subset_rejected](../../compiler/tests/semantics/worker_grants_subset_rejected.sak));
and on WASM and native a `main` that declares `Worker` exits 75 at startup
([worker_host_refused_off_js](../../compiler/tests/semantics/worker_host_refused_off_js.sak)).
