# Sakura Lang runtime ABI v1 (WASM, `sakura_rt`)

**Status:** design specification, version **1.2.1**, agreed before `sakura_lir` or `sakura_rt`
emits code (plan §3.1); 1.0.1 folds in the R1 errata that design §17.108 endorsed, 1.0.2 the
R3 readings P8–P12 that design §17.129 endorsed, P9 amended (both lists are kept as history at
the end, with the questions still open), and 1.1.0 the four `stats()` counters and the
direct-style twins of design §17.118 (plan item L4), 1.1.1 the gate 10 erratum to §8.1 and
§8.4 (design §17.159; the plan's "1.0.3"), 1.1.2 the hostile-host hardening of plan WS12
item 8 (§8.1, §8.4–§8.6), 1.1.3 the defect sites of plan L5 (§5, §6.5, §12; design §17.162), and 1.2.0 the `restarts`
counter, the import and handle-kind lists as built and §5's fallback wording (design §17.160,
§17.161; plan M4-OPT), and 1.2.1 the fail-closed copy budget of §8.5 and its reading as a per-instance ceiling (design §17.209). Owned by the designer; implemented jointly by M4-LIR (`crates/sakura_lir`)
and M4-RT (`compiler/runtime/rt`, crate `sakura_rt`) and consumed by M4-WASM
(`crates/sakura_codegen_wasm`, `runtime/js/sakura_wasm_host.js`). This document is prose,
tables and byte layouts; it contains no code and is not itself normative language
specification (STABILITY.md: pre-alpha). Controlling decisions: design §17.81–§17.83, §17.88,
§17.92–§17.99, §17.103 (fast by default), §17.108 (1.0.1), §17.129 (1.0.2). Evidence: [research/08](../../../research/08-m4-wasm-and-verification.md),
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
- `sakura_rt`, std and user code are compiled with the same ABI revision (research/08 §3.4).
  As built (1.2.0): the emitter and the runtime object take the revision and the counter count
  from one file, `runtime/rt/src/abi.rs`, which `sakura_codegen_wasm` includes, and a test of
  that crate holds the Node host's `ABI_VERSION` and counter list to it.
- 1.0.1 changes bytes (the `Handler` payload, the tuple's second word, the table formats) as a
  PATCH because nothing had been emitted against 1.0.0 (design §17.108); from the first emitted
  module on, such a change is MAJOR as stated above.
- 1.0.2 states the return register and `env` convention (§5), the outcome numbers and the
  defect message and site (§6.5, §9), the `ok_handle` completion status and runtime-side
  queueing (§8.2, §8.5) and cancellation and finalizer semantics (§6.4, §7.4), as design
  §17.129 endorsed P8–P12. It is a PATCH for the same reason as 1.0.1: no module has been
  emitted. `sakura_abi_version` was `0x010002`.
- 1.1.0 (MINOR, design §17.118) appends four counters to `stats()` (§10: `frames`, `boxes`,
  `dups`, `drops`; the export now returns 80 bytes) and states how direct style (§5) is
  compiled. Nothing shipped changes: the six counters keep their offsets, and twins are
  internal functions outside the import and export lists. `sakura_abi_version` is `0x010100`.
- 1.1.1 is an erratum (design §17.159, security review W0, W5, W12; plan WS12 item 7 calls it
  1.0.3, written before 1.1.0 landed): §8.1 lists the kind-`secret` imports as built and says
  what the host can and cannot erase; §8.4 says what is built for generations and `file` ids.
  It is wording that matches the shipped modules and hosts (PATCH); no byte, import signature
  or check changes, so `sakura_abi_version` stays `0x010100`.
- 1.1.2 is an erratum (security review W1–W4, W7–W11; plan WS12 item 8; design §17.159 (5),
  §17.161): it states the checks the guest and the Node host now make on each other (§8.1),
  the release of host-built instances and callbacks (§8.4), one policy for a full request
  table and the copy budget as built (§8.5), and a fresh instance after a trap (§8.6). No
  shipped byte or import signature changes: `retire_handle` gains kinds 4 and 7, which a host
  without them ignores, and the handle-kind list itself is restated in M4-OPT's 1.2.0
  (§17.161), so `sakura_abi_version` stays `0x010100`.
- 1.1.3 is an erratum (design §17.162, plan §3.8 row 5, L5): §12's section is now filled with
  its sources and sites, and §5, §6.5 and §12 state how a contract check's site reaches the
  defect record and how a host renders it as the JS backend's spans. It needs no new byte
  layout, import or export: the section's layout, the defect outcome's `site` word and
  `rt_raise`'s `site` argument are those 1.0.2 defined (a host that ignores the section prints
  the record without spans, as before); the trailing site parameter is a parameter of
  internal functions, which no export or import names. `sakura_abi_version` stays `0x010100`.
- 1.2.0 (MINOR, design §17.160 (2), (4), §17.161, §17.191 (1)) appends an eleventh counter,
  `restarts`, to `stats()` after `drops` (§10; the export now returns 88 bytes), and restates
  three things as built: §5's gauge fallback (the caller runs the call again through frames;
  it does not rewrite it), §8.2's import list (the W1, W2 and L4 imports, the fourth shape
  `hosted`) and §8.4's handle kinds 0–7 with the kinds `retire_handle` receives. Nothing
  shipped changes: the ten counters keep their offsets, so a 1.2 host reads a 1.1 module's ten
  (`stats()` returns its length), and a 1.1 host refuses a 1.2 module by the MINOR rule above.
  `sakura_abi_version` is `0x010200`.
- 1.2.1 is an erratum (design §17.209 (1), (3)): §8.5 states how the hosts read
  `SAKURA_MAX_COPY_BYTES` (a positive integer or the defect `D091`, never unlimited) and that the
  budget is a per-instance memory ceiling, not a byte-exact contract. A failure mode is
  specified; no byte, import or export changes, so `sakura_abi_version` stays `0x010200`.
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
§8 tables with the listed signature; no export other than the §9 list. The gate is normative on
the **linked** module (runtime, std and user code, §9); the `sakura_rt` object passes the same
checks on its own first (`runtime/rt/tests/wasm_gate.rs`), so an LLVM default cannot widen the
profile silently.

**Building the runtime object.** LLVM's default wasm32 features include `reference-types`, whose
`call_indirect` encoding (a padded multi-byte table index) the MVP validator rejects although no
reference-typed instruction is used. `sakura_rt` is therefore compiled with
`-Ctarget-cpu=mvp -Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory`, fat LTO and one
codegen unit, so libcore's embedded bitcode is recompiled under exactly the profile; M4-WASM
builds the same way. `sign-ext` and `nontrapping-fptoint` are not needed, so the allowlist stays
as written.

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
| 4 | `u32` | `rc` | reference count, number of **owning** links (§7). A fresh object has `rc = 1` held by the creating frame's local. Increment is checked: a count may reach 2^32 − 1, and the increment from 2^32 − 1 is `D092 rc_overflow`, leaving the count unchanged. Decrement to 0 releases (§3.5); while an object is being released the field links the destruction worklist |
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
| 5 | `Tuple` | `u32 arity`, `u32 type_id` of the tuple's **layout row** (one row per tuple *shape* used, §3.4), then `arity` **slots** (§3.3) at offset 16 |
| 6 | `Record` | `u32 type_id` (layout table index), 4 bytes padding, slots per the layout table, declaration order, at offset 16 |
| 7 | `Variant` | `u32 type_id`, `u32 ctor`, slots of that constructor at offset 16; a nullary constructor is a static singleton in the data segment, one per `(type_id, ctor)` |
| 8 | `Closure` | `u32 code` (table index of the entry stub, §5), `u32 n_captures`, slots at offset 16, described by the code descriptor of `code` (§3.4) |
| 9 | `ListNode` | persistent vector node: `u32 kind` (0 leaf or tail, 1 branch; see P4), `u32 count`, up to 32 word children, as the JS `$rt.List` trie |
| 10 | `ListRoot` | `u32 len`, `u32 shift`, `ptr root`, `ptr tail`, `u32 tail_len` (JS `$rt.List` shape) |
| 11 | `MapRoot`, 12 `SetRoot` | `u32 len`, `ptr log` (a `ListRoot` of entries), `ptr hamt`, `u32 holes` (JS `$rt.VMap`/`VSet` shape) |
| 13 | `HamtNode` | `u32 bitmap`, children |
| 14 | `Handler` | `u32 effect_id`, `u32 type_id` (the layout row of its state), `u32 n_state`, `u32 n_clauses`, state slots (initial values) at offset 24, then `n_clauses` 4-byte clause closure pointers. The state's types are known where the `handler` expression is compiled, so an `Int`/`Float` state is an unboxed slot and `StateGet`/`StateSet` never box (design §17.108); a type-variable state is kind 0 as everywhere |
| 15 | `Secret` | `ptr inner` (a `String` or `Bytes`), never readable by `Show`, `Eq` (`V2032` already forbids), JSON or the host import copy-out (§8.3: the import kind `secret` is the only way out) |
| 16 | `Untrusted` | `ptr inner` |
| 17 | `Handle` | `u32 kind`, `u32 slot`, `u32 generation` (§8.4, kinds as listed there): `Dir`, `Body`, a JS value; `flags.scoped = 1` for `Body` |
| 18 | `Range` | two `i64` (JS: the tuple `[start, end]`) |
| 0x8000 | `SomeBox` | `slot` payload (§4.3); produced only by the runtime's `some` constructor under `--layout=niche` |
| 0x8001 | `Frame`, 0x8002 `Activation`, 0x8003 `FinEntry`, 0x8004 `Task`, 0x8005 `Request` | execution state, §7; allocated in the activation arena, never in a user slot |

`type_id` indexes the **layout table** (§3.4) that `sakura_lir` emits into the data segment;
`effect_id` indexes the effect table (qualified names, as the JS effect id strings). Both are
module-wide, assigned in a deterministic order (sorted qualified name), so two compilations of
the same program produce identical bytes (AGENTS.md: deterministic output).

### 3.3 Slots

A **slot** is 8 bytes and starts at an 8-byte boundary of the object (offset 16 for tuples,
records, variants and closures, 24 for handler state, 32 for frames). A slot of statically known
scalar type holds the scalar directly (`i64` or `f64`; `Bool` as `i64` 0/1); a `Unit` slot holds
the `Unit` immediate word (§4), so it is kind 0 and dropping it is a no-op; a pointer slot holds
the `i32` pointer in the low 4 bytes and zero in the high 4; a slot of type-variable type holds
a generic word (§4). The layout table says which. Rationale: one slot size keeps projection one load without a
per-field offset table, and `Int`/`Float` fields never allocate (§17.83 known-type rule).
Memory cost is accepted for v1; M5 may pack.

### 3.4 Layout table

One entry per `TypeId`, per tuple shape used and per handler state shape. Bytes (little-endian
`u32`; every offset is relative to the table's start, 0 = absent; a name is `u32 len` + UTF-8):

- **Header:** `n_types`, `option_type_id`, `result_type_id`, `pair_type_id` (the tuple row
  `(word, word)`), `int_pair_type_id` (the tuple row `(i64, word)`); runtime builtins construct
  these (`list_get`, `map_entries`, `list_zip`, `list_enumerate`); `0xFFFFFFFF` when the program
  has no such type. Then a directory of `n_types` entry offsets.
- **Entry:** `kind` (0 record, 1 error record, 2 tuple, 3 variant; `Show` prints a field-less
  error tag as `Name` and a field-less record as `Name {}`, as JS), `name`, `n_rows`, then
  `n_rows` row offsets (one row; one per constructor for a variant).
- **Row:** `u32 size_bytes`, `u32 n_slots`, `u32 pointer_bitmap` (bit *i* set when slot *i*
  owns a pointer and must be dropped; `max(1, ⌈n/32⌉)` words), `u32 scalar_kinds` offset (2 bits
  per slot, slot *i* at bits 2·(*i* mod 16) of word *i*/16: 0 word, 1 `i64`, 2 `f64`, 3 `Bool`),
  `u32 name_offset` (the type, or the constructor, for `Show` and the debug section), `u32
  field_names` offset (`n_slots` name offsets; 0 for tuples and constructors).

**Code descriptor table** (`code` is the funcref table index, §5): `u32 n_codes`, `n_codes` entry
offsets (0: none), each entry `name, n_frame_slots, frame_kinds, n_captures, capture_kinds`
(kinds as above; a kind-0 slot owns its word). Frames (§6.1) and closures (tag 8) are described
by it. `Show`, `Eq`, `Ord`, `Hash`, JSON encode/decode,
`Dup`/`Drop` and the schema codecs all read this table and nothing else (research/08 §6.1:
"centralize layout knowledge"). The LIR validator checks that every `Alloc`/`Proj` agrees
with the table.

### 3.5 Destruction

`Drop(p)`: if `p` is not a pointer (an immediate or 0) or `flags.static`, return. Decrement
`rc`; if nonzero, return. Otherwise push `p` on the **destruction worklist**, a stack threaded
through the `rc` fields of the objects being released (a count of zero has no other use), so
destruction never allocates. A loop pops, decrements each owned child by the layout (pushing
the ones that reach zero), runs the object's runtime destructor for tags 15–17 (`Secret` zeroes
the bytes of an inner `String`/`Bytes` that dies with it; `Handle` retires the host slot,
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
- **Return register** (design §17.129 P8). A callee leaves its owned result, and an unwinding
  `Fail` caught by a catch entry its owned error (§6.4), in the task's 8-byte return register;
  the resume block of the frame that receives it takes the register first. The driver never
  drops the register's contents, because it cannot know whether the bits are a word. A task's
  result is a generic word (§4). `env` is passed on a function's **entry** call only; a
  function that needs it after its first call or suspension keeps it, counted, in a slot. The
  `frame` argument is the frame's address, a transient borrow for that one call; every stored
  link is an id (§7.2).
- **Direct style** (as JS `f$d`): a top-level function with row `{}` that the transitive
  no-suspend analysis (research/08 §3.2 item 6) proves never performs, installs, fails or
  calls a non-direct function may additionally be emitted as `(args…) -> result` in native
  WASM parameters with its own WASM stack frame. Its recursion depth is bounded by a counted
  **depth gauge** global: past its limit the direct attempt is abandoned and the caller makes
  the same call through the frame protocol (as compiled, below), so the native stack never
  overflows. Direct functions may raise a defect (§6.5) and nothing else.
  **As compiled (1.1.0, design §17.118):** the functions are `sakura_lir::direct::regions`
  (direct by the §17.111 facts, and first-order straight-line LIR: no `with`, `try`, handler
  state, callback or closure call; every call a call of another such function). Each gets a
  **twin** beside its frame-protocol body; twins call twins natively, and frame-protocol code
  calls a non-recursive region's twin instead of pushing a frame. The gauge counts estimated
  native stack bytes (a twin charges `64 + 8 × variables` on entry and gives it back on
  return; budget 128 KiB, so Wasmtime's 512 KiB default stack holds), and a module global
  holds the twin's state: 0 returned, 1 a defect is raised (the runtime holds it, as §6.5), 2
  the gauge ran out. A twin that runs out of gauge releases what it holds and returns 2 through
  every twin above it; the frame-protocol caller then makes the same call through frames,
  which call recursive defs through frames. Direct code performs nothing, so running it again
  is unobservable; a region that cannot run out (no cycle of twins, deepest chain within the
  budget) is entered without holding its arguments, otherwise the caller holds (`Dup`s) them
  for the second attempt. Every defect keeps its code, message and site. **1.2.0:** each second
  attempt adds one to the `restarts` counter (§10) before it starts (every backend that runs
  this fallback counts it, a native one included), so the bound of design
  §17.160 (2) is measured: a program's `restarts` is at most the number of entries into
  regions that can run out of gauge, and one entry is restarted at most once: the frame path
  calls recursive defs through frames, and a call it makes into another non-recursive region
  is a new entry, counted on its own.
- **Blame sites** (1.1.3, design §7.2, §17.115, §17.162; as JS `$site`, `runtime/js/ABI.md`
  §5). A top-level function whose `requires` or `where` checks blame its caller takes one
  trailing `i64` parameter, a site group id (§12) or 0: a call of it by name passes the group
  of the call's position, its function-value adapter passes 0, and a self tail call rebinds it
  to the self call's group before jumping back. Lambdas and clauses have no such parameter
  (a call through a function value names no site); their checks use 0.
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
| 0 `Return` | the return register (§5) holds the owned result | pop the frame, resume the parent at its `block`, whose resume block takes the result from the register |
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
(non-owning id of the frame that installed it), and the live state slots (owned), described by the
handler's layout row so scalar state stays unboxed here too (P1). Performing
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
`join_block` with the error in the return register (§5). A finalizer entry (tag 0x8003) owns its
cleanup closure and borrows the task; `finalize(fin, body)` pushes the entry, runs `body`,
and on normal return pops and runs it with `returned = true` exactly once (as JS). A defect or
cancellation unwinds the whole stack the same way with `returned = false`. A finalizer closure
takes one `Bool` parameter, `returned`, in slot 0, and runs under the activation in scope where
it was registered (the entry records it). While a finalizer runs the task is not unwinding: the
interrupted unwinding is kept in four slots after the finalizer's own, so a finalizer's own
`try` and finalizers nest. A defect, an escaping `Fail` (`D093`) or `exit` inside finalizers
aborts every running finalizer and continues the outer unwinding; finalizers they registered
still run. A defect replaces a `Fail` being unwound; otherwise the first unwinding wins
(design §17.129 P12). Finalizers run on
the trampoline like any Sakura code (they are closures); a `Fail` inside a finalizer is a
defect `D093 finalizer_failed`, as the JS runtime's behaviour will be pinned to by a shared
fixture (today JS lets it escape; the fixture decides and both backends follow it).

### 6.5 Outcome protocol

A task ends in exactly one of: `ok(value)`, `fail(error)`, `defect(code, message, spans)`,
`cancelled`; a task that is waiting is `pending(request_id)`. The outcome numbers are `ok 0`,
`fail 1`, `defect 2`, `cancelled 3`, `pending 4`, `exit 5` (design §17.129 P9); a defect
outcome's value is its `D` number, and the `run` and `outcome` exports (§9) also carry its
message and site, because the §12.2 defect record needs them. The host reads the outcome from
the task record (§7.3) after `run` returns. Defects are **never traps**: the LIR emits the
compare-and-branch and jumps to a shared per-function defect block that fills `task.defect`
(code as a `u16` from the `D` registry, message as a static string offset, the site id from
§12) and returns step 3. **1.1.3:** a contract check's site is its caller's group plus the
clause's offset in it when the group is not 0, else the clause's own site (role
`definition`). A defect whose message carries program data (`Option.expect`'s own text,
`D031`'s offset, `D004`'s value, `D011`'s `contract evaluation failed: <code>: <message>`)
holds it in a runtime `String` the task owns until it is freed; the outcome's message is that
text, so a record says what JS's says (design §17.162). Exit: `Cli.exit(code)` is a fourth terminal, `exit(code)`, that
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
finalizers before `cancel` returns (as JS `cancelTask`). A pending request's continuation is
dropped without running user code; a running task stops at its next `perform` (which then
returns no frame) or suspension; "before `cancel` returns" is reached by the canceller joining
the task (design §17.129 P12). A scope ends only when every child
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
- `Secret` (1.1.1): a `Secret` crosses as `sakura-vt1` tag 14. Only an import of kind `secret`
  receives its inner value; every other import receives `14, Unit` (redacted), decided by the
  guest's encoder (`Sched::vt_secrets`) at run time, not by a LIR check. The kind-`secret`
  imports as built are exactly `js_handler` (the arguments of a JS-built handler, for example
  `Llm.anthropic`'s key), `http_send` and `http_stream_open` (a request's `secret_headers`, the
  same egress as JS `fetchArgs`), listed in `sakura_rt::vt::SECRET_IMPORTS`; the cassette
  imports (`http_recording_*`) are not, so a recording sees that a header is secret but never
  its value. The guest zeroes the encoded transport `Bytes` before freeing it and the host
  zeroes its copied slice once decoded; the decoded values are JS strings, which cannot be
  zeroed, so the host only drops its references to them (a JS-built handler keeps its key for
  the life of the instance). This redaction is hygiene under the trust model (the host is part
  of the trusted base, §8), not a defence against the host. A further kind-`secret` import
  needs a line in the security review record (`docs/reviews/m3-security-review.md`) first.
- Checks as built (1.1.2): the host is trusted, but a buggy one must not corrupt the guest
  heap. The guest refuses with `D090`, before compiled code reads anything, an operation index
  past the handler's clause count in a JS-built handler's answer, and a host-delegated
  builtin's answer whose top (tag, handle kind, or record or variant type id) is not the
  builtin's declared result's (`sakura_rt::vt::top`, passed by `sakura_codegen_wasm`); type
  arguments inside the top stay the host's word. An import that may call back into the guest
  through `alloc_bytes` is called with no `&mut Rt` live (`wasm.rs` module doc).

### 8.2 Import table

One module `sakura_host`. The imports mirror the `sakura_core::builtins::BUILTINS` that reach
the host, plus the runtime's own; the exact list is `sakura_codegen_wasm::validate::IMPORTS`
and `HOSTED` (below), the §1 gate rejects a module naming any other, and `parity.rs` fails on
an import the Node host lacks. Every import has one of four shapes:

| Shape | Signature | Used for |
|---|---|---|
| `sync` | `(args as scalars/slices…) -> scalar`, or a `Bytes` from `alloc_bytes` copy-out | console, clock reads, `random_*`, `log_write`, `file_close`, the test runner, SQL and JS-built handlers, `http_body_close`, `serve_respond`, `trace_*`, `retire_handle`, `abort` |
| `async` | `(request_id: i32, args…) -> i32 accepted` | anything that suspends: `clock_sleep_ms`, `http_send`, `http_stream_open`, `http_stream_read`, `serve_listen`; completion comes back through the export `complete` (§8.5) |
| `callback` | the host receives a `callback_id: i32` and later calls the export `invoke(callback_id, args_ptr)`; used only by `serve_listen` | reentrancy, §8.5 |
| `hosted` (1.2.0) | `(ptr: i32, len: i32) -> i32`: the arguments as one `sakura-vt1` tuple, the answer a status-prefixed `Bytes` from `alloc_bytes` (§8.1) | a builtin whose JS implementation the host runs as is: `fs_*`, `file_open`/`file_read`/`file_write`, `env_get`, `cli_*`, `json_*`, `http_body_of`, `http_recording_*` |

Statuses: `0 ok`, `1 err` (the operation's ordinary `Err(reason)`; the reason string is copied
out), `2 denied` (a `[permissions]` refusal, reported as the operation's ordinary error, as JS),
`3 host_failure` (becomes `D091`), `4 ok_handle` (an `ok` completion whose payload starts with
the 12 bytes `kind, slot, generation` of the host handle it carries; §8.5, design §17.129 P10).

**As built (1.2.0, design §17.161).** The imports of an ABI module (`Host::Abi`) are exactly
these; a WASI command (the pure corpus on Wasmtime, `Host::Wasi`) imports only
`wasi_snapshot_preview1.fd_write` and `proc_exit` and implements the pure ones inside itself:

- Runtime (M4-RT): `retire_handle(kind, slot, generation)`, `abort(request)`,
  `trace_enabled() -> i32`, `trace_write(ptr, len)` (one §17.80 trace line).
- Console, clock, random, log (M4-WASM): `console_print(ptr, len)`, `console_read_line() ->
  Bytes` (0 at the end), `clock_now_ms() -> i64`, `clock_monotonic_ms() -> i64`, `async`
  `clock_sleep_ms(request, ms: i64)`, `random_int(lo: i64, hi: i64) -> i64`, `random_float() ->
  f64`, `random_bytes(n: i64) -> Bytes`, `random_uuid() -> Bytes`, `log_write(ptr, len)`,
  `file_close(id: i64)`.
- Test runner (W1): `test_seed() -> i64`, `test_record_failure(ptr, len)`, `test_failures() ->
  i64`, `test_output(ptr, len)`, `test_output_lines() -> Bytes`, `test_fixtures() -> Bytes`,
  `test_snapshot(name, name_len, rendered, rendered_len)`; property sessions `prop_begin(ptr,
  len) -> i64`, `prop_next(session: i64, result: i64, ptr, len) -> Bytes`, `prop_record(session:
  i64, ptr, len)`, `prop_failures(session: i64) -> i64`.
- Host-built handlers (W2): SQL `sql_open_memory() -> i64`, `sql_open_file(ptr, len) -> i64`,
  `sql_needs_root(instance: i64) -> i32`, `sql_root(instance: i64, ptr, len)`,
  `sql_query`/`sql_execute(instance: i64, ptr, len) -> Bytes`, `sql_begin(instance: i64) ->
  Bytes`, `sql_release`/`sql_rollback(instance: i64, token: i64)`; JS-built handlers
  `js_handler(name, name_len, ptr, len) -> i64` (kind `secret`, §8.1), `js_clause(instance:
  i64, op, ptr, len) -> Bytes`, `js_resume(session, status, ptr, len) -> Bytes`,
  `js_callback(id: i64, ptr, len) -> Bytes`.
- HTTP and serving (W2): `async` `http_send(request, ptr, len)` and `http_stream_open(request,
  ptr, len)` (both kind `secret`), `async` `http_stream_read(request, ptr, len)`,
  `http_body_close(ptr, len)`, `serve_listen(request, listener, ptr, len)` (`async` and
  `callback`), `serve_respond(task: i64, ptr, len)`.
- `hosted` (W1, W2): `cli_args`, `cli_parse`, `env_get`, `file_open`, `file_read`,
  `file_write`, `fs_exists`, `fs_list`, `fs_memory_root`, `fs_read`, `fs_root`, `fs_subdir`,
  `fs_write`, `http_body_of`, `http_recording_append`, `http_recording_append_stream`,
  `http_recording_lookup`, `http_recording_lookup_stream`, `http_recording_open`,
  `json_decode`, `json_encode`, `json_parse`, `json_schema`.

L4 added no import: twins are internal functions, and their gauge and state are module
globals (§5). `ptr, len` is a slice (§8.1) and every unannotated parameter an `i32`.

### 8.3 Permissions and capabilities

The driver passes `SAKURA_PERMISSIONS` to the host exactly as for JS; every host operation
rechecks grants on each call (research/09 §5 item 5: numeric secrecy is not security). A `Dir`
is a `Handle` object (§3.2 tag 17) whose host slot records the resolved directory and its
read/write authority; `fs_subdir` creates a new slot with attenuated authority. `fs_memory_root`
is a slot over an in-memory store with no host authority (design §17.119). Path rules are the JS ones.

### 8.4 Handle tables

Host side: one table per **kind** (the 1.2.0 list below), entries `(slot,
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

As built (1.1.1 erratum): the Node host issues each handle a fresh slot and generation 1 and
never reuses a slot, so a generation mismatch is reachable only from a forged or foreign handle
(a host fault); the generation increment and retirement above are the rule for a host that
reuses slots. A `File`'s `id` is the JS runtime's plain `Int` id into its own file table, as on
JS, not a packed handle; `dir` and `body` are handles.

Handle kinds (1.2.0, design §17.161): `0 dir`, `1 file`, `2 body`, `3 socket`, `4 sql`, `5
callback`, `6 jsvalue`, `7 js_handler`. `file` is reserved (a `File`'s `id` is a plain `Int`,
above) and `socket` unused (a served request is a task, `invoke`); `jsvalue` is an opaque host
value a JS callback returned, which the guest only passes back to the host and never decodes.
From a guest and host that keep these rules, `retire_handle` receives kinds 0, 2 and 4–7: a
`dir`, `body` or `jsvalue` handle when its `Handle` object dies (§3.5), and kinds 4, 5 and 7
as below. The guest passes on whatever kind a handle it was given carries, so a host ignores
a kind it does not know.

Instances and callbacks (1.1.2, design §17.161): a host-built handler's state, an `Sql`
database (kind 4, `sql`) or a JS-built handler such as `Llm.anthropic` with its key (kind 7),
is named by a handle every clause closure of the guest handler captures and that never
crosses in a value; when the guest handler dies the handle is retired and the host closes the
database or drops the JS handler and the callbacks its clauses made. A JS function a JS-built
handler passes to an operation (kind 5, `callback`) is held by the native closure standing for
it and retired when that closure dies, no later than its clause's handler. A `Dir` or `Body`
argument whose handle is of another kind is a host fault (`D090`), never an object of the
wrong class.

### 8.5 Asynchronous requests, cancellation and late completions

- The guest allocates a **request** (tag 0x8005: `generation`, `task` id, `task_epoch`,
  `frame` (owned continuation), `kind`, `state ∈ {pending, completed, cancelled}`) and calls
  the `async` import with its packed `request_id`. The host keeps `(request_id, copied
  inputs)` in its closures, never guest pointers (research/09 §5 item 6).
- Completion: the host calls the export `complete(request_id, status, payload_ptr, payload_len)`
  after copying the payload out. The guest checks the request's generation, that `state ==
  pending`, and that `task.epoch == request.task_epoch`; on any mismatch (a stale, duplicate
  or older-epoch completion) it **discards** the payload (drops the `Bytes`), and when the
  status is `4 ok_handle` retires the handle named by the payload's first 12 bytes through the
  import `retire_handle(kind: i32, slot: i32, generation: i32)` (no result), and returns 0;
  the host then does nothing else. The binding is on the M4-WASM host as well (P10).
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
  until guest state is consistent"). As defence the runtime also queues a `complete` that
  arrives while guest code is running: it returns `2` (queued) and delivers it at the next
  scheduler point with the usual checks (design §17.129 P11).
- Limits: at most `SAKURA_MAX_REQUESTS` (default 4,096) pending requests and
  `SAKURA_MAX_COPY_BYTES` (default 256 MiB) aggregate in-flight copied bytes per instance;
  exceeding either is the operation's ordinary error `"too many pending operations"` /
  `"response too large"`, so a server under load fails requests instead of the process (§10).
  As built (1.1.2), one policy for each: an operation whose result is a `Result<_, String>`
  (`http_*`) answers `Err` with the message; one without an ordinary error to carry it is the
  defect `D091` with it (`Clock.sleep` and `serve_listen` on a full table; a synchronous
  answer past the budget, raised by the guest from a status-2 answer, or ending the run where
  the import has no answer). The budget counts one answer, and an HTTP completion together
  with the completions already queued for delivery.
- Limit variables (1.2.1, design §17.209 (1)): unset is the default; decimal digits with a value
  from 1 to 2^53 - 1 are that value; anything else (empty, a sign, a fraction, an exponent,
  spaces, words) is the defect `D091` naming the variable, the value seen and the accepted
  form, raised at first use (JS `http_send`, which the Node host also calls, reads
  `SAKURA_MAX_COPY_BYTES` on each request; the Node host reads its own budget once per instance,
  at its first copy or when `serve_listen` starts), so a limit is never off and never zero by
  accident. Until 1.2.1 a non-numeric value turned the budget off and an empty one refused
  every body. The budget is a per-instance ceiling on host memory, not a byte-exact contract:
  the JS backend counts body bytes per response and this host the encoded answer plus the
  queued completions, so a response within the limit by less than its headers and the other
  in-flight answers can be `Ok` on one and `Err` on the other.
- Payloads (1.1.2): `complete` and `invoke` take ownership only of a `Bytes` inside memory
  with `rc = 1` and no static flag; anything else is refused untouched (`complete` returns 0
  and the request stays pending).

### 8.6 Traps and teardown

A WASM trap (impossible in a correct build: every defect is a branch) is caught by the host,
reported as `D090` with the function name from the names section and the PC, and the host
runs its own **resource ledger**: it closes every open handle and aborts every request of
the instance, because guest finalizers cannot run after a trap (research/08 §3.4). The same
ledger runs at process exit. The instance is never entered again (1.1.2): `sakura test`
reports the test as a defect and runs the rest on a fresh instance of the module.

## 9. Exports

`memory`, `sakura_abi_version` (global), `run(task) -> i32` (an outcome number, §6.5), `start(entry_fn) -> task`,
`complete(req, status, ptr, len) -> i32`, `invoke(cb, ptr, len) -> i32`, `alloc_bytes(len) ->
ptr`, `drop(ptr)`, `layout_table() -> (ptr, len)`, `effect_table() -> (ptr, len)`, `outcome(task)
-> (kind, ptr, len)` (the outcome encoded as §8.1 transport; a defect's carries its code,
message and site, §6.5), `tests() -> (ptr, len)` (the
test list and `effects` map as JS `__tests__.js`, in transport encoding), `stats() -> (ptr,
len)` (§10 counters). Nothing else; the validator rejects extra exports (§1). The list
describes the **linked** module: rustc exports a `static` as a global holding its address and
cannot return several values through `extern "C"`, so the M4-WASM linker synthesises
`sakura_abi_version` and the multi-value exports over single-value runtime functions. The
`sakura_rt` object alone exports `memory`, `alloc_bytes` and `drop`.

## 10. Memory bounds and growth

- `SAKURA_MAX_HEAP` (bytes; default 1 GiB; the manifest may lower it per package in a later
  milestone) is the WASM memory maximum declared in the module **and** the allocator's hard
  limit; the host passes the value at instantiation and refuses a module whose declared
  maximum exceeds it. The allocator grows memory in 64 KiB pages on demand up to the limit.
- Allocator (research/09 §3 recommendation): size classes 16, 32, 48, 64, 96, 128, 192, 256,
  384, 512, 768, 1024, 1536, 2048, 3072, 4096 bytes (geometric, `class` byte in the header),
  one free list each, carved from 64 KiB runs; larger objects are **page runs** (`class = 0xFF`,
  run size stored in a `u32` at offset −8 from the 8-aligned header), 4 KiB-granular, first fit
  over an address-ordered free list, coalesced on free, from the same bounded heap. Every size
  computation is checked before alignment and growth. Counters (reported by `stats()`):
  `requested` (cumulative bytes requested), `live` and
  `peak` (bytes of live blocks at their class or run size), `live_objects` (live blocks; the
  §7.5 tests compare it), `committed` (linear memory above the heap base), `free_list_bytes`
  (small free lists plus free page runs), and since 1.1.0 (design §17.118) `frames`
  (continuation frames allocated), `boxes` (`IntBox`/`FloatBox` objects compiled code
  allocated, the LIR's `Box`), `dups` (reference-count increments compiled code asked for:
  `Dup` and the increments of projections, captures and state reads) and `drops` (calls of the
  `drop` export, compiled code's and the host's), each a cumulative `u64` in that order after
  the first six, and since 1.2.0 (design §17.160 (2)) `restarts` (direct-region calls run again
  through frames after their twin ran out of gauge, §5) after `drops`. The interface is
  `SAKURA_WASM_STATS=<path>`: `sakura_wasm_host.js` appends the counters by name as one JSON
  line at exit, for benchmark records; there is no CLI flag, and the JS backend reports no
  counters. A dlmalloc comparison harness stays in `runtime/rt/bench`.
- Failed growth or an exhausted limit is **one defect path**: `D096 heap_exhausted` from the
  allocating site, after which the task unwinds (§6.4). The runtime reserves a fixed emergency
  region (64 KiB, committed at start, its own free lists, small blocks only, used only while the
  runtime has enabled emergency allocation) so unwinding and the destruction worklist never allocate from the exhausted
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

**As built (1.1.3, design §17.162).** `role` is `u8` 0 `primary`, 1 `definition` (2,
`caller`, is unused). Site ids are entry index + 1 (0 is no site). Sources are indexed by the
compiler's file number; a source's `uri` is the one the JS backend's spans carry, and an
empty one has no span. A **site group** is a run of `primary` entries for one call position,
one per contract that the callee's checks blame the caller with, in the callee's body order
(`sakura_lir` lowering); every clause with a position has one `definition` entry with its
`contract_id`. The host renders a defect's site as the JS runtime's `violated` does: a
`primary` entry with a `contract_id`, then that contract's `definition` entry with role
`definition`; a `definition` entry alone as `primary`. `module_hash` and the sources' content
hashes are zero (not computed): the section is read from the module it is part of, and the
host still bounds-checks it and ignores a malformed one. Function sites are 0 and there are
no block entries yet (no logical stack traces).

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

## Open questions (from M4-RT R2)

Readings `sakura_rt` implements where 1.0.1 is still silent; each is a question for the designer
(`compiler/REQUESTS.md`, 2026-10-18) and, where the LIR must agree, says so. Design §17.129
endorsed P8–P12 (P9 amended: `run` and `outcome` carry a defect's message and site) and 1.0.2
folds them into §5, §6.2, §6.4, §6.5, §7.4, §8.2, §8.5 and §9; they stay below as history.

- **P1 Activation state (§6.3, design §17.108).** §17.108 keeps an activation's live state slots
  as generic words, but `StateGet`/`StateSet` address the *activation*, so the amendment's goal
  (no box per `StateSet`) holds only if the activation's state follows its handler's row.
  Reading: the activation's state slots are described by `handler.type_id` (the activation
  owns its handler, so the row is one load away). LIR must agree.
- **P2 Builtin ownership (§5).** Runtime builtins take their arguments owned and return owned
  results, like every Sakura call, so a builtin may reuse a uniquely owned input (`list_push`
  updates a unique root in place, §17.103).
- **P3 Effect table (§3.2 `effect_id`).** No format is given. Reading: `u32 n`, then `n` name
  offsets (qualified names, as JS effect ids), so `Show` of a handler prints `<handler
  std::console::Console>` as JS. LIR must emit it.
- **P4 Collection layouts (tags 9–13).** `ListNode` kind 2 is unused (the trie root is an
  ordinary branch; JS has no root node). A tail node's capacity is its block's class (at most
  32); a push appends in place while `count == tail_len < capacity` and the node is not static
  (JS's "append to the shared tail"). `HamtNode`: `u32 bitmap` @8, `u32 n` @12, `n` 8-byte
  entries @16, a leaf `(pos << 1 | 1, hash)`, a child `(ptr, 0)`; a collision node has bitmap 0;
  the root is always a branch (a lone leaf is wrapped where JS keeps the leaf itself). `holes`
  is `log.len − len`; removed log entries are one static marker object.
- **P5 Tuples across generic boundaries (§3.3, §4).** A `(Int, String)` built at a known type has
  an `i64` slot; the same value built by generic code (`list_zip`, the `pair` row) has a boxed
  one. `Eq`, `Ord`, `Hash` and `Show` read each object's slots through its own row and treat a
  word pointing at an `IntBox`/`FloatBox` or holding a `Bool` immediate as that scalar, so the
  two are equal, hash alike and print alike; `map_of` reads its pairs through their row (boxing
  a scalar slot). Question for M4-LIR: does it coerce tuples at instantiation sites, or project
  through the row kinds? The runtime accepts both.
- **P6 `Float` equality (§2).** §2 says `Eq` treats all NaNs as one value "as JS does", but JS
  `eq` has `NaN != NaN` (its test asserts `!eq(Some(NaN), Some(NaN))`). Implemented as JS: `Eq`
  is IEEE `==` (`-0.0 == 0.0`); `Ord`, `Hash` and map/set keys treat all NaNs as one value
  (`cmp(NaN, NaN) = 0`, NaN last; SameValueZero keys). Proposed rewording of §2 accordingly.
- **P7 Hash function.** Not observable (no std function exposes `Hash`): strings use FNV-1a 32
  over their UTF-8 bytes (cached in the header, §3.2), composites an FxHash-style mix of their
  normalised contents finished by murmur3 `fmix32`.

- **P8 Results, errors and `env` (§5, §6.2, §6.4).** "The frame's result slot" and "the parent's
  named slot" have no field in the §6.1 frame. Implemented: a callee leaves its result, and a
  caught `Fail` its error, in the task's 8-byte **return register**, which the parent's resume
  block takes first (the driver never drops it, because it cannot know whether the bits are a
  word). `env` (the closure) is passed on a function's entry call only; a function that needs
  it after its first call or suspension keeps it (counted) in a slot. The `frame` argument is
  the frame's address, a transient borrow for that call; stored links are ids (§7.2). A task's
  result is a generic word. LIR must agree.
- **P9 Outcome numbers (§6.5).** `ok 0, fail 1, defect 2, cancelled 3, pending 4, exit 5`; a
  defect outcome's value is its `D` number. The runtime keeps the code, not yet the message or
  site (`run` and `outcome` exports land with W1).
- **P10 A late completion that carries a handle (§8.5).** Once a request is cancelled its id is
  stale, so the guest cannot know what the completion carried. Implemented: the status
  `4 ok_handle` says the payload's first 12 bytes are `kind, slot, generation`; a stale,
  duplicate or older-epoch `ok_handle` completion retires that handle and drops the payload.
  Host must agree.
- **P11 Completion while the guest runs (§8.5).** The host is required to queue; as defence the
  runtime also queues a `complete` that arrives while guest code is running, returns `2`
  (queued), and delivers it at the next scheduler point with the usual checks.
- **P12 Cancellation and finalizers (§6.4, §7.4).** `cancel` marks the task and its
  descendants, bumps epochs, drops the continuation of a pending request without running user
  code and calls `abort`; a running task stops at its next `perform` (which then returns no
  frame) or suspension. "Before `cancel` returns" is reached by the canceller joining the
  task. Finalizers run on the trampoline with `returned` (`Bool`) in slot 0, under the
  activation in scope where they were registered (the finalizer entry records it). While one
  runs the task is not unwinding: the interrupted unwinding is kept in four slots after the
  finalizer's own, so a finalizer's own `try` and finalizers nest. A defect, an escaping
  `Fail` (`D093`) or `exit` inside finalizers aborts every running finalizer and continues the
  outer unwinding; finalizers they registered still run. A defect replaces a `Fail` being
  unwound; otherwise the first unwinding wins.

## Errata history (1.0.0 → 1.0.1)

Recorded by the `sakura_rt` implementer during R1 where 1.0.0 was ambiguous, silent or not
implementable as written; design §17.108 endorsed E1–E12 (E6 amended for handler state) and
1.0.1 folds them into the sections above. Kept as the record of what changed and why.

- **E1 Destruction worklist (§3.5).** Implemented as a stack threaded through the `rc` fields of
  the dying objects (an object at count zero has no other use for the field), not as an arena
  array with an in-place fallback. Destruction therefore never allocates, never recurses and
  cannot overflow, which is what §3.5 and research/09 §3 ("reserve room for destruction
  metadata") ask for; the bounded array and its fallback become unnecessary. Not observable.
- **E2 Tuple payload (§3.2 tag 5).** "`u32 arity`, `u32 pointer_bitmap_len`, then slots" names a
  bitmap length but no bitmap, and per-arity layout rows (§3.4) cannot tell `(Int, String)` from
  `(String, Int)`. Read as: `u32 arity`, `u32 type_id` of the tuple's **layout row** (one row
  per tuple *shape* used), slots at offset 16, so tuples, like records, are described by the
  layout table and nothing else (§3.4 rationale). Runtime builtins that build tuples use the two
  shapes named in the table header (E3). LIR must agree.
- **E3 Layout table bytes (§3.4).** The listed row fields are kept in order (`size_bytes`,
  `n_slots`, pointer bitmap words, `scalar_kinds` offset, `name_offset`) with one field appended,
  `field_names` (offsets of the field names, which `Show` of a record needs). Around the rows: a
  header `n_types, option_type_id, result_type_id, pair_type_id (word, word),
  int_pair_type_id (i64, word)` (builtins such as `list_get`, `map_entries` and
  `list_enumerate` construct these), a directory of entry offsets, and per type an entry
  `kind (record | error record | tuple | variant), name, n_rows, row offsets` (one row per
  constructor for a variant). Names are `u32 len` + UTF-8; offsets are relative to the table.
  Error records need their own kind because `Show` prints a field-less error tag as `Name`, a
  field-less record as `Name {}` (as JS). LIR must agree.
- **E4 Closures and frames need a descriptor (§3.2 tag 8, §6.1).** A closure's captures can be
  scalars or pointers and its header has no bitmap; frames name "the function's frame
  descriptor" without a format. Both are read from one **code descriptor table** indexed by
  `code`: `name, n_frame_slots, frame_kinds, n_captures, capture_kinds` (2-bit kinds as §3.4). A
  slot of kind 0 owns its word. LIR must agree.
- **E5 `Unit` slots (§3.3, §3.4).** Kind 3 covers `Bool` and `Unit`, so `Show` cannot tell
  `false` from `()`. Read as: kind 3 is `Bool` (`i64` 0/1); a `Unit` slot has kind 0 and holds
  the `Unit` immediate word (dropping an immediate is a no-op). LIR must agree.
- **E6 Slot alignment and handler state (§3.2 tags 6, 8, 14; amended by §17.108: handler state has a layout row, see §3.2).** Slots start at the next 8-byte
  boundary: offset 16 for records, variants, tuples and closures (records pad after
  `type_id`), offset 24 for a handler's state slots, followed by its `n_clauses` 4-byte clause
  closure pointers. A handler's state slots and an activation's live state slots hold **generic
  words** (§4), because neither object has a type to describe them; `StateGet`/`StateSet` box and
  unbox. LIR must agree.
- **E7 `rc` overflow (§3.1).** A count may reach 2^32 − 1; the increment from 2^32 − 1 is
  `D092` and leaves the count unchanged.
- **E8 Building the runtime object (§1).** LLVM's default wasm32 features include
  `reference-types`, whose `call_indirect` encoding (a padded multi-byte table index) fails the
  MVP validator ("zero byte expected"), even though no reference-typed instruction is used. The
  object passes the gate unchanged in profile when compiled with `-Ctarget-cpu=mvp
  -Ctarget-feature=+mutable-globals,+multivalue,+bulk-memory` and fat LTO with one codegen
  unit (so libcore's embedded bitcode is recompiled under those features);
  `runtime/rt/tests/wasm_gate.rs` builds it that way. `sign-ext` and `nontrapping-fptoint` were
  **not** needed, so the allowlist stays as written and no MINOR bump is due. The M4-WASM build
  must use the same flags.
- **E9 Exports rustc cannot produce (§0, §9).** rustc exports a `static` as a global holding its
  *address*, so the immutable `sakura_abi_version` global with the version as its value, and
  the multi-value exports (`layout_table`, `effect_table`, `outcome`, `tests`, `stats`), must be
  synthesised by the M4-WASM linker over single-value runtime functions. The runtime object
  exports only `memory`, `alloc_bytes` and `drop` today (all in §9), and imports only
  `sakura_host.retire_handle`.
- **E10 `retire_handle` signature (§8.5).** Named but not typed: implemented as
  `(kind: i32, slot: i32, generation: i32) -> ()`.
- **E11 Counters (§10).** `requested` is cumulative bytes requested; `live` and `peak` are bytes
  of live blocks at their class or run size; `committed` is linear memory above the heap base;
  `free_list_bytes` counts small free lists and free page runs; `live_objects` (added) counts
  live blocks, which the §7.5 tests compare.
- **E12 Page runs and the emergency region (§10).** Page runs are 4 KiB-granular runs from the
  same bounded heap (first fit, coalesced on free), and small classes are carved from 64 KiB
  runs; "a separate bounded region" is read as separate free structures, bounded by
  `SAKURA_MAX_HEAP` like everything else. The emergency region is a separate 64 KiB pool with
  its own free lists, committed at start, used only while the runtime has enabled emergency
  allocation (unwinding) and only for small blocks.

## Questions and errata (from M4-LIR)

Recorded by the `sakura_lir` implementer (L1, L2, L1a) where this document is ambiguous or
silent. Each entry names the reading implemented in `crates/sakura_lir` and why; none changes a
decision, and each is a question for the designer (`compiler/REQUESTS.md`, 2026-10-18). Numbered
L1… so they do not collide with the M4-RT entries above.

- **L1 Agreement with 1.0.1.** `sakura_lir` emits the §3.4 layout table
  (`LayoutTable::to_bytes`) and code descriptor table (`layout::code_table`), decoded in
  `crates/sakura_lir/tests/abi.rs` exactly as `runtime/rt/src/layout.rs` reads them, and the
  effect table of open question P3 (`layout::effect_table`); tuple rows per shape (§3.2 tag 5),
  `Unit` fields as kind-0 immediates, slots at offset 16 (24 for handler state), and handler
  state rows (L11). Snapshots of all three tables are in `compiler/tests/abi/`.
- **L2 E2 needs dynamic tuple reads and row-blind structural operations.** A tuple's row is fixed
  where it is built, but its static type differs between generic and monomorphic code:
  `fn dup<A>(x: A) -> (A, A) { (x, x) }` builds `(word, word)` and `dup(20).0 + 1` reads it as
  `(Int, Int)`; `(7, "seven")` is built `(i64, word)` and read by
  `fn first<A, B>(p: (A, B)) -> A`. So (a) a tuple element read is the LIR's `TupleGet`, which
  takes the slot kind from the object's row and boxes or unboxes to the reader's representation
  (a branch per read; a static check is impossible), and (b) `Eq`, `Ord`, `Hash` and `Show` of
  tuples must not depend on the row: `(3, 3)` built either way is one value, one `Set` member
  and one `Map` key, so `Hash` of an `i64` slot and of an `IntBox` word must agree. Builtins that
  read tuples (`map_of`) read through rows too. `crates/sakura_lir/tests/corpus/lir/tuple_shapes.sak`
  pins both; it failed in the interpreter until (b) held. This answers M4-RT's P5: the LIR
  projects through the row kinds and never coerces a tuple at an instantiation site, and
  `sakura_rt` already implements (b). Question for the designer: keep E2 with these two
  conditions, or make tuples all words (one row per arity, no conversion, one `IntBox` per
  scalar element)? Either is one change in `sakura_lir/src/lower.rs`; under §17.103 the first
  costs a branch per generic read and the second an allocation per scalar element.
- **L3 Closure calls (§5, §6.1).** The frame has no `env` field. Read: calling a closure stores it
  as the callee frame's slot 0, owned (the callee frame owns its closure); a clause takes the
  activation id in slot 1, then the operation's arguments (L12). The function loads every
  capture at entry (each load a new reference) and drops the closure, so only block 0 needs
  `env` and no resumed block reads it. Every argument and result passed through a closure value
  other than a clause is a generic word (Lean's boxed convention), because the callee may be a
  generic function, a lambda, the callback stub or an adapter; a direct call of a top-level
  function keeps known representations, and a top-level function used as a value is a static
  closure (§3.1 `static`) of a generated adapter. `MakeCallback` returns an existing callback
  unchanged, so the first perform site wins (as JS `callback` and `sakura_eval`).
- **L4 Catch entries record the target's region state (§6.4).** A `try`'s join point may lie
  outside `with` and `try` regions opened after the `try` in the same function (Core: "a join
  point defined outside body"), so a caught `fail` must also release those activations and pop
  those catch entries. A catch entry therefore holds, besides `join_block`, `frame_id` and
  `fin_mark`, the activation id in scope at the join and the catch-stack height there; the LIR's
  `TryPush` carries the static counts (`Exits { handles, trys }`) the runtime computes them from.
  The values the catch block receives besides the error are moved into the catching frame's
  slots by `TryPush` and dropped by `TryPop`; when a `fail` resumes a frame that was suspended at
  a call, the slots of that abandoned continuation are dropped.
- **L5 The defect path (§5, §6.5).** §5 asks for balance "including the defect path". The LIR has
  no defect edges: an instruction that can defect (checked arithmetic, `Check`, a builtin)
  leaves its block owning exactly the references the validator's count holds at that
  instruction, and the defect path releases them before unwinding frames. The emitter
  materialises this as a per-site landing pad (or spills the live set) in front of the shared
  per-function defect block. The interpreter does so, and every corpus program that ends in a
  defect ends with no live object.
- **L6 Builtins take owned arguments**, as calls do (§5): `list_len(xs)` drops `xs`. Conservative;
  M5 may mark builtin parameters borrowed. A builtin's parameter and result representations are
  those of its std `@external` declaration (type-variable positions are words; `Int`, `Float`,
  `Bool`, `Unit` known scalars; a `Never` result is `unit`), plus the four `sakura_core` emits
  itself (`list_at: (ptr, i64) -> word`, `contract_enter`, `contract_exit`, `match_failure`);
  table: `crates/sakura_lir/tests/corpus/builtins.sigs`. `sakura_rt`'s builtins must take the same.
- **L7 Frame and closure slot kinds.** Frame slots are the entry parameters, then every parameter
  of a block the trampoline resumes (a call or `perform` continuation, a catch target), in block
  order. `Bool` and `Unit` values there use kind 3; an activation id uses kind 1 (non-owning,
  zero-extended), including as a closure capture (a lambda in a clause that reads the clause's
  state captures the activation id, never the activation, as §7.1 requires).
- **L8 `Unit`-typed Core temporaries.** Core types some temporaries `Unit` although they hold a
  value (the `ok`/`err` of a `try`, join parameters); the LIR represents a `Unit` local bound to
  a reference, and every `Unit` join parameter, as a generic word. A request to `sakura_core`
  for after the gates is in `compiler/REQUESTS.md`.
- **L9 Constants** (`FnKind::Const`) are evaluated at each use, as in `sakura_eval`, not cached as
  JS does; they are pure, so only allocation counts differ.
- **L10 `D003`.** It never occurs on this profile (§17.80): `Int` arithmetic is checked `i64`.
  Fixtures whose `needs:` names the JS backend are outside the LIR corpus; M4 fixtures use their
  `expect-defect` (or `expect-defect@wasm`).
- **L11 Handler state rows (§17.108 E6 amendment; 1.0.1 §3.2 tag 14).** Each `handler`
  expression's state is described by the shape row of its state values where it is built (a
  type-variable state is kind 0), shared with tuple rows of the same shape, as 1.0.1 §3.4 says
  ("one entry per handler state shape"). `MakeHandler`, `StateGet` and `StateSet` name the row,
  and the validator checks every state value against it. An activation's live state follows its
  handler's row (M4-RT's P1, the same reading: otherwise `StateGet`/`StateSet`, which address
  the activation, would still box). A host-built handler (`console_host`) has no state and no
  row (`type_id = 0xFFFF_FFFF`). Static resolution: a clause's activation parameter gets the row
  of the `handler` expression that lists the clause, and the row follows `let` aliases, jump
  arguments and closure captures. Effect: `semantics/million_operations` allocates 4 objects
  instead of 2,000,005 (with L12).
- **L12 Clauses use their operation's declared representations.** A clause is called only by
  `perform`, and both know the operation's signature, so `perform` passes its arguments, and the
  clause returns its result, in the representations of the operation's declared types (a
  type-variable position stays a word; a function-typed argument is the callback closure). The
  effect table records each operation's signature for the validator. Same reason as L11: an
  `Int`-returning operation would otherwise box once per `perform` (§17.103).
