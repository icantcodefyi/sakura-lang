# Sakura Lang runtime ABI v1 (WASM, `sakura_rt`)

**Status:** design specification, version **1.0.0**, agreed before `sakura_lir` or `sakura_rt`
emits code (plan §3.1). Owned by the designer; implemented jointly by M4-LIR (`crates/sakura_lir`)
and M4-RT (`compiler/runtime/rt`, crate `sakura_rt`) and consumed by M4-WASM
(`crates/sakura_codegen_wasm`, `runtime/js/sakura_wasm_host.js`). This document is prose,
tables and byte layouts; it contains no code and is not itself normative language
specification (STABILITY.md: pre-alpha). Controlling decisions: design §17.81–§17.83, §17.88,
§17.92–§17.99. Evidence: [research/08](../../../research/08-m4-wasm-and-verification.md),
[research/09](../../../research/09-m4-abi-and-encoding.md) with
[09-evidence](../../../research/09-evidence/). Every section names the research it rests on or
states its own rationale.

The JS backend's ABI is [`runtime/js/ABI.md`](../js/ABI.md) (v0). Where this document says
"as JS" it means the same observable behaviour as that document, which the parity harness
(M4-TEST) checks through stdout and the §17.80 effect trace.

Terms: **guest** is the WASM module (user code, std and `sakura_rt` linked together); **host**
is the embedding (Node through `sakura_wasm_host.js`, or Wasmtime for the pure corpus). **Word**
is an `i32` holding a linear-memory address or a small enumeration. **Object** is a heap block
with the header of §3. **Ordinary value** is any Sakura value (§13.2 Core types); **execution
state** is frames, handler activations, finalizer entries, tasks and pending requests (§7).

---

## 0. Versioning of this document

- The version is `MAJOR.MINOR.PATCH`. **MAJOR** changes any byte layout, tag number, import
  signature, outcome code or ownership rule already shipped; **MINOR** adds a tag, import,
  section field or layout with the old ones unchanged; **PATCH** is wording. The module records
  the ABI version it was compiled against in the `sakura.debug` section (§12) and as the
  exported immutable global `sakura_abi_version` (`i32`: `MAJOR << 16 | MINOR << 8 | PATCH`).
- The host refuses to instantiate a module whose MAJOR differs from its own, and a module whose
  MINOR is greater than its own (the module may need an import the host lacks). The refusal is
  the driver diagnostic `V8003 abi_version_mismatch` (requested from `sakura_diagnostics`), never
  a trap.
- `sakura_rt`, std and user code are compiled with the same ABI revision (research/08 §3.4);
  the linker rejects a mismatch of `sakura_abi_version` between the runtime object and the
  emitter's expectation.
- A change to this document lands with the decision that caused it in design §17 and, after
  the first emitter exists, with the conformance fixtures that pin the changed bytes. Rationale:
  the two implementers start in parallel from this text alone (plan §3.1); without a frozen
  revision each would drift.

## 1. Target profile

Wasm32, one non-shared linear memory with a declared maximum, MVP tables with `call_indirect`,
multi-value, bulk memory, mutable globals, floats. Nothing else: no reference types beyond MVP
`funcref` tables, no GC, exception handling, tail calls, memory64, threads, SIMD, stack switching
(§17.81; research/09 §1 allowlist: `MUTABLE_GLOBAL | FLOATS | MULTI_VALUE | BULK_MEMORY`,
starting from an empty feature set). The emitter validates every module it produces with
exactly that set and additionally checks structurally (research/09 §1): exactly one memory,
32-bit, non-shared, declared maximum ≤ the configured heap budget (§10); every import is in the
§8 tables with the listed signature; no export other than the §9 list. The `sakura_rt` object
compiled by rustc must pass the same gate; the M4-RT CI job runs it so an LLVM default
(bulk-memory is fine, `sign-ext` and `nontrapping-fptoint` need to be in the allowlist or
disabled) cannot widen the profile silently. If `sign-ext`/`nontrapping-fptoint` turn out to be
needed by rustc output, the allowlist grows by exactly those two with a MINOR bump; they are
MVP-adjacent and supported by both pinned engines.

Trampoline instead of tail calls: research/09 §1 measured 3.9 ms vs 2.0 ms per million steps
on Node and 1.0 ms vs 1.9 ms on Wasmtime for a pure dispatch loop. The profile keeps the
trampoline (§6); tail calls stay out until a measurement on the real differential corpus says
otherwise.

## 2. Scalar representation

| Sakura type | Known-type representation (locals, params, results, fields) | Boxed form (§4) | Source |
|---|---|---|---|
| `Int` | `i64`; every `+ - * / % neg` is checked, `D001` on overflow, `D002` on zero divisor, including `MIN / -1` and `MIN % -1` (`D001`; research/09 §6 note) | `IntBox`, tag 1, payload `i64` at offset 8 | §17.80, §17.83 |
| `Float` | `f64`, IEEE 754 binary64, NaN payloads not canonicalised; `Show`, `Eq`, `Ord`, `Hash` treat all NaNs as one value and `-0.0 == 0.0` as JS does | `FloatBox`, tag 2, payload `f64` at offset 8 | §17.83 |
| `Bool` | `i32`, exactly 0 or 1 | immediate word `0`/`1` tagged as §4 | §17.83 |
| `Unit` | no value; a `Unit`-typed slot in a generic position is the immediate word for `Unit` (§4) | immediate | — |
| `String`, `Bytes`, collections, records, variants, closures, handlers | `i32` pointer to an object | the pointer itself | §17.83 |

Division and remainder truncate toward zero (`i64.div_s`, `i64.rem_s`); research/09 §6
checked 42 operand pairs against the JS runtime. `D003` never occurs on WASM (§17.80).
Checked arithmetic is emitted inline by the LIR as compare-and-branch to the defect path
(§6.5), never as a trap: a WASM trap cannot run finalizers, so a trap of any kind in a Sakura
program is a runtime bug (`D090` is reported by the host from the trap, §8.6).

## 3. Heap objects

### 3.1 Header

Every object starts with an 8-byte header and is 8-byte aligned; address 0 is never allocated
and is the only null (research/09 §3 recommendation; needed by the §17.83 niche).

| Offset | Size | Field | Meaning |
|---|---|---|---|
| 0 | `u16` | `tag` | object kind (§3.2). Values ≥ 0x8000 are reserved for the runtime (`SomeBox`, frames, activations) and can never be produced by a user type or by FFI |
| 2 | `u8` | `class` | size class index (§10) or 0xFF for a page-run allocation |
| 3 | `u8` | `flags` | bit 0 `static` (RC never changes; literals and nullary constructors in the data segment); bit 1 `scoped` (the object is a `@resource` handle, §8.4); bits 2–7 zero |
| 4 | `u32` | `rc` | reference count, number of **owning** links (§7). A fresh object has `rc = 1` held by the creating frame's local. Increment is checked: on 2^32 − 1 the runtime defects `D092 rc_overflow` instead of wrapping. Decrement to 0 releases (§3.5) |
| 8 | — | payload | type-specific, below |

Rationale: the 8-byte `{tag, rc}` pair matches Koka's and Grain's prefix size (research/09 §3)
and the §17.82 wording `{tag, rc}`; the two spare bytes hold the size class so `free` does
not need a size lookup, and flags so `Dup`/`Drop` on a static literal is one branch.

### 3.2 Tags

| Tag | Object | Payload at offset 8 |
|---|---|---|
| 1 | `IntBox` | `i64` |
| 2 | `FloatBox` | `f64` |
| 3 | `String` | `u32 byte_len`, `u32 hash` (0 = not computed; a computed 0 is stored as 1), then UTF-8 bytes; no terminator. `String.len` is `byte_len` (as JS) |
| 4 | `Bytes` | `u32 byte_len`, bytes |
| 5 | `Tuple` | `u32 arity`, `u32 pointer_bitmap_len`, then `arity` **slots** (§3.3) |
| 6 | `Record` | `u32 type_id` (layout table index), slots per the layout table, declaration order |
| 7 | `Variant` | `u32 type_id`, `u32 ctor`, slots of that constructor; a nullary constructor is a static singleton in the data segment, one per `(type_id, ctor)` |
| 8 | `Closure` | `u32 code` (table index of the entry stub, §5), `u32 n_captures`, slots |
| 9 | `ListNode` | persistent vector node: `u32 kind` (0 leaf, 1 branch, 2 root), `u32 count`, up to 32 word children, as the JS `$rt.List` trie |
| 10 | `ListRoot` | `u32 len`, `u32 shift`, `ptr root`, `ptr tail`, `u32 tail_len` (JS `$rt.List` shape) |
| 11 | `MapRoot`, 12 `SetRoot` | `u32 len`, `ptr log` (a `ListRoot` of entries), `ptr hamt`, `u32 holes` (JS `$rt.VMap`/`VSet` shape) |
| 13 | `HamtNode` | `u32 bitmap`, children |
| 14 | `Handler` | `u32 effect_id`, `u32 n_state`, `u32 n_clauses`, state slots (initial values), clause closure pointers |
| 15 | `Secret` | `ptr inner` (a `String` or `Bytes`), never readable by `Show`, `Eq` (`V2032` already forbids), JSON or the host import copy-out (§8.3: the import kind `secret` is the only way out) |
| 16 | `Untrusted` | `ptr inner` |
| 17 | `Handle` | `u32 kind`, `u32 slot`, `u32 generation` (§8.4): `Dir`, `File`, `Body`, socket; `flags.scoped = 1` for `File` and `Body` |
| 18 | `Range` | two `i64` (JS: the tuple `[start, end]`) |
| 0x8000 | `SomeBox` | `slot` payload (§4.3); produced only by the runtime's `some` constructor under `--layout=niche` |
| 0x8001 | `Frame`, 0x8002 `Activation`, 0x8003 `FinEntry`, 0x8004 `Task`, 0x8005 `Request` | execution state, §7; allocated in the activation arena, never in a user slot |

`type_id` indexes the **layout table** (§3.4) that `sakura_lir` emits into the data segment;
`effect_id` indexes the effect table (qualified names, as the JS effect id strings). Both are
module-wide, assigned in a deterministic order (sorted qualified name), so two compilations of
the same program produce identical bytes (AGENTS.md: deterministic output).

### 3.3 Slots

A **slot** is 8 bytes. A slot of statically known scalar type holds the scalar directly (`i64`
or `f64`; `Bool` and `Unit` as `i64` 0/1 and 0); a pointer slot holds the `i32` pointer in the
low 4 bytes and zero in the high 4; a slot of type-variable type holds a boxed word (§4). The
layout table says which. Rationale: one slot size keeps projection one load without a
per-field offset table, and `Int`/`Float` fields never allocate (§17.83 known-type rule).
Memory cost is accepted for v1; M5 may pack.

### 3.4 Layout table

One entry per `TypeId` and per tuple arity used: `u32 size_bytes`, `u32 n_slots`, `u32
pointer_bitmap` (bit *i* set when slot *i* owns a pointer and must be dropped; for n > 32 the
bitmap continues in following words), `u32 scalar_kinds` offset (2 bits per slot: 0 word/boxed,
1 `i64`, 2 `f64`, 3 `Bool`/`Unit`), `u32 name_offset` (for `Show` and the debug section), and
for variants one such row per constructor. `Show`, `Eq`, `Ord`, `Hash`, JSON encode/decode,
`Dup`/`Drop` and the schema codecs all read this table and nothing else (research/08 §6.1:
"centralize layout knowledge"). The LIR validator checks that every `Alloc`/`Proj` agrees
with the table.

### 3.5 Destruction

`Drop(p)`: if `flags.static`, return. Decrement `rc`; if nonzero, return. Otherwise push `p`
on the task's **destruction worklist** (a bounded array in the arena; if full, the runtime
grows it from the heap and, if that fails, releases in place one object at a time without
recursion by iterating its pointer bitmap and pushing children back on the same array as
slots free up). A loop pops, drops each owned child by the bitmap, runs the object's
runtime destructor for tags 15–17 (`Secret` zeroes its bytes; `Handle` retires the host slot,
§8.4), then frees the block. No recursion anywhere (§17.82; research/09 §3). The
million-object linked-list destruction test is an M4-RT gate.

## 4. Boxing at polymorphism boundaries

Research/09 §2; §17.83. A **generic word** is an `i32` with the following encoding, chosen so
that pointers need no translation and the two scalar immediates need no allocation:

| Low 2 bits | Meaning | Payload |
|---|---|---|
| `00` | pointer to an object (8-byte alignment guarantees these bits are zero); `0` is null and appears only as `None` under `--layout=niche` | address |
| `01` | `Bool`/`Unit` immediate | bit 2: 0 = `Unit`, 1 = `Bool`; bit 3: the boolean |
| `10`, `11` | reserved; never produced | — |

`Int` and `Float` box to `IntBox`/`FloatBox` objects (16 bytes). No small-integer tagging:
research/09 §2 shows a 32-bit word has 30 payload bits, a 64-bit carrier has 62 and still needs
spill boxes, and either would make allocation depend on magnitude; `Int` semantics are not
changed for an allocation optimisation (§17.93). Boxing happens at instantiation sites chosen
by `sakura_lir` (a value of known `Int`/`Float` type flowing into a type-variable position, and
the reverse unbox when a generic result flows into a known position); runtime builtins over
`List<A>`/`Map<K, V>` always see generic words. Specialised copies (the JS `Show`/`Schema`
specialisation, `runtime/js/ABI.md` §7) are preserved by the LIR, so a `List<Int>` passed to
a specialised function is still a list of `IntBox`es: v1 does not unbox collections (M5).

### 4.3 `Option` niche (M4-OPT, behind `--layout=niche`)

Default layout: `Option` is an ordinary `Variant` (tag 7, `None` = ctor 0 static singleton,
`Some` = ctor 1). Niche layout, per research/09 §9 and §17.83: `None` is word `0`; `Some(x)`
is `x` itself when `x` is a nonzero word whose object tag is not `SomeBox`; otherwise (`x` is
`0`, or `x`'s tag is `SomeBox`) `Some(x)` allocates a `SomeBox` owning `x`. Projection:
if the tag is `SomeBox`, the payload, else the word itself. Immediates (`Bool`, `Unit`) are
nonzero and never `SomeBox`, so they pass through. The induction in research/09 §9 covers
every depth because an existing `SomeBox` is wrapped again, never counted. Equality, hash,
`Show`, JSON and `Drop` all go through the same two runtime functions; the LIR validator
rejects a direct `Proj::Payload` on an `Option` under niche layout. JSON wire format is
unchanged by layout (`None` is `null`, `Some(x)` is `x`; nested-`Option` round-tripping is the
open compatibility question research/09 §9 names and is a precondition of enabling the niche
by default, plan §3.1 M4-OPT).

## 5. Functions, closures, calling convention

- Every Sakura function compiles to a WASM function with signature `(env: i32, frame: i32) ->
  i32` where `env` is the closure object (0 for a top-level function) and `frame` the current
  **continuation frame** (§6). Arguments and results travel in the frame's slots, not WASM
  parameters: this is what lets the trampoline resume any function at any block without a
  native stack (research/08 §3.2 item 1). The returned `i32` is a **step code** (§6.2).
- A closure (tag 8) is `code` + captured slots. `code` indexes the module's one `funcref`
  table, in deterministic order. Calling a function value: allocate or reuse the callee frame,
  store the arguments, set `frame.code = closure.code`, return step `Call`. The capture list is
  what Core's `Bind::Closure(DefId, Vec<Atom>)` names; the LIR owns the free-variable analysis.
- **Direct style** (as JS `f$d`): a top-level function with row `{}` that the transitive
  no-suspend analysis (research/08 §3.2 item 6) proves never performs, installs, fails or
  calls a non-direct function may additionally be emitted as `(args…) -> result` in native
  WASM parameters with its own WASM stack frame. Its recursion depth is bounded by a counted
  **depth gauge** global (as JS `G.d`, limit 400): past the limit the direct call is rewritten
  by the caller into the frame protocol, so the native stack never overflows. Direct functions
  may raise a defect (§6.5) and nothing else.
- Ownership at a call (research/09 §4, §17.82): arguments are **passed owned**: the caller
  `Dup`s any argument it still uses afterwards and the callee `Drop`s every parameter on every
  terminal path (or transfers it). Results are **returned owned**. Captures are owned by the
  closure and `Dup`ed into the frame when read. The LIR validator checks that every path from
  entry to a terminal has balanced counts for every local, including the defect path.

## 6. Continuation frames, trampoline, outcomes

Research/08 §3.2; §17.81; research/09 §4 ownership table. No tail calls and no native stack
growth across Sakura calls.

### 6.1 Frame layout (tag 0x8001, arena-allocated)

| Offset | Size | Field |
|---|---|---|
| 8 | `u32` | `code`: table index of the function to resume |
| 12 | `u32` | `block`: block id inside that function to resume at (0 = entry) |
| 16 | `u32` | `parent`: **generational frame id** (§7.2) of the caller's frame, 0 for the task root |
| 20 | `u32` | `activation`: id of the innermost handler activation in scope (§6.3) |
| 24 | `u32` | `catch`: id of the innermost catch entry (§6.4) |
| 28 | `u32` | `n_slots` |
| 32 | — | slots: parameters, then locals that live across a call or suspension, as 8-byte slots (§3.3) with a pointer bitmap in the function's frame descriptor (data segment) |

A frame owns its slots (research/09 §4: "frames own ordinary locals"). `parent`, `activation`
and `catch` are **non-owning** ids. Frame ids are generational so a stale id (a frame already
popped by a `fail` that unwound past it) is detected, not dereferenced.

### 6.2 Trampoline

`sakura_rt` exports one driver, `run(task_id) -> i32`. It loops: read the current frame's
`code`, `call_indirect` it with `(env, frame)`, then act on the step code:

| Step | Meaning | Driver action |
|---|---|---|
| 0 `Return` | the frame's result slot holds the owned result | pop the frame, deliver the result to the parent's named slot, resume parent at its `block` |
| 1 `Call` | the function set up a child frame (in `task.next_frame`) | push it, continue |
| 2 `Fail` | `task.error` holds the owned error value | unwind to the innermost catch entry (§6.4) |
| 3 `Defect` | `task.defect` holds `{code, message, spans}` | unwind every finalizer (§6.4), end the task |
| 4 `Suspend` | `task.request` names a pending host request (§8.5) | return to the host with outcome `pending` |
| 5 `Yield` | scheduler point (`Task.yield_now`, fiber start) | pick the next runnable task (§7.4) |

Loop iterations are not counted or limited: bounded work is the host's business (it owns
the heap limit, §10, and the aggregate request limit, §8.5). A frame is reused for a self
tail call (the Core `opt` loop rewrite, as JS) without the driver's involvement.

### 6.3 Handler activations (tag 0x8002)

`Handle` (`with h { body }`) creates an **activation**: `effect_id`, `handler` (owned pointer
to the `Handler` object, whose initial state slots are **copied** into the activation's state
slots as JS `install` does), `outer` (non-owning id of the enclosing activation), `frame`
(non-owning id of the frame that installed it), and the live state slots (owned). Performing
`E.op(args)` walks `outer` from the frame's `activation` to the first activation with that
`effect_id` (as JS `find`; after checking `task.cancelled`, §7.4) and calls the clause closure
with a **clause frame** whose `activation` is the found activation's `outer` (design §5.5:
the clause sees the handlers outside its own frame) and whose `catch` is the performer's.
Operation callbacks (`runtime/js/ABI.md` §3) are closures of tag 8 whose `code` is a runtime
stub that swaps the frame's `activation` to the perform site's id before entering the user
function and restores it after; the perform-site id is stored in a capture slot as a
non-owning id, and the stub checks the generation (research/09 §4 "borrow context ID").
State: `StateGet`/`StateSet` address the activation's slots through the clause's first
parameter, which is the activation id.

### 6.4 Catch and finalizer chain

A `Try` region pushes a **catch entry** on the task's catch stack: `join_block`, `frame_id`
(non-owning), `fin_mark` (the finalizer stack height at entry, as JS `catcher.m`). `fail e`
pops finalizer entries above `fin_mark` and runs each (newest first) with `returned = false`,
pops frames down to `frame_id` (dropping their slots), then resumes the catching frame at
`join_block` with the error in the designated slot. A finalizer entry (tag 0x8003) owns its
cleanup closure and borrows the task; `finalize(fin, body)` pushes the entry, runs `body`,
and on normal return pops and runs it with `returned = true` exactly once (as JS). A defect or
cancellation unwinds the whole stack the same way with `returned = false`. Finalizers run on
the trampoline like any Sakura code (they are closures); a `Fail` inside a finalizer is a
defect `D093 finalizer_failed`, as the JS runtime's behaviour will be pinned to by a shared
fixture (today JS lets it escape; the fixture decides and both backends follow it).

### 6.5 Outcome protocol

A task ends in exactly one of: `ok(value)`, `fail(error)`, `defect(code, message, spans)`,
`cancelled`; a task that is waiting is `pending(request_id)`. The host reads the outcome from
the task record (§7.3) after `run` returns. Defects are **never traps**: the LIR emits the
compare-and-branch and jumps to a shared per-function defect block that fills `task.defect`
(code as a `u16` from the `D` registry, message as a static string offset, the site id from
§12) and returns step 3. Exit: `Cli.exit(code)` is a fourth terminal, `exit(code)`, that
unwinds finalizers of every task and sets the process exit code (as JS `$rt.Exit`).

## 7. Ownership rules (RC without cycles)

Research/09 §4 is the source; this section is the agreement §17.82 asked for. The invariant:
**ordinary values form a DAG and owning links point only from ordinary values to ordinary
values, or from execution state into ordinary values. No ordinary value owns execution
state, and execution state never owns execution state except downward (scheduler → task →
frames/activations/finalizers/requests).** All other links are non-owning generational ids.

### 7.1 Link classification

| Link | Kind | Rule |
|---|---|---|
| frame slot → ordinary value | owning | dropped when the frame pops |
| activation → `Handler` object and live state slots | owning | dropped when the `with` region exits |
| closure capture → ordinary value | owning | dropped with the closure |
| callback closure → perform-site activation | **non-owning id** | checked on call; a stale id is `D094 context_escaped` (cannot happen in checked programs: the WS1 escape rules forbid a callback outliving its scope; the check is defence, research/09 §4 "generation checks detect, not prove") |
| handler state → callback closure → the same activation | broken by the rule above | this is the research/09 §4 counterexample; test `cycle_handler_callback` |
| frame → parent frame, activation, catch entry | non-owning id | |
| catch entry → frame | non-owning id | |
| finalizer entry → cleanup closure | owning | popped before running |
| finalizer entry → task | non-owning id | |
| task → child task ids (scope) | owning of the **slot**, not of the child's values | a scope ends only after every child has an outcome (§7.4) |
| child → parent task | non-owning id | completion notifies by id |
| `Fiber<T>` value → task | non-owning id + generation | `join` on a retired id is `D095 fiber_retired` (unreachable in checked programs; defence) |
| request → continuation frame | owning | moved back to the task on completion or dropped on cancel |
| host completion → request | id + generation + task epoch | §8.5 |
| `Handle` value → host table slot | owning of the slot (retire on drop) | §8.4 |

### 7.2 Generational ids

Execution-state objects live in a per-instance **arena** indexed by `(slot: u24, generation:
u8)` packed in one `u32`; slot 0 is never used. Reuse increments the generation; a slot whose
generation would wrap to 0 is retired for the instance's lifetime (research/09 §5 item 5 applies
the same rule to host handles). Dereferencing an id checks the generation and the expected
tag; a mismatch is `D090` with the site (it is a runtime bug, not a user error, because the
escape rules make it unreachable).

### 7.3 Task record (tag 0x8004)

`epoch: u32` (incremented on cancel, §8.5), `state` (runnable, waiting, done), `outcome`
(§6.5), `frame` (current frame id), `catch_top`, `fin_top` (stack tops), `cancelled: u8`,
`parent` (id), `children` (small owned list of ids), `request` (pending request id or 0),
`seed_state` (for `Task.deterministic`, §11). The task owns its frame, catch, finalizer and
destruction stacks (arena arrays).

### 7.4 Scheduler

One thread. A single run queue in the arena; `Task.deterministic(seed)` orders fiber starts
with the §11 generator; `yield_now` under it does not suspend (as JS). `cancel(task)` marks the
task and its descendants, bumps each epoch, aborts each pending request through the host
(§8.5), and wakes each so it stops at its next effect operation or suspension and runs its
finalizers before `cancel` returns (as JS `cancelTask`). A scope ends only when every child
has an outcome and every request of the scope's tasks is terminal (research/09 §4).

### 7.5 Required cycle and lifetime tests (M4-RT gate; from research/09 §4)

`cycle_handler_callback` (handler state holds a callback that performs the same effect),
`callback_alias_nested_attenuation`, `fiber_self_reference`, `reentrant_host_completion`,
`parent_cancel_during_child_completion`, `finalizer_order_{ok,fail,defect,cancel}`,
`double_completion_rejected`, `close_then_late_completion`, `scope_exit_with_pending_work`,
`million_frame_destruction`, `rc_overflow_is_D092`, `oom_while_unwinding`. Each counts live
objects, arena slots, requests and host handles before and after ten repetitions and
requires equality; sanitizer cleanliness is necessary, not sufficient.

## 8. Host import ABI

Research/09 §5; §17.81 (Node host delegates to the existing JS handlers). The host never
reads a guest object header and the guest never receives a host pointer.

### 8.1 Transport rules

- Scalars cross as WASM scalars; `i64` crosses JS as `BigInt`, never `Number` (research/09 §5
  item 1).
- A **slice** is `(ptr: i32, len: i32)` with `ptr + len ≤ memory.byteLength` checked by the
  host before reading; `(0, 0)` is the empty slice. Strings are UTF-8 (the host validates on
  copy-in from the host side: a non-UTF-8 result is a runtime bug `D090`). Bytes are opaque.
- **Copy-in** (guest → host): the host copies the bytes out of linear memory **before** it
  returns or awaits anything; it never keeps a view across an `await` or a call back into the
  guest (research/09 §3: `grow` detaches the buffer).
- **Copy-out** (host → guest): the host calls the export `alloc_bytes(len) -> ptr` (a `Bytes`
  object with `rc = 1`), **reacquires** its memory view, copies, then passes the pointer to the
  guest as the owned result. If `alloc_bytes` fails (heap limit), it returns 0 and the host
  reports the defect `D096 heap_exhausted` through the completion path. Imports never free
  guest storage (research/09 §5 item 2).
- **Records and variants** cross as **ABI value transport** (research/09 §5 items 3–4): a
  length-prefixed little-endian encoding `sakura-vt1`: `u32 type_id`, then slots in
  declaration order, scalars as 8 bytes, strings/bytes as `u32 len` + bytes, nested objects
  recursively, variants as `u32 ctor` + payload, `Option` always tagged (`u8 0/1`) so
  `Some(None)` survives independent of layout, `Int` as `i64` (lossless; never through
  `JSON.parse`). The host decodes against the layout table (exported as a data-segment range,
  §9) and validates discriminants, UTF-8, lengths, depth ≤ 64, nodes ≤ 2^20 and total bytes ≤
  the §10 copy budget before constructing. `Json` values use the same transport (the std
  `Json` variant), not JSON text, so no precision is lost.
- **Schemas** (JS `emit::schema`) cross as the same string encoding JS uses, so
  `json_encode`/`json_decode`/`cli_parse` keep their exact behaviour by delegating to the JS
  implementations in M4 (the guest-side implementations are an M4-RT stretch goal; the parity
  harness does not care which side runs them).
- `Secret`: an import of kind `secret` (only `http_send`'s `secret_headers` and
  `secret_expose` under `@trust`) receives a slice of the inner bytes; the host zeroes its
  copy after use. No other import may be passed a `Secret` pointer; the LIR validator refuses.

### 8.2 Import table

One module `sakura_host`. The names and arities mirror `sakura_core::builtins::BUILTINS` with
ABI `Cps` that reach the host (the JS runtime's `fs_*`, `file_*`, `http_send`,
`http_stream_*`, `sql_*`, `serve_listen`, `llm_anthropic`, `clock_*`, `random_*`, `env_get`,
`log_write`, `console_print`, `cli_*`, `secret_*`); the exact list is generated from the
`BUILTINS` table by `sakura_codegen_wasm` and checked by a test against `sakura_wasm_host.js`
exports (as the JS builtins test does). Every import has one of three shapes:

| Shape | Signature | Used for |
|---|---|---|
| `sync` | `(args as scalars/slices…) -> i32 status` plus results through `alloc_bytes` copy-out or an out-slot in the frame | `console_print`, `clock_now_ms`, `random_*`, `env_get`, `string_*` helpers that stay on the host in v1, `fs_root`, `fs_subdir`, `file_buffer` |
| `async` | `(request_id: i32, args…) -> i32 accepted` | anything that suspends: `clock_sleep_ms`, `http_send`, `http_stream_open`, `file_read`, `sql_*`, `serve_listen`, `llm_anthropic`; completion comes back through the export `complete` (§8.5) |
| `callback` | the host receives a `callback_id: i32` and later calls the export `invoke(callback_id, args_ptr)`; used only by `serve_listen` and `Task` fibers | reentrancy, §8.5 |

Statuses: `0 ok`, `1 err` (the operation's ordinary `Err(reason)`; the reason string is copied
out), `2 denied` (a `[permissions]` refusal, reported as the operation's ordinary error, as JS),
`3 host_failure` (becomes `D091`).

### 8.3 Permissions and capabilities

The driver passes `SAKURA_PERMISSIONS` to the host exactly as for JS; every host operation
rechecks grants on each call (research/09 §5 item 5: numeric secrecy is not security). A `Dir`
is a `Handle` object (§3.2 tag 17) whose host slot records the resolved directory and its
read/write authority; `fs_subdir` creates a new slot with attenuated authority. `fs_detached_root`
is a slot with no authority. Path rules are the JS ones.

### 8.4 Handle tables

Host side: one table per **kind** (`dir`, `file`, `body`, `socket`, `sql`), entries `(slot,
generation, instance_id, authority, state ∈ {open, closing, closed}, host object)`. Guest
side: the `Handle` object. Every operation on a handle checks kind, instance, generation and
state; a mismatch is the operation's ordinary error `"the file is closed"`/`"the body is
closed"` for `closed` (as JS and §17.88) and `D090` for the others (unreachable from checked
code). A `File`/`Body` handle has `flags.scoped`; when the guest drops it (RC 0) **or** the
introducing operation's finalizer runs (`file_closing`, `http_stream_closing`), whichever is
first, the host slot moves to `closing`, the host resource is closed, then `closed`; reuse of
the slot increments the generation; a slot is retired before its generation wraps. `File`
remains the std `@resource` opaque record `{ id: Int, path, mode }` (§17.89 item 1); on WASM
`id` is the packed `(slot, generation)` and the std code is unchanged.

### 8.5 Asynchronous requests, cancellation and late completions

- The guest allocates a **request** (tag 0x8005: `generation`, `task` id, `task_epoch`,
  `frame` (owned continuation), `kind`, `state ∈ {pending, completed, cancelled}`) and calls
  the `async` import with its packed `request_id`. The host keeps `(request_id, copied
  inputs)` in its closures, never guest pointers (research/09 §5 item 6).
- Completion: the host calls the export `complete(request_id, status, payload_ptr, payload_len)`
  after copying the payload out. The guest checks the request's generation, that `state ==
  pending`, and that `task.epoch == request.task_epoch`; on any mismatch it **discards** the
  payload (drops the `Bytes`), asks the host to close any host resource the completion carried
  (`retire_handle(kind, slot, generation)`), and returns 0; the host then does nothing else.
  Otherwise it marks the request `completed`, moves the continuation frame back to the task,
  stores the decoded result in the frame's result slot, marks the task runnable and returns 1.
  Completion can win exactly once.
- Cancel: `cancel(task)` sets `request.state = cancelled`, drops the owned continuation frame
  (running no user code), bumps `task.epoch`, then calls the import `abort(request_id)` so the
  host aborts the I/O (as JS `suspend(promise, k, cancel)` clears a timer or aborts a fetch).
- Reentrancy: `complete` and `invoke` are only called by the host while the guest is **not**
  running (the trampoline returned `pending` or `yield`). If the host is called back
  synchronously from inside an import (the Node `fetch` abort path can do this), it queues
  the completion and delivers it after the import returns (research/09 §5 "queue completion
  until guest state is consistent").
- Limits: at most `SAKURA_MAX_REQUESTS` (default 4,096) pending requests and
  `SAKURA_MAX_COPY_BYTES` (default 256 MiB) aggregate in-flight copied bytes per instance;
  exceeding either is the operation's ordinary error `"too many pending operations"` /
  `"response too large"`, so a server under load fails requests instead of the process (§10).

### 8.6 Traps and teardown

A WASM trap (impossible in a correct build: every defect is a branch) is caught by the host,
reported as `D090` with the function name from the names section and the PC, and the host
runs its own **resource ledger**: it closes every open handle and aborts every request of
the instance, because guest finalizers cannot run after a trap (research/08 §3.4). The same
ledger runs at process exit.

## 9. Exports

`memory`, `sakura_abi_version` (global), `run(task) -> i32`, `start(entry_fn) -> task`,
`complete(req, status, ptr, len) -> i32`, `invoke(cb, ptr, len) -> i32`, `alloc_bytes(len) ->
ptr`, `drop(ptr)`, `layout_table() -> (ptr, len)`, `effect_table() -> (ptr, len)`, `outcome(task)
-> (kind, ptr, len)` (the outcome encoded as §8.1 transport), `tests() -> (ptr, len)` (the
test list and `effects` map as JS `__tests__.js`, in transport encoding), `stats() -> (ptr,
len)` (§10 counters). Nothing else; the validator rejects extra exports (§1).

## 10. Memory bounds and growth

- `SAKURA_MAX_HEAP` (bytes; default 1 GiB; the manifest may lower it per package in a later
  milestone) is the WASM memory maximum declared in the module **and** the allocator's hard
  limit; the host passes the value at instantiation and refuses a module whose declared
  maximum exceeds it. The allocator grows memory in 64 KiB pages on demand up to the limit.
- Allocator (research/09 §3 recommendation): size classes 16, 32, 48, 64, 96, 128, 192, 256,
  384, 512, 768, 1024, 1536, 2048, 3072, 4096 bytes (geometric, `class` byte in the header),
  one free list each; larger objects are **page runs** (`class = 0xFF`, size stored in a `u32`
  before the header at offset −8 with the header then at an 8-aligned address) from a separate
  bounded region. Every size computation is checked before alignment and growth. Counters:
  `requested`, `live`, `committed`, `peak`, `free_list_bytes` (reported by `stats()`;
  `sakura run --stats` prints them). A dlmalloc comparison harness stays in `runtime/rt/bench`.
- Failed growth or an exhausted limit is **one defect path**: `D096 heap_exhausted` from the
  allocating site, after which the task unwinds (§6.4). The runtime reserves a fixed emergency
  region (64 KiB) so unwinding and the destruction worklist never allocate from the exhausted
  heap; if the emergency region is also exhausted the task ends `defect(D096)` without running
  the remaining finalizers and the host ledger (§8.6) closes resources. Test
  `oom_while_unwinding`.
- JS host view invalidation (research/09 §3): the host holds no `Uint8Array` across any call
  into the guest, any `await`, or any `alloc_bytes`; `sakura_wasm_host.js` acquires a view at
  the start of each import, after each export call, and never stores one in a closure.
  Memory never shrinks.

## 11. Deterministic generation: `sakura-gen-v1`

Research/09 §11; §17.96. One generator protocol shared by the JS runtime (`arbitrary`,
`Task.deterministic`), `sakura_eval` and `sakura_rt`:

- Raw bits: **SplitMix64**: state `s ← s + 0x9e3779b97f4a7c15`; output `z = s`; `z ← (z ⊕ (z
  ≫ 30)) · 0xbf58476d1ce4e5b9`; `z ← (z ⊕ (z ≫ 27)) · 0x94d049bb133111eb`; return `z ⊕ (z ≫
  31)`, all modulo 2^64 (JS `BigInt` with `asUintN(64, …)` after each multiply; Rust
  `wrapping_*`; WASM `i64` ops). From state 0 the first outputs are `e220a8397b1dcdaf`,
  `6e789e6aa1b965f4`, `06c45d188009454f`, `f88bb8a8724c81ec`, `1b39896a51a8749b`
  (research/09-evidence `splitmix.mjs`); this vector is a fixture on all three
  implementations.
- Seed parsing: `--seed N` is a decimal `i64`; the base state is `splitmix(N as u64)` once
  (one step from `N`, so seeds 0 and 1 are far apart).
- Domain separation: the state for property `p`, case `run`, parameter `param` is
  `mix(base, hash64("case") ⊕ run) → mix(·, param)` where `mix(s, x)` is one SplitMix64 step
  from `s ⊕ x`; `Task.deterministic(seed)` uses `mix(base, hash64("task"))`; `hash64` is
  FNV-1a 64 of the ASCII label.
- Derived draws: `below(n)` for `1 ≤ n ≤ 2^63` is unbiased rejection sampling on the top bits
  (`k = ceil(log2 n)` bits, retry while `≥ n`); `float()` is the upper 53 bits divided by
  2^53; `int(lo, hi)` is `lo + below(hi − lo + 1)`; `bool()` is the top bit. Alphabet, size
  schedule, pool reuse (a quarter of `Int`/`Float`/`String` values repeat), refinement retry
  limit (100), discard threshold (90 %), and shrinking order are those of the JS runtime
  today (`runtime/js/ABI.md` §8), restated as the protocol with the SplitMix draws substituted
  one for one.
- Profiles: generated `Int`s for shared JS/WASM fixtures are drawn in the JS-safe range
  (`gen-v1/js-safe`); a `gen-v1/i64` profile exists for WASM-only fixtures (research/09 §11).
- `std/random.seeded` is **not** part of this protocol: it is std code (Park–Miller) that
  already runs identically on every backend because it is compiled, not hosted. It stays.
- Versioning: the protocol id `sakura-gen-v1` is recorded in every property failure message
  and in `sakura test --json`; changing any rule above is `gen-v2`. The old Mulberry sequences
  are not preserved; property fixtures that pin a shrunk counterexample are re-blessed once,
  reviewed one by one (AGENTS.md: review every snapshot diff).

## 12. Debug custom section `sakura.debug`

Research/09 §10; names section plus this section are enough for Sakura-rendered `D0xx` spans
and logical stack traces. Layout (little-endian, version-prefixed):

| Field | Type | Meaning |
|---|---|---|
| `magic` | 8 bytes | `SKRDBG01` |
| `abi_version` | `u32` | §0 packing |
| `module_hash` | 32 bytes | BLAKE3 of the code section; the host checks it against the loaded module and ignores the section on mismatch |
| `n_sources` | `u32` | then `n_sources × (u32 len, UTF-8 uri, 32-byte content hash)` |
| `n_sites` | `u32` | then `n_sites × (u32 source, u32 start_byte, u32 end_byte, u8 role, u32 contract_id or 0)`; `role` as `runtime/js/ABI.md` §5 (`primary`, `definition`, `caller`) |
| `n_funcs` | `u32` | then `n_funcs × (u32 func_index, u32 name_offset, u32 site)` |
| `n_blocks` | `u32` | then `n_blocks × (u32 func_index, u32 block, u32 site)` for every continuation block, so a frame's `(code, block)` maps to a source span |

Every `Check` carries its site id into the defect record (§6.5); every frame's `(code, block)`
gives a span, so the host renders the logical task stack by walking `frame.parent` ids
through this table (research/09 §10: "reconstruct the logical task stack from those
frames"). The section is bounds-checked before use. `sakura build --strip` omits it; stack
traces then show function names and block numbers. DWARF is out of scope (plan §3.2 adds
it to the list).

## 13. Diagnostics introduced by this ABI

To be registered in `sakura_diagnostics::codes` by REQUESTS entry (never constructed before
the runtime exists): `V8003 abi_version_mismatch` (driver), `D092 rc_overflow`, `D093
finalizer_failed`, `D094 context_escaped`, `D095 fiber_retired`, `D096 heap_exhausted`. `D094`
and `D095` are defence in depth: unreachable from a program that passes the WS1 escape check,
tested only through the runtime's own unit tests with hand-built frames.

## 14. Conformance fixtures this document owns

`compiler/tests/abi/` (M4-TEST): byte-exact dumps of the layout table and one object of each
tag for a fixed program (`insta` snapshots, reviewed by hand); the SplitMix vector; the
transport encoding of each std record; a `sakura.debug` section round trip; the §7.5 cycle
and lifetime list; and the hostile-host tests from research/09 §5 (wrong-kind, stale and
foreign handles; invalid pointer and oversized length slices; non-UTF-8 strings; late and
double completions; cancellation races; shutdown with pending work).
