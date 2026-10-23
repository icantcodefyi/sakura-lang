# Performance delta ledger

WS-A records evidence here for performance round 1
([plan §4–§5](../docs/plans/performance-r1.md), design §17.109–§17.110).
A row is evidence for the named workload, engine and input; it is not a language score.

No optimisation delta is claimed by this initial ledger. The
[candidate baseline](baselines/candidate/README.md) is **unblessed** and the
[noise report](../research/10-evidence/ws-a/README.md) describes unchanged-source variation.
A noisy before/after difference without confirmation is not a speedup or regression.

| Workstream / change | Before → after measured commit | Program / engine / size | Before → after Sakura ms and Sakura/JS ratio | `bench compare` artifact / verdict | Semantics and trace parity | Load / limitations |
|---|---|---|---|---|---|---|
| WS-A initial measurement | Candidate at `a843e60`; no optimisation comparison | See linked raw evidence | Candidate only | No adoption verdict | Benchmark oracles; not `--no-direct` parity evidence | Shared reference host; CI subset only |
| WS-P3 direct non-tail recursion (§17.112) | `824a876` clean → WS-P3 tree on `824a876` (`dirty: true`, the uncommitted change itself); [before](results/2026-10-18-d88970189598-wsp3-before.json), [after](results/2026-10-18-d88970189598-wsp3-after.json) | fib, binary_trees; Node 26.11.0, Bun 1.4.3; matched; all four sizes | fib n=30: Node 45.15 → 7.442 ms, 9.54× → 1.64×; Bun 28.46 → 7.98 ms, 9.51× → 2.66×. binary_trees n=14: Node 195.4 → 42.2 ms, 9.16× → 2.03×; Bun 171.5 → 58.52 ms, 7.94× → 2.83× (every size improved 63–86% in ratio) | [compare](results/2026-10-18-d88970189598-wsp3-compare.json): **blocked** on peak RSS only: binary_trees sakura/node n=14 198 → 291 MB (+47%), and the unchanged hand-JS control js/node n=8 +26% | `deep_sum`, `deep_countdown`, `mutual_recursion`, `deep_recursion_defect`, `deep_recursion_gauge` under both ABIs with trace and stderr parity (`cargo test -p sakura_cli --test semantics`) | Improvement: recursive calls no longer allocate a continuation and context per call; each costs a gauge increment, compare and decrement. RSS: the higher peak reproduced in-process (20 runs of n=14: peak 266 MB direct vs 163 MB `--no-direct`). Post-GC heap after those runs was 4.6 vs 4.7 MB, but per WS-A's ruling that is not peak RSS and proves neither GC causation nor the absence of a leak. **Open block**, not suppressed; one run of each, no confirmation rerun |
| WS-P4 checked arithmetic (§17.114): interval pass, then inline fast path | `13d49d5` base → `57813e0` interval pass → `01d8c02` pass + inline fast path, each clean (local commits; they landed with the same trees as the `perf(core)` and `perf(codegen)` WS-P4 commits before those were rebased onto later master). Two rounds each, interleaved per program; each file merges the six per-program `bench run --filter` runs and sets `meta.commit` by hand. Measured before WS-P6 (`e833b02`) landed: [base r1](results/2026-10-18-d88970189598-wsp4-base-r1.json), [base r2](results/2026-10-18-d88970189598-wsp4-base-r2.json), [pass r1](results/2026-10-18-d88970189598-wsp4-pass-r1.json), [pass r2](results/2026-10-18-d88970189598-wsp4-pass-r2.json), [inline r1](results/2026-10-18-d88970189598-wsp4-inline-r1.json), [inline r2](results/2026-10-18-d88970189598-wsp4-inline-r2.json) | fib, countdown, sieve, nbody, spectral_norm, contracts_plain; Node 26.11.0, Bun 1.4.3; matched; all four sizes | Geometric mean of the ratio over sizes and rounds, base → pass → inline. countdown: Node 1.88 → 1.89 → 1.57, Bun 4.67 → 4.79 → 2.75. contracts_plain: Node 2.75 → 2.53 → 2.38, Bun 1.98 → 1.79 → 1.45. spectral_norm: Node 11.26 → 10.71 → 9.17, Bun 14.57 → 14.07 → 13.27. fib: Node 1.54 → 1.48 → 1.56, Bun 2.52 → 2.38 → 2.36. sieve: Node 27.3 → 27.0 → 31.1 (one round at +32%, the other +0.5%), Bun 64.7 → 59.6 → 60.1. nbody: Node 37.6 → 38.2 → 37.2, Bun 35.9 → 37.7 → 36.5. Largest size, Node: countdown n=300000 1.353 → 0.950 ms (2.22× → 1.62×); contracts_plain n=10⁶ 2.956 → 2.155 ms (3.50× → 2.64×); spectral_norm n=200 14.42 → 11.79 ms (10.73× → 9.19×) | Inline vs base, both inline rounds as heads: [vs base r1](results/2026-10-18-d88970189598-wsp4-compare-base1.json) **blocked** (nbody bun n=5000 +22%, spectral_norm bun n=400 +11%, sieve sakura/node n=10000 peak RSS +25%); [vs base r2](results/2026-10-18-d88970189598-wsp4-compare-base2.json) **flagged** only. Noise control, base r1 vs base r2 (identical code): [compare](results/2026-10-18-d88970189598-wsp4-compare-noise.json) **blocked** on the same three rows (+20%, +25%, RSS +45%) | Every semantics fixture under both ABIs (`cargo test -p sakura_cli --test semantics`), new `arith_ranges_proved` and `arith_ranges_counter_overflow`; `SAKURA_DIFF_PROGRAMS=2000` differential with range-edge cases and the evaluator's `audit_ranges` oracle (2000 agree, 1765 through the LIR too); three deliberately unsound variants of the pass were each caught | **Inconclusive where noisy:** load average 10–11 during both runs (other agents' builds and tests); base-vs-base blocks, so the blocks above are not attributed to the change. Improvements explained: proved loop counters, guarded `n - k` and non-zero literal divisors emit the raw operator (no call, no check); the inline fast path replaces the `$rt.add`/`sub`/`mul` call by `r = a op b; if (!Number.isSafeInteger(r)) $rt.op(a, b)` (the call only raises the defect). No check is removed that can fail |
| WS-P4b contract-seeded intervals (§17.154) — **advisory only** (§17.153) | `d900ee3` (origin/master, release build) → WS-P4b tree on `d900ee3` (release build); ad-hoc, not `bench run`: no raw result files | `countdown` with `requires n >= 0` added to `sum_tail` (not the benchmark definition, which has no contract and is unaffected); `runner.mjs measure`, n=300000; Node 26.11.0, Bun 1.4.3; two rounds | Median ms per call, rounds 1/2: Node 1.152/1.155 → 1.154/1.156; Bun 0.405/0.410 → 0.419/0.417. No hand-JS ratio taken. The only emitted difference is the removed `if (!Number.isSafeInteger(t_2)) $rt.sub(n, 1)` for `n - 1`; `acc + n` keeps its check | None (no `bench compare`) | Semantics fixtures under both ABIs, WASM parity, `SAKURA_DIFF_PROGRAMS=2000` differential with seeding `requires` cases under `audit_ranges` | **Inconclusive:** load average 12–15 (other agents' builds); the difference is below run-to-run noise. Accepted only at the quiet window (§17.153), where `countdown`'s halves are reported separately (§17.154 item 3) |
| WS-P2 handlers (§17.113): bound performs, tail-direct dispatch, `s<k>` state slots | `c4da994` clean → `9fa7d68` (the two WS-P2 commits on `c4da994`; `dirty: true` = untracked result files only); [before](results/2026-10-18-wsp2-c4da994-before.json), [after](results/2026-10-18-wsp2-9fa7d68-after.json); pre-rebase pair [13d49d5](results/2026-10-18-wsp2-handled-13d49d5-before.json) → [160b364](results/2026-10-18-wsp2-handled-160b364-after.json) | handled_*; Node 26.11.0, Bun 1.4.3; matched; all four sizes | n=10^6, Sakura/JS ratio Node / Bun: d1 12.51× → 0.86× / 30.42× → 0.95× (20.46 → 1.39 ms Node); d4 13.37× → 0.86× / 28.39× → 1.46×; d16 20.54× → 0.84× / 45.81× → 1.18× (31.88 → 1.57 ms Node); fwd 13.24× → 9.02× / 17.87× → 14.33×; suspend 8.94× → 5.18× / 3.54× → 4.07× (Bun inconclusive, see A/B: 0.64× head/base); reentry 1.02× → 1.03× / 7.52× → 7.14×. Bun d1/d4 at n=1000–10000 stay ~5× (plan §8 WS-P2 item 11) | [compare](results/2026-10-18-wsp2-9fa7d68-compare.json): **blocked**. Time blocks on 11 non-handler programs and a complexity change of the unchanged hand-JS binary_trees/Bun control, under host load 9.4–13.1. An interleaved fresh-process A/B ([A/B](results/2026-10-18-wsp2-ab-under-load.txt)) puts every blocked cell within ±5% by minimum except handled_reentry/Bun n=100000 (min +14%, median −5%: inconclusive). Peak RSS: **handled_suspend/Node n=10^5 and 10^6 88 → 121 MB (+37%), open block** (plan §8 WS-P2 item 10) | Every semantics fixture under both ABIs with trace and stderr parity; `handler_cancel_bound`; runtime register tests; differential 2000 | Improvement: a perform bound to its handler literal is one register check plus a direct call of the clause twin, so the loop has no find, context, continuation or trampoline; checks and traces unchanged. Measured under load from other agents' jobs; the suite verdicts are not adoption evidence |
| WS-A `handled_suspend` RSS protocol (§17.156) — **advisory** | `c4da994` → `6b53ec7` (fetched master, includes `ce9ebe1`); [report](../research/10-evidence/suspendrss/README.md), [raw readings](../research/10-evidence/suspendrss/readings.json), [GC summary](../research/10-evidence/suspendrss/summary.json) | handled_suspend; Node 26.11.0; n=100000 and 1000000; five fresh processes per build/size/default-or-cap arm, interleaved; 60 calls/process | Memory diagnostic only, no timing ratio. Post-GC heap: ~5.18 → ~5.17 MB, head ~11 kB smaller. At n=10^6, new-space commitment 32–64 → 64 MiB, no natural mark-compacts; old-space used does not rise. Cap=8 MiB: maxima n=10^5 72.712 → 69.894 MB; n=10^6 72.401 → 70.271 MB | No `bench compare` or new confirmation-group claim. **Strict three-reading reproduction incomplete**: capped peaks are lower, not literally equal; uncapped n=10^6 maxima 123.617 → 122.061 MB because four baseline processes also expand. Supports nursery sizing and observes no retained-heap increase; existing designer warning stands, unresolved reproduction/equality check routed to **WS-P2b** | Both builds pass all eight checked-in oracle inputs and all 2400 diagnostic call checksums; not a new trace/`--no-direct` semantics run | [Load](../research/10-evidence/suspendrss/load.jsonl) 14.16–17.97 on 12 CPUs (quiet limit 3); fixed-work verbose-GC diagnostic, not calibrated harness timing. No equality tolerance invented; no baseline change/bless; quiet-window adoption still pending |
| §17.138 one-use reference temporaries inlined into the next call (WS-P2) | `13d49d5` → `0a3f4b4` (pre-rebase), confirmed in `c4da994` → `9fa7d68`; [allocation variant](results/2026-10-18-wsp2-binary-trees-8942a62-alloc-variant.json) kept as a failed measurement | binary_trees; Node, Bun; all sizes | n=14 Node: 40.32 → 40.41 ms, 1.91× → 1.68×; peak RSS maximum 292 → 168 MB (five processes 157–168). Against the pre-WS-P3 run ([before](results/2026-10-18-d88970189598-wsp3-before.json) vs [after](results/2026-10-18-wsp2-full-0a3f4b4-after.json)) `bench compare` is **ok** | [full 13d49d5 → 0a3f4b4](results/2026-10-18-wsp2-full-0a3f4b4-compare.json) carries the same load-driven time blocks as above and the handled_suspend RSS block; no binary_trees RSS crossing | `direct_inlined_bind_blame` (D020, producer), `direct_inlined_bind_defect` (D021, consumer), both ABIs with stderr parity; `one_use_temporaries_do_not_outlive_their_consumer` snapshot | Inlining into an allocation (the first variant) made `new Tree(1, [r, bottom_up$d(…)])` allocate the array before the call: 257–290 MB, no better. Calls only |
| WS-P5 contracts (§17.115): blame site as a parameter, `D011` bracket elided around total predicates, self-tail blame | `ce9ebe1` clean → WS-P5 tree on `ce9ebe1` (`dirty: true`, the uncommitted change, committed as the two WS-P5 commits `fix(core)` and `perf(codegen)`); [before](results/2026-10-18-wsp5-ce9ebe1-before.json), [after](results/2026-10-18-wsp5-ce9ebe1-tree-after.json) | contracts, contracts_plain; Node 26.11.0, Bun 1.4.3; matched; all four sizes | contracts n=10^6: Node 4.971 → 3.562 ms, 3.24× → 2.35×; Bun 4.653 → 3.215 ms, 2.10× → 1.43× (every size improved 23–74% in ratio). contracts_plain n=10^6: Node 2.324 → 2.45 ms, 2.47× → 2.95×; Bun 1.516 → 1.618 ms, 1.08× → 1.15× | [compare](results/2026-10-18-wsp5-ce9ebe1-tree-compare.json): **advisory (host load 10.00)**; contracts improved >10% in all eight cells; contracts_plain flags four cells and blocks Node n=10^6 (+19.6%) | Every semantics fixture under both ABIs with trace and stderr parity; JS/WASM parity; `requires_in_a_self_tail_loop_blames_the_self_call` and `contract_brackets_only_where_the_predicate_can_defect` (both ABIs); `violated` runtime test; differential 2000 | Improvement: a successful `requires` no longer writes and clears the global `S` register, and `x >= 0` / `result > x` lose their `contract_enter`/`contract_exit` calls; `List.all`'s predicate keeps its bracket; checks unchanged. contracts_plain's emitted modules have no site or bracket code (only the runtime import line changed); its Node block is mostly the unchanged hand-JS control falling 0.94 → 0.83 ms between runs. Before at load 6.11, after at 10.00: direction only (§17.153) |
| WS-P8 fused checked reads (§17.151) — **advisory only** (§17.153) | `6b53ec7` clean → `cd97a62` (the WS-P8 commits on `6b53ec7`), each clean, run back to back; [before](results/2026-10-18-d88970189598-wsp8-before.json), [after](results/2026-10-18-d88970189598-wsp8-after.json) | spectral_norm; Node 26.11.0, Bun 1.4.3; matched; all four sizes | n=400: Node 49.76 → 22.70 ms, 9.92× → 4.68×; Bun 79.10 → 12.53 ms, 12.58× → 2.06×. Every size: Node −48% to −53% in ratio, Bun −82% to −84%. Unchanged hand-JS controls moved +0.1% to −7.4% (Bun n=200) | [compare](results/2026-10-18-d88970189598-wsp8-compare.json): **advisory (host load 17.98)**, every cell improved > 10%, no RSS crossing | New `fused_get_expect`, `fused_get_expect_out_of_range`, `fused_get_expect_map_miss` under both ABIs (stderr identical) and on WASM; `build_dump_core_fuses_get_expect`; `SAKURA_DIFF_PROGRAMS=2000` differential with a fused-read form (2000 agree, 40 end in `D030`) | Improvement: `Option.expect(v[i], …)` emits `$rt.list_get_expect(v, i, …)`, which reads through `list_at`'s leaf cache and allocates no `Some`. Load 13–18 (other agents' differential runs): direction only. Bun meets §17.151's ≈2.1×; Node 4.68× is above its ≈3.4×, to be read at the quiet window |
| WS-P2b resumable loops (§17.156 (2)) — **advisory** (§17.153) | `meta.commit` is `522a3f9` in both files (HEAD when run): before = that tree with `emit.rs` restored to `d2cd5f8` (`dirty: true`, the reverted file), after = `522a3f9` itself, whose compiler is `e2c9092`'s (`522a3f9` only adds fixtures); landed rebased onto later master; [before](results/2026-10-18-wsp2b-d2cd5f8-before.json), [after](results/2026-10-18-wsp2b-e2c9092-after.json) | handled_*; Node 26.11.0, Bun 1.4.3; matched; all four sizes | n=10^6, Sakura ms and Sakura/JS ratio: handled_suspend Node 13.06 → 3.793 ms, 5.31× → 1.38×; Bun 14.85 → 0.7904 ms, 2.30× → 0.13×. handled_fwd Node 19.12 → 17.63 ms, 8.89× → 8.75×; Bun 14.57 → 10.69 ms, 13.49× → 9.99×. d1/d4/d16/reentry: `run` byte-identical (only `main`'s argument loop changed) | [compare](results/2026-10-18-wsp2b-e2c9092-compare.json): **advisory (host load 23.64)**; handled_suspend improved >10% in seven of eight cells; one block, handled_fwd/Node n=10^5 +13.9%, which an interleaved A/B ([A/B](results/2026-10-18-wsp2b-fwd-ab-under-load.txt)) puts at head 4% faster (min 1.72 vs 1.78 ms, 10 rounds): load. Peak RSS handled_suspend/Node maximum n=10^5 122.8 → 64.3 MB, n=10^6 120.8 → 52.7 MB | Every semantics fixture under both ABIs with trace and stderr parity, JS/WASM parity, new `loop_resume_twice` (two suspensions per iteration, direct and forwarding clause) and `loop_resume_edges`; `loops_with_unbound_performs_resume_at_their_head` snapshot; differential 2000 | Improvement: a loop whose CPS steps are unbound performs runs as `for (;;)`; a twin's result continues inline, and only a CPS clause or builtin allocates the continuation `$sN(live…)`. **RSS cause confirmed** ([readings](results/2026-10-18-wsp2b-suspend-rss.txt)): 12 interleaved processes 82–117 → 51–61 MB (n=10^5), 114–115 → 49.6–50.0 MB (n=10^6); post-GC heap 4.81 MB both; `--trace-gc` 850 → 2 scavenges, no mark-compact; semi space capped at 8 (diagnostic) 66 → 50 MB. The per-iteration closures were the allocation behind the nursery growth (plan §8 WS-P2 item 10). Load 10–25 throughout: direction only |
| WS-P9 mutual tail calls merged into one loop (§17.160 (5)) — **advisory** (§17.153) | `b3ad344` clean → `bd869ef` (the WS-P9 commit on `b3ad344`), each clean, run back to back; landed rebased as `ad6e2e1`; [before](results/2026-10-18-wsp9-b3ad344-before.json), [after](results/2026-10-18-wsp9-bd869ef-after.json) | countdown; Node 26.11.0, Bun 1.4.3; matched; all four sizes | n=300000: Node 0.918 → 0.653 ms, 1.91× → 1.36×; Bun 0.312 → 0.211 ms, 1.34× → 0.91×. Every cell improved 23–83% in ratio; Node n=100000 (2.46× → 1.91×) stays the noisiest cell. WASM `stats()` at n=1000: frames 1,006 → 4 (direct), 1,007 → 7 (`--no-direct`) | [compare](results/2026-10-18-wsp9-bd869ef-compare.json): **advisory (host load 16.28)**; every cell improved > 10%, no RSS crossing | Every semantics fixture under both ABIs with trace and stderr parity, JS/WASM parity, new `mutual_tail_loop` and `mutual_tail_blame`; `mutual_tail_calls_share_one_loop` snapshot, `requires_in_a_mutual_tail_loop_blames_the_member_call` (both ABIs); LIR corpus and `bench_counters` reviewed | Improvement: `is_even`/`is_odd` no longer leave the direct twin after the gauge's budget and finish through frames; each is one loop with the other nested as a join point. `sum_tail`'s half is unchanged. Load 10–16 (other agents' jobs): direction only, `countdown`'s halves are still split at the quiet window (§17.154 item 3) |
| WS-P11a tag compare for field-less equality (§17.183 item 1) — **advisory** (§17.153) | `b4aa556` clean → `deb9a20` (the WS-P11a commit on `b4aa556`), each clean; landed rebased as `71625ab7`; harness pair [before](results/2026-10-19-wsp11a-b4aa556-before.json), [after](results/2026-10-19-wsp11a-deb9a20-after.json); controlled [A/B](results/2026-10-19-wsp11-ab.json) | deltablue (after the projection-check boundary below); Node 26.11.0, Bun 1.4.3; A/B at n=1000 and 5000, eight fresh processes per cell, interleaved | A/B medians: Node n=1000 43.5 → 39.6 ms, n=5000 283 → 249 ms; Bun 63.1 → 59.9 ms, 374 → 365 ms (ratios against the unchanged control: about 67× → 61× Node n=1000). Harness pair not read: cells moved −26% to +106% between loads 26 and 40 | [compare](results/2026-10-19-wsp11a-compare.json): **advisory (host load 40.11)** | `tag_eq`, `tag_eq_defect` under both ABIs with trace parity and on WASM; `build_dump_core_compares_tags`; LIR corpus reviewed; `bench_counters` unchanged; `shapes` emits a byte-identical module | Improvement: the seven `$rt.eq(dir, …)` sites are tag compares; no `eq` frame in a Node profile after. The gap is still ~60×: `hget` 36%, `setAt` 13% (WS-P11b row) |
| WS-P11b per-map `Int` key kind (§17.183 item 1) — **not landed** | `deb9a20` runtime → the same with the key kind (uncommitted; described in plan §8 WS-P11b); [A/B](results/2026-10-19-wsp11-ab.json) | deltablue n=1000/5000; map_set `run_int`/`run_string` n=10^4/10^5; Node, Bun; six to eight fresh processes per cell | deltablue Node +2% / +4–5%, Bun −1% / +2%; map_set:int within noise (Node −10% / +4%, Bun −2% / +0.5%); map_set:string Bun n=10^5 +6% | none (ad hoc A/B) | Runtime tests passed; not landed | Flat to slower: the `Int` key cost is the trie walk (first touch of each node) and the log's `setAt` copies, not the key's type dispatch; a node-discrimination variant was no better. Load 9–11 |
| WS-P14 owned `Float`-record lists and flat `List<Bool>` (§17.265 (6)) — counted (§17.264) | `f56e0fec` (`dirty: true`: the code change reverted) → WS-P14 tree on `f56e0fec`; [before](results/2026-10-20-d88970189598-wsp14-before.json) (`nbody`, `sieve`, `spectral_norm`), [after](results/2026-10-20-d88970189598-wsp14-after.json) (the whole `js` suite) | nbody, sieve, spectral_norm; Node 26.11.0 (`--predictable`), Bun 1.4.3; all four sizes | Instructions per call at the largest size, before → after, and Sakura/hand-JS ratio against the blessed baseline: nbody n=10^5 Node 1.351 G → 817 M (−39.6%), 11.90× → 7.23×; Bun 1.222 G → 584 M (−52.2%), 8.89× → 4.18×. sieve n=200000 Node 80.5 M → 53.9 M (−33.0%), 4.62× → 3.08×; Bun 87.6 M → 43.0 M (−50.9%), 6.98× → 3.42×. spectral_norm n=400 Node 754 M → 753 M (−0.2%), 4.49× → 4.49× | [compare](results/2026-10-20-d88970189598-wsp14-compare.json) against `baselines/d88970189598.json`: **blocked** on two Node cells, both named: handled_reentry n=1000 +16.6% (in-place A/B 451 K → 527 K instructions, bisected to `list_at`'s flat branch; V8 then stops inlining `fromLeaves` into std's `range`; n ≥ 10^4 unchanged) and shapes n=50000 +29.1% (not this change: the same directory with the old runtime counts 454 M in this worktree, and copies of it elsewhere count 380–392 M with either runtime, the baseline's 381 M; unexplained, environmental). Flagged: map_set:int n=10^5 +6.0% and map_set:string n=10^4 +6.5% (old and new runtime count the same 894 M in place: not this change); sieve sakura/node peak RSS 89 → 147 MB at n=200000 (one 1.6 MB array per list, grown by copying: large-object garbage until a full GC), needing a confirmation group | `owned_fresh_result` under both ABIs with trace parity, on WASM and through the LIR corpus; its `--dump owned` pinned in `cli.rs`; runtime test of flat lists, spills and shared tails; `owned_updates.txt` nbody 4 → 46,004 under `audit_owned` | Improvement: (i) a def whose result is fresh now consumes a parameter its owned entry updates in place, so `advance` gets `advance$o` and its ten `List.set`s per step write the bodies' list in place; (ii) an owned push loop of booleans keeps one flat array, so `sieve`'s `List.set` is a store. Not landed, measured by hand-edited modules: flat `Float`/`Int` lists (spectral_norm +5%: a flat read misses `list_at`'s cache), flat lists of references (rbtree +23%: the collector), scalar replacement of `advance`'s loop-carried body (−7% after (i), under the 10% bar), in-place element records (−32% more, needs a proof that `bs[i]` and `bs[j]` are distinct objects). Load 15–65: counts read regardless (§17.264) |
| WS-P14 compare re-read with the A/A recount (§17.272 (2)) — counted | the same two files: blessed `320c9980` → [WS-P14 after](results/2026-10-20-d88970189598-wsp14-after.json); recount artefacts built from `320c9980` and from `main` after `0ef768c9` (WS-P14's emitter and runtime; no benchmark path changed since `7790603e`) | handled_reentry n=1000, shapes n=50000; Node 26.11.0 (`--predictable`) | Instructions per call, blessed / recount (`320c9980` artefact) / head, five interleaved pairs each at one location: handled_reentry 450.5 K / 467.8 K / 528.0 K (head/recount +12.9%); shapes 381.0 M / 485.9 M / 520.2 M (+7.1%; the recount is 27.5% off the blessed cell: `location`) | [compare](results/2026-10-20-d88970189598-wsp14-recount-compare.json) ([table](results/2026-10-20-d88970189598-wsp14-recount-compare.md)): **blocked** on one Node cell, handled_reentry n=1000, carried under §17.272 (1) (the `list_at` inlining boundary; WS-P20 makes the bounded attempt); shapes n=50000 **flagged**, `location`. Bun rows advisory; the sieve Node RSS crossing still needs its group (WS-P20) | No code change | Cause session ([bench/README.md](README.md#location-and-the-aa-recount-17272-2)): the location sensitivity is a GC-phase alias, not the identity-hash sequence. With the young generation fixed at 64 MB the same pair reads shapes +0.4% (370.6 → 372.1 M) and handled_reentry +13.0%: shapes' residual +7–8% in the default configuration is the collector's phase moved by a larger runtime file, not more work; handled_reentry's is code. No flag added to the Node command line (a re-bless question for the design lead) |
| WS-P15 `Map<Int, _>` integer index and the `Map.get` fusion (§17.275) — counted (§17.264) | `0216e570` → WS-P15 tree on `0216e570` (`dirty: true`, the uncommitted change); [after](results/2026-10-20-d88970189598-wsp15-after.json), [confirmation group](results/2026-10-20-d88970189598-wsp15-after-confirm.json), [arms and same-location readings](results/2026-10-20-d88970189598-wsp15-ab.txt) | map_set, map_set:int, map_set:string, map_set:tuple; Node 26.11.0 (`--predictable`), Bun 1.4.3; all four sizes | Instructions per call at n=100000 against the blessed baseline: map_set:int Node 839.7 M → 522.5 M (−37.8%), 6.21× → 3.84×; Bun 865.0 M → 700.4 M (−19.0%), 13.23× → 10.69×. map_set Node 3.176 G → 2.674 G, 3.28× → 2.75×. map_set:string Node 2.26× → 2.48× and map_set:tuple 2.75× → 2.76× in the file; at one location (base and head artefacts side by side) string 985.1 → 989.5 M (+0.4%), n=10000 +0.9%, tuple +0.7%, int −38.5%. Arms (WS-P13a, hand-edited runtime, map_set:int): the integer index −33.1%, the `map_get_or` fusion −1.2 points more (its bucket, 200,000 gets, −18.6%), the owned insert in one walk −4.4 points more | [compare](results/2026-10-20-d88970189598-wsp15-compare.json) ([table](results/2026-10-20-d88970189598-wsp15-compare.md)) with the confirmation group, re-read under §17.283 (3) ([one-call compare](results/2026-10-20-d88970189598-wsp15-compare-one-call.json), [table](results/2026-10-20-d88970189598-wsp15-compare-one-call.md)): **flagged**, no count and no RSS cell blocked (first read **blocked** on three Node many-call peak RSS crossings; map_set:string n=10000 +7.4% and n=100000 +9.5% flagged: location, +0.9% and +0.4% at one location). RSS: map_set:int n=100000 264 → 377 MB is the call count (k 8 → 16; 264 → 264 MB at equal k); map_set n=10000 187 → 268 MB and map_set:string n=50000 223 → 291 MB are the fusion: fewer young allocations move V8's mark-compacts (two instead of three, the last at 157–183 MB instead of 104–125 MB) while the last post-GC heap is lower (22.4 vs 29.2 MB, 16.4 vs 22.5 MB); ruled a harness defect under §17.283 (the many-call peak depends on `calls`; the one-call peaks read 121 → 98, 73 → 72, 84 → 80 MB): WS-P15 accepted and closed. One-call peaks (the gate since §17.283), base → head (confirmation): map_set:int n=100000 121.2 → 98.3 (98.8) MiB, map_set n=10000 73.1 → 72.5 (72.5) MiB, map_set:string n=50000 84.0 → 80.0 (79.8) MiB, all `ok`; the many-call peaks above are printed as a diagnostic | `map_int_keys` (0, −1, ±(2^53 − 1), 2^31, 2^32 ± 1, 32-bit and level edges, compaction), `map_int_shared` (persistence after sharing, root growth, negative trie), `map_int_json`, `fused_get_or` (Int, String, tuple and record keys; an `Option` used twice is left alone) under both ABIs, on WASM and through the LIR corpus (wasmtime skips `map_int_json`: JSON needs JS descriptors); `build_dump_core_fuses_get_or`; runtime tests: a model check of 12,000 operations over safe-integer keys to 2^43 with retained versions and owned/persistent index equality; rt `fused_get_or`; `bench_counters` map_set n=1000 dups 15,011 → 12,011, frames 9,005 → 3,005 (all-frame) | Integer index: positions in 32-slot arrays under `HNode` branches, no hash, `HLeaf` or `HColl`; std's `Map.contains` (`get(m, k).is_some()`) is now `map_contains`, and a get sinks past at most four lets to its consumer (`unwrap_or(-1)`). Updates (50,000 of 162.9 M) are the log path and two lookups in std's `Map.update`, untouched (§17.275 (3)). Load 15–50 (other agents): counts read regardless |
| WS-P20 cold-path raising (§17.274), with §17.272 (1)'s `list_at` attempt and (4)'s `sieve` RSS group — counted (§17.264) | `0216e570` (`dirty: true`: the change reverted in the WS-P20 worktree) → WS-P20 tree on `0216e570`; [before](results/2026-10-20-d88970189598-wsp20-before.json) (seven `--filter` runs in one session, concatenated: `spectral_norm`, `nbody`, `sieve`, `countdown`, `contracts_plain`, `contracts`, `handled_reentry`), [after](results/2026-10-20-d88970189598-wsp20-after.json) (the whole `js` suite), [`sieve` RSS group 2](results/2026-10-20-d88970189598-wsp20-sieve-rss-group2.json) | the seven programs; Node 26.11.0 (`--predictable`), Bun 1.4.3; all four sizes | Instructions per call, before → after (same worktree, same session), Sakura/hand-JS ratio: **spectral_norm** n=400 Node 754.5 M → 556.8 M (−26.2%; every size −26.1% to −26.6%), 4.49× → 3.31×; nbody n=10^5 Node 819.2 M → 774.1 M (−5.5%), 7.26× → 6.85×; sieve n=200000 Node 54.08 M → 53.94 M (−0.2%); countdown n=300000 16.0 M → 16.0 M (+0.1%); contracts_plain n=10^6 72.48 M → 72.49 M (+0.0%); contracts n=10^6 119.2 M → 119.5 M (+0.2%: the `try` bracket of `total`'s kept predicate costs nothing measurable); handled_reentry unchanged at every size (n=1000 526 K → 526 K). Bun: every cell within ±3.3% except spectral_norm n=400 (486 M → 543 M, flagged), which is calibration: the before file counted 16 calls, the after 8, and the two artefacts counted by hand at equal calls read 524 → 542 M at 8 and 506 → 492 M at 16 (spreads up to 12%); the `--processes 9` rerun reads 537 M at 8 calls | [compare](results/2026-10-20-d88970189598-wsp20-compare.json) ([table](results/2026-10-20-d88970189598-wsp20-compare.md)) against `baselines/d88970189598.json`: **blocked** only on the carried handled_reentry n=1000 (recount 471.1 K, head 527.7 K, +12.0%: the WS-P14 reading, §17.272 (1)); map_set:int n=10^5 +6.0% and map_set:string n=10^4 +6.1% flagged (§17.272 (3), the same as at WS-P14); shapes n=50000 and n=100000 `location` (head/recount +4.3%, +2.0%). No other Node cell flagged. [RSS compare](results/2026-10-20-d88970189598-wsp20-rss-compare.md) with group 2 as the confirmation head: `sieve` Node many-call peak RSS 89 → 147.6 MB at n=200000 (group 2 147.4 MB) and 69 → 121.6 MB at n=50000, confirmed by the second group under the gate of the time; re-read after rebasing onto §17.283's gate (the one-call peak; `--no-recount`, same files, no re-measurement) every RSS row is **ok**: `sieve` n=200000 one-call 56.3 → 58.1 MB (group 2 58.3 MB), n=50000 52.5 → 52.8 MB; the many-call crossing ran at 128 calls against the baseline's 64 (the head is faster), so §17.272 (4)'s confirmation reads no crossing and nothing is accepted under it | Every `expect-defect` fixture on JS and WASM (the workspace gate; the native lane was not run), `json_contracts.rs`, the LIR corpus and the compat suite pass unchanged; new fixtures `defect_D011_predicate_calls_cps_code` (a predicate's call into CPS code under `runSync`), `defect_D001_after_a_kept_bracket` (a defect after the exit keeps its code), `owned_argument_used_after_call` (§17.278, `--dump owned` pinned); runtime tests of the frozen instances and `contract_failed` | What moved `spectral_norm` is `list_get_expect` throwing inline (its `defect` call on the failing arm kept V8 from treating the arm as cold); the emitted arithmetic arm is `throw $rt.add_defect(a, b)`, which counts the same as the old call on every program measured, while `BigInt` code inline in the arm cost `handled_fwd` n=10^4 +27% (V8 counts it against inlining) and was not kept. **The `list_at` attempt did not land** (§17.272 (1)): two shapes, each counted by hand in place against the kept form. The cached read with the miss out of line brings `handled_reentry` n=1000 back down (0.55 → 0.49 M) but costs `sieve` n=200000 +2.0% (every flat read is a miss) and `contracts_plain` +19.6%, `contracts` +16.1% (the fold over a 16-element list); also reading a trie-less list's tail inline gives `sieve` −3.2% and `handled_reentry` 451 K counted (blessed 450.5 K), but `contracts_plain` +25.1% and `contracts` +20.6% (the first full compare blocked on both). A Node cell flagged either way, so the carry stands and `list_at` is unchanged. §17.278: the fixture's later `set` is `shared(borrowed-after)`; under the rule before WS-P14 it was `shared(escapes-call bump_all)` (the plain entry does not borrow the parameter it consumes, and a consumed parameter never does), so no site is lost and the call-site rule is not refined. Every count here was taken before §17.282's fixed young generation joined the Node line, with the baseline's flag set (§17.282: WS-P20 compares against the file `main` has); the tree was then rebased onto WS-P15 without re-counting |
| WS-W5 contracts on WASM (§17.285 (2b)): no `D011` bracket around a predicate that cannot raise — counted (§17.264) | blessed `3bf2343f` ([`baselines/wasm`](baselines/wasm/d88970189598.json)) → `90199110` (clean, before rebasing; it landed as `d2534e3e`, whose `lower.rs` differs only by formatting, so the same LIR); [after](results/2026-10-20-d88970189598-wsw5-after.json) (the whole `wasm` suite, `--engine node`) | every program; Node 26.11.0 (`--predictable --min-semi-space-size=16 --max-semi-space-size=16`); all sizes | Instructions per call, blessed → head: **contracts** n=1000 3.21 M → 2.90 M (−9.5%), n=10^4 30.70 M → 27.69 M (−9.8%), n=10^5 302.71 M → 271.98 M (−10.2%), n=10^6 3,019 M → 2,713 M (−10.1%); ratio 68.08× → 61.25× at n=10^6. **contracts_plain** unchanged (n=10^6 1,660 M → 1,663 M, +0.2%; 68.6× → 68.7×). Every other Sakura cell within ±0.8% | [compare](results/2026-10-20-d88970189598-wsw5-compare.json) ([table](results/2026-10-20-d88970189598-wsw5-compare.md)): **ok**; contracts n=10^5 and n=10^6 `improved >10%` (explained here), n=1000 and n=10^4 −9.6% and −9.9% on the ratio; nothing flagged or blocked | The workspace gate: every `expect-defect` fixture on JS and WASM, `json_contracts.rs` (the `--json` `defect` record), the LIR corpus (27 enter/exit pairs elided in 19 print snapshots, nothing else), the native corpus and `bench_counters.rs` (direct and `--no-direct`) unchanged; no new trace record | **Attribution** (WS-P13a's protocol: the emitted `main.wasm` hand-edited as WAT, three pairs per arm, n=10^5, before landing): emitted 302.9 M; removing `increment`'s four bracket calls and their four `rt_take_defect` tests 274.7 M (−9.3%), also `total`'s (it wraps the call of `all`) 273.5 M (−9.7%, so that bracket stays); landed 272.0 M. What remains between `contracts` (2,713 M) and `contracts_plain` (1,663 M) is `List.all`'s closure loop in `total`'s predicate (≈1,000 instructions an element): `rt_dup`/`drop`/`list_at`/`rt_box_i64` calls, not the check. **`contracts_plain` has no contract, so no contract mechanism can move it**: in it `total`'s fold is 85% of the count (`run`'s loop alone 25.6 M at n=10^5), and the arms that move it are outside a contract: an inline immediate test before `drop` −4.5% (the `box ()`+`drop` of every loop's `Unit` join parameter, ABI errata L8), an inline `rc > 1` decrement −13.5% (it changes what ABI §10's `drops` counts), `cx::wasm::rt()`'s 1,376-byte frame on every runtime call returned early −3.2% (`cx.rs`). Filed as a `REQUESTS.md` design-question; the lane's exit is not met on `contracts_plain` nor on `contracts` at n ≤ 10^4 |
| WS-W6 `tokenizer`'s superlinear cost on WASM: `concat` consumes its left string and appends in place (§17.285 (c)) — counted (§17.264) | blessed `3bf2343f` (`baselines/wasm/d88970189598.json`) → `8ede0ecf` (clean; the change before its rebase onto WS-W5, landed as `1580157d`, whose code is identical); [after](results/2026-10-20-d88970189598-wsw6-after.json) (the whole `wasm` suite, Node and Wasmtime) | tokenizer, json_validate (and every other program, unchanged); Node 26.11.0 (`--predictable`, semi-space 16), Wasmtime; all four sizes | Cause, from counts across sizes: WASM instructions per call grew 21.9 M → 362 M → 5.03 G → 18.2 G at n = 10^3, 10^4, 5·10^4, 10^5 (log-log slope 1.45; hand JS 1.02), because `s = "{s}{w}{sep}"` copied all of `s` twice per iteration: the LIR's `concat` borrowed both operands, so `rt_concat` never saw a unique left string. After: **tokenizer** Node 17.74 M / 170.8 M / 853.7 M / 1.704 G (n = 10^5 −90.6%), Sakura/hand-JS 4.51× / 7.38× / 19.17× / 34.28× → 3.65× / 3.49× / 3.25× / 3.21×; slope 0.992 against hand JS's 1.021 (2.8% apart; the exit asks 10%); Wasmtime n = 10^5 18.52 G → 2.07 G, slope 1.425 → 1.001. **json_validate** Node, the same cause: n = 30000 13.56 G → 3.375 G (23.66× → 5.89×), slope 1.153 → 0.924 | [compare](results/2026-10-20-d88970189598-wsw6-compare.txt) against `baselines/wasm/d88970189598.json`: **ok**; no cell flagged or blocked, no RSS row over its gate; improved > 10%: tokenizer every size on both engines (−17.7% to −90.6%), json_validate n = 10^4 (−52.3%) and 3·10^4 (−75.1%), all from this change | Checksums and oracles green on both engines; `cargo test --workspace` (parity JS/WASM corpus, LIR interpreter, differential evaluator, native evaluator) green; LIR print snapshots lose only `drop` lines after `concat`, five validator rows gain one `Dup` where the left operand stays live; `bench_counters` drops fall one per `concat` | Loaded host (one-minute load up to ~46), read on counts only; wall times not quoted |
| WS-P21 list-element ownership (§17.273) and the moved-record reuse, unsound as first landed and re-landed under §17.290 (6) — counted (§17.264); **accepted below its 25% exit** (§17.290 (8): the bucket reachable without `j > i`) | `65ed5852` (`dirty: true`: the change reverted in the WS-P21 worktree) → `2dfdcd58` (the ruled site) → `555a2d84` (the second shape); [before](results/2026-10-20-d88970189598-wsp21-before.json) (`nbody`), [site](results/2026-10-20-d88970189598-wsp21-site.json) (the whole `js` suite at `2dfdcd58`'s code), [after](results/2026-10-20-d88970189598-wsp21-after.json) (the whole `js` suite at `555a2d84`). **Re-counted at the re-land** (§17.290 (8), one worktree, one session): `38ee331b` (`main` after the revert `c761a5f8` and the `list_get` fix `8a52a865`) → `81852064` (the re-land with conditions (i) and (ii)), both clean; [before](results/2026-10-21-d88970189598-wsp21-reland-before.json) (`nbody`), [after](results/2026-10-21-d88970189598-wsp21-reland.json) (the whole `js` suite at `81852064`) | nbody; Node 26.11.0 (`--predictable --min-semi-space-size=16 --max-semi-space-size=16`), Bun 1.4.3; all four sizes | **Re-land: nbody n=10^5 Node 625.6 M → 580.7 M (−7.2%), 5.55× → 5.14×; −15.5% against `65ed5852`'s 687.4 M, of which the ruled site alone (at `38ee331b`) is −9.0%**; n=1000 6.16 → 5.72 M (−7.2%), n=5000 30.6 → 28.4 M, n=20000 125.0 → 116.1 M; Bun n=10^5 524.2 → 503.7 M (−3.9%). First landing, for the record: Instructions per call, before → site → after (one worktree, one session, the re-blessed flags), Sakura/hand-JS ratio: **nbody n=10^5 Node 687.4 M → 623.6 M (−9.3%) → 581.6 M (−15.4%), 6.08× → 5.52× → 5.14×**; n=1000 7.35 → 6.13 → 5.70 M (−22.4%); Bun n=10^5 574.9 → 529.0 → 501.8 M (−12.7%). Hand counts of the emitted module (same flags, three interleaved pairs, n=10^5, 8 calls): the site-only module 625.1 M, the head 581.0 M, and the head with `b`'s rebuild reused unconditionally 485.8 M (−29.5%): that edit, which is what §17.273's −32% measured, is unsound in general (slot `i` still holds `b`'s first object while the inner loop reads `bs[j]`, and only `j > i` makes it safe, which §17.273 rejects proving); an if/else guard counts the same as the ternary (581.3 M), so the guard costs nothing and the gap is the four first rebuilds a step | Re-land: [compare](results/2026-10-21-d88970189598-wsp21-reland-compare.json) ([table](results/2026-10-21-d88970189598-wsp21-reland-compare.md)) against `baselines/d88970189598.json`: **flagged**, on four Bun advisory cells only (contracts_plain n=1000, deltablue n=5000, handled_reentry n=10^5, shapes n=50000; base spreads 57%, 17%, 18%, 10%); one Node cell blocked (pipeline n=1000, +19.2% by ratio) and read `ok` by its A/A recount against `3bf2343f` (head/recount −0.5%); no Node cell flagged; nbody Node −20.3% to −28.0% by ratio is the improvement. First landing: [compare](results/2026-10-20-d88970189598-wsp21-compare.json) ([table](results/2026-10-20-d88970189598-wsp21-compare.md)) against `baselines/d88970189598.json` (the `3bf2343f` re-bless, same Node flags): **flagged**, on three Bun advisory cells only (handled_suspend n=10^5, handled_reentry n=10^5, deltablue n=5000; base spreads 52%, 18%, 17%; their ownership facts are unchanged, deltablue's `audit_owned` count included). No Node cell flagged or blocked; no RSS row crossed (nbody one-call peak 82 → 82 MiB at n=10^5). nbody Node −20.3% to −26.5% by ratio (the baseline predates WS-P20's −5.5%) and spectral_norm −24% (WS-P20) are the improvements | `owned_elements_shared` (one record at two indices: `[r, r]`, `push(xs, at(xs, 0))`; `shared(elements)`, the persistent result), `owned_elements_guard` (`b` live through every `bs[j]`; the guard fails at `i == j`), `owned_elements_moved` (the second shape; `bs[i]` read while `b` is rebuilt), each under both ABIs, on WASM and through the LIR corpus, `--dump owned` pinned for the three and for nbody; `audit_owned` tags each reuse whose guards pass: `owned_updates` nbody 46,004 → 56,005 → 62,005, every other benchmark unchanged; differential: every program ends with `spread_p` and `drift_p` on fresh records, `[r, q, r]` or a pushed element, 2,000 programs with the reuse on and 2,000 with `SAKURA_DIFF_ELEMENTS=0`. §17.290 adds `owned_elements_option` (CE1: a `bs[j]` `Option` held across the update; direct printed `ys 3:0 4:10 10:0` before `8a52a865`, now `ys 3:0 4:10 0:0`), `owned_elements_boxed` (CE2: was `boxes 0 3 3`, now `boxes 0 1 2`) and `owned_elements_carried` (CE4: was `box1 3`, now `box1 0`), each under both ABIs, on WASM and in the LIR corpus, `--dump owned` pinned with no in-place line; `owned_elements_moved` and nbody's `in place unless b%19 is r%11` pin unchanged; `owned_updates` nbody 62,005 at the re-land. The differential generator writes the three shapes (`hold_p`, `box_p`, `carry_p`), each confirmed to stop the evaluator with its condition undone; 2,000 programs with the reuse on and 2,000 with `SAKURA_DIFF_ELEMENTS=0` | The ruled site reuses `bs[j]`'s record (10 a step); the second shape reuses `b`'s once it is a moved fresh record (6 of its 10 rebuilds a step: the first rebuild after each `b = bs[i]` allocates). Below the 25% exit; what is left needs `j > i` (a range fact §17.273 rejected) or a reuse inside `List.map`'s callback (the 5 bodies a step `advance` returns). The second shape as first landed (`555a2d84`, on `main` `f39be4ac`) was unsound (§17.290: CE2, CE4) and is replaced by the re-land. Load 15–50 (other agents): counts read regardless. Re-land: Homebrew moved `node` to 26.12.0 during the first attempt (38 Node cells failed to load `libsimdutf.36`); both re-land files were counted on the pinned 26.11.0 keg through a `PATH` wrapper that sets `DYLD_LIBRARY_PATH` to simdutf 9.2.1 and execs it (the wrapper's start-up is in both processes of a pair and cancels); load 44 (other agents), counts read regardless |
| WS-W8 a `Unit` join parameter stays `Unit` (§17.287 (2)): `local_reprs` demotes it to a word only when a jump feeds it one (or it is a `try`'s catch parameter) — counted (§17.264); the bucket rule, not the 10% rule | blessed `3bf2343f` ([`baselines/wasm`](baselines/wasm/d88970189598.json)) → `11173cf1` (clean, on `7912098b`, before rebasing; it landed as WS-W8's `perf(lir)` commit, whose `lower.rs` differs only by `cargo fmt`, so the same LIR); [after](results/2026-10-21-d88970189598-wsw8-after.json) (the whole `wasm` suite, `--engine node`) | every program; Node 26.11.0 (`--predictable --min-semi-space-size=16 --max-semi-space-size=16`); all sizes | Instructions per call, WS-W5's [after](results/2026-10-20-d88970189598-wsw5-after.json) → head (this change alone): **contracts_plain** (target) n=1000 1.811 M → 1.730 M (−4.5%), n=10^4 −5.5%, n=10^5 166.83 M → 158.62 M (−4.9%), n=10^6 1,662.8 M → 1,580.7 M (−4.9%); **contracts** −5.2% / −5.7% / −5.6% / −5.7% (n=10^6 2,713 M → 2,560 M; −15.2% against the blessed file with WS-W5's bracket). Every other `while` program moves the same way: `spectral_norm` −8.4 to −9.4%, `sieve` −7.7 to −8.3%, `handled_d1`/`d4`/`d16` −5.5 to −6.9%, `handled_suspend` −4.9 to −5.1%, `pipeline` −2.6 to −3.0%, `nbody` −2.4 to −3.0%, `map_set:*` −0.7 to −2.3%, `handled_fwd` −1.3 to −1.7%, `handled_reentry` −0.3 to −1.2%; every other cell within ±0.8% | [compare](results/2026-10-21-d88970189598-wsw8-compare.json) ([table](results/2026-10-21-d88970189598-wsw8-compare.md)) against the blessed file: **ok**; `contracts` `improved >10%` at every size (WS-W5's bracket and this change, −14.2 to −15.2% on the ratio, explained here); nothing flagged or blocked | The workspace gate: every `expect-defect` fixture on JS and WASM, `--no-direct`, trace parity, the native corpus and the LIR corpus green. The LIR print diffs (47 snapshots on the landed base, 38 before rebasing) were reviewed with var and block numbers normalised: each is a join parameter `word` → `unit`, with the `box` of `()` that fed it (113) and its `drop` (97) gone, nothing else; `try` catch parameters stay `word`; the validator summary changes only in `drop` counts. `bench_counters.txt` changes only in `drops` (19 programs, direct and `--no-direct` alike) | **Hand count** on `contracts_plain` n=1000 (`bench_counters.txt`): `drops` 4,185 → 3,169 = −1,016, one per iteration of `run`'s `while` (1,000) and of `List.range(0, 16)`'s builder loop (16); `fold`'s accumulator is a real word and keeps its `drop`. `contracts` 8,278 → 6,270 = −2,008, the same 1,016 plus `List.all`'s loop in `total`'s `requires` (62 calls × 16). **`boxes` does not move, and cannot**: the box of `()` writes the immediate `UNIT` word (ABI §4) and allocates nothing, and `boxes` counts `IntBox`/`FloatBox` allocations, so §17.287 (2)'s "`boxes` and `drops` fall" is met on `drops` only; the bucket (one `drop` call per iteration) is empty. ABI 1.8.1 (L8) |
| WS-W9 the decrement in place (§17.287 (3)): compiled code's `Drop` tests `is_ptr`, the `static` flag and `rc > 1` inline and stores `rc − 1`, otherwise calls `drop`; in-place decrements counted in an internal `i64` global that `stats()` adds to `drops` — counted (§17.264) | blessed `3bf2343f` ([`baselines/wasm`](baselines/wasm/d88970189598.json)) → `aa17320b` (clean: WS-W8 then WS-W9 on `7912098b`, before rebasing; it landed as WS-W9's `perf(wasm)` commit, whose `func.rs` differs only by `cargo fmt`); [after](results/2026-10-21-d88970189598-wsw9-after.json) (the whole `wasm` suite, `--engine node`) | every program; Node 26.11.0 (`--predictable --min-semi-space-size=16 --max-semi-space-size=16`); all sizes | Instructions per call against the blessed file: **contracts_plain** (target) n=1000 1.809 M → 1.596 M (−11.8%), n=10^4 −13.5%, n=10^5 166.85 M → 145.17 M (−13.0%), n=10^6 1,660 M → 1,445 M (−12.9%); **contracts** −23.0% / −24.0% / −24.6% / −24.4%. This change alone, against WS-W8's [after](results/2026-10-21-d88970189598-wsw8-after.json): `contracts_plain` −7.7% / −8.5% / −8.5% / −8.6% (the −13.5% of the WS-W5 design-question was measured before WS-W8 took the `Unit` `drop`s, which are immediates and now never reach `Drop`), `contracts` −10.3 to −11.1%. Others that move, this change alone: down `rbtree` −8.6 to −11.1%, `spectral_norm` −9.1 to −10.1%, `shapes` −7.5 to −8.2%, `pipeline` −6.0 to −6.3%, `handled_d1`/`d4`/`d16` −5.5 to −6.1%, `handled_suspend` −3.7 to −4.4%, `deltablue` −3.4 to −8.2%, `nbody` −3.2 to −3.3%, `handled_fwd`/`handled_reentry` −1.5 to −2.4%, `map_set:*` −0.5 to −1.5%; **up `binary_trees` +2.6 to +3.2%** (named: its drops are mostly last references, which pay the inline test and then call `drop` as before); every other cell within ±1.1% | [compare](results/2026-10-21-d88970189598-wsw9-compare.json) ([table](results/2026-10-21-d88970189598-wsw9-compare.md)) against the blessed file: **ok**; `improved >10%` on the ratio (explained here): `contracts_plain` −12.2 to −13.6%, `contracts` −23.2 to −24.8%, `spectral_norm` −17.4 to −18.1%, `handled_d1`/`d4`/`d16` −10.5 to −12.3%, `rbtree` n ≥ 10^4 −10.3 to −11.0% (each the sum of WS-W8 and WS-W9 on the blessed file, not summed by hand); nothing flagged or blocked; `binary_trees` within the flag threshold | The workspace gate: every `expect-defect` fixture on JS and WASM, `--no-direct`, trace parity, `reuse_and_borrow_counters_reconcile_by_hand`, the §7.5 cycle and lifetime tests and the native corpus green. **`bench_counters.txt` byte-identical** (no `SAKURA_BLESS`); the control: with the global left out of `stats()`, `contracts_plain` n=1000 reads `drops` 1,122 instead of 3,169, so 2,047 of its decrements are made in place and all are counted. Native unchanged | ABI 1.8.2 (PATCH): §3.1's permission sentence, §10's `drops` redefined as the decrements compiled code asked for (in place or through the export) and the host's calls of the export. Only `Inst::Drop` decrements in place; the landing pads' and cold paths' drops still call the export. The global is not exported: the `stats` export reads it, so no host changes |
| WS-W3 the frame protocol's runtime cost (§17.218 (3), §17.287 (4)): (1) `rt()`'s construction out of line, (2) one `rt()` per step, (3) the dispatched frame, then the driven task, resolved once per step — counted (§17.264) and the twelve-call protocol | each on `7912098b`: (1) `6ef46a88` (landed as `b5438494`), (3a) `b72de168` (landed as `c15c24a6`), (3b) `91279484` (landed as `4bd05b5a`); (2) measured and not landed; counted runs of the whole `wasm` suite (`--engine node`) at [(1)](results/2026-10-21-d88970189598-wsw3-c1.json), [(3a)](results/2026-10-21-d88970189598-wsw3-c3.json), [(3b)](results/2026-10-21-d88970189598-wsw3-c4.json), each against the blessed `3bf2343f` ([`baselines/wasm`](baselines/wasm/d88970189598.json)) | **target** all-frame `fib(30)` (the WS-W3 scratch module, `--wasi --no-direct`, Wasmtime 49.0.3); **control** the same with twins and the `fib` cells | **Instructions per all-frame `fib(30)` call** (macOS `/usr/bin/time -l`, twelve-call process minus two-call process over ten, three processes): 7,368 M → (1) 6,751 M (−8.4%) → (3a) 6,132 M (−9.2%) → (3b) 5,134 M (−16.3%); **30.3% fewer in all, 1.44× fewer**. Twins 258.6 M at every step. Twelve-call protocol (eight processes an arm, interleaved, load 17–81, advisory): all-frame 353–423 ms → 189–217 ms after (3b), twins 13.6–15.9 ms unchanged within noise. (2), the `&mut Rt` of `after` carried into the next `next` in `run_body`, counted **+11.0%** (7,495 M against (1)'s 6,756 M): `wasm::rt` was already a load and a test after (1), and the merged body made `next` an out-of-line call; not landed under §17.201's rule | [(1)](results/2026-10-21-d88970189598-wsw3-c1-compare.json), [(3a)](results/2026-10-21-d88970189598-wsw3-c3-compare.json), [(3b)](results/2026-10-21-d88970189598-wsw3-c4-compare.json): each **ok**, nothing flagged or blocked; `fib` ±0.5% at every size (the twin path, the control); improved >10% (explained here): (1) every runtime call loses the 1,376-byte prologue, so every cell that calls the runtime moves, `contracts` −20 to −22%, `contracts_plain` −11 to −12% (−3.2% was the hand-edited estimate; recorded, not a gate), `handled_*` −13 to −16%, `rbtree`, `sieve`, `spectral_norm` −11 to −13%, `nbody` −10%, the rest 0 to −9%; (3a) on top of (1), same location: `handled_fwd` −5.8%, `handled_reentry` −4.2%, everything else ±0.6%; (3b) on top of (3a): `handled_fwd` −12.6%, `handled_reentry` −7.8%, `handled_d1/d4/d16` and `handled_suspend` −3.5 to −4.6%, everything else ±1% | The workspace gate (fmt, clippy, 714 tests) on each landed commit; no counter, snapshot or defect record changes (the change is inside the runtime's driver; a stale id still goes through `arena_obj`) | **`perf` buckets** (Linux `perf` 6.8, `cpu-clock` 997 Hz, Ubuntu 24.04 in OrbStack's AArch64 VM, `--profile perfmap`, the README's protocol, whole-process leaf shares): before `wasm::rt` 22.8%, `arena_obj` 14.9%; after (1) `wasm::rt` gone from the top sixteen (below 0.94%), `arena_obj` 20.0%; after (3a) `arena_obj` 13.7%; after (3b) `arena_obj` 2.1%; what is left is `fib` 17.8%, `pop_frame` 15.6%, `run` 12.4%, `Heap::alloc` 7.4%, `code` 6.8%, `call` 5.1%, `arena_new` 4.8%. **Acceptance (2×) not met**: 1.44× on counts. (4) the free list is not attempted: `frame_alloc`+`Heap::alloc`+`Heap::free` are about 14% leaf, so it cannot carry the rest of 2×, and a frame cache outside `heap.rs` would move the heap's `live`/`peak`/`free_list_bytes` statistics, which `heap.rs` (WS-W6's) owns. The next bucket is `pop_frame`'s per-slot `frame_kind` walk with `code` lookups. Load 7–81 throughout; the counted lane reads no load |

`5b10553` is a **benchmark-definition boundary**, not a comparable performance delta:
sieve changed from full-list passes to indexed clearing from p² through `List.set`,
in Sakura and every control/port. Do not compare any pre-change sieve ratio with the
new version, including the old `bench/results/2026-10-18-*` files. The WS-A historical
raw runs are retained, but sieve is excluded from their noise/comparison summaries.
The candidate uses the new definition in one fresh full-suite run.

WS-S10 adds two further **benchmark-definition boundaries** (§17.150).
All earlier nbody and countdown ratios, including the historical candidate's,
are excluded from every comparison and noise summary; raw evidence is retained.
These rows make no optimisation or performance-ceiling claim. The existing
local/CI noise subsets contain neither program and need no recalculation.

| Definition boundary | Source-blob evidence | Unchanged workload / verification | Measurement limitation |
|---|---|---|---|
| WS-S10 nbody: slice/push/concat helper → persistent `List.set` | [old/new identities](../research/10-evidence/ws-a/nbody-definition-change.json) | Five bodies, same pair traversal, Float order and energy checksum; every helper index is in range; [validation](../research/10-evidence/ws-a/wss10-validation.md) | Source idiom change; pre-change ratios are incompatible; new timings advisory under §17.153 |
| WS-S10 countdown: unchecked hand JS → checks after each arithmetic operation | [old/new identities](../research/10-evidence/ws-a/countdown-definition-change.json) | Same descending sum and parity; parity before sum, next-index check before accumulation, checked final addition; [audit](../research/10-evidence/ws-a/wss10-control-audit.md) | Control now performs the required checks; old denominator is incompatible; fresh quiet-window run needed |


The DeltaBlue definition boundary (§17.183 item 1) adds the missing Sakura checks
against every destination except the last after changing scale and offset, matching
the hand-JS control's AWFY projection checks 3 and 4. **All earlier `deltablue`
ratios are excluded from comparisons and noise summaries**, including the candidate
and the noisy round-14 run; retain their raw evidence. The checksums are unchanged,
but the timed validation work differs. A fresh quiet-window run is required.

| Definition boundary | Source-blob evidence | Unchanged workload / verification | Measurement limitation |
|---|---|---|---|
| DeltaBlue: add AWFY projection checks 3 and 4 (§17.183 item 1) | `bench/programs/deltablue/sakura/main.sak`: old `f1da8357e415e16240a224c3094ffb0b1bd49a63` (at `9728a44c`) → new `4f8af375a0b6370723d2b4b56a73650aed3afcd2` (Git blob hashes) | Same planner operations, traversal order and checksums; checks cover indices 0 through n−2, with n=0 returning early and n=1 checking no destinations. All eight oracle sizes (0, 1, 2, 3, 100, 1000, 5000, 10000) pass against unchanged `meta.toml` on Sakura and hand JS under Node and Bun, and Sakura WASM under Node | Benchmark-definition correction, no optimisation or timing claim; old ratios incompatible; no baseline blessed or timing expectations changed |

`d97cf247` is a **method boundary for `bench compile`**: from that commit each
fresh-process case runs once untimed before its 15 timed samples, so neither the first
sample's cold OS file cache nor the filling of std's analysis cache (design §17.195 (3))
lands in its median or p95. Compile-latency readings before `d97cf247` include that first
cold run; compare them with later readings by direction only, not as a delta.

For each optimisation landing, add a row linking both raw runs and the machine-readable
`bench/bench compare` output. Record the exact measured commits (not just the later
report commit), pinned engines, matched lane, input size, absolute Sakura times and
Sakura/hand-JS ratios. Include the same-size confirmation when needed, RSS and size-sweep
findings, plus links to that change's semantics and effect-trace parity checks. Explain
improvements above 10%, including any change to checks or work performed. Preserve failed
and noisy measurements and label inconclusive comparisons explicitly.

The suite changed from the historical research/10 microbenchmarks; do not put those
older ordinary-JS controls and current matched-suite controls in one before/after row.
A subsequent compiler change needs a fresh run; do not relabel these measurements with
that change's commit.

## Round-18 bars and representation carries

These are the design dispositions in §17.199 (3), §17.201 (2), and §17.211 (5).
They do not promote any loaded-host reading to an official ceiling result.

| Program and contract | Recorded evidence | Disposition and next reading |
|---|---|---|
| `countdown`, §17.118 direct-region bar | The [M4-OPT reading, §17.199 (3)](../docs/design.md) records seven frames on both measured paths, 103 versus 103 ms at n=300000, and 116 versus 113 ms at n=3000000, near the 101 ms Node startup floor. The current [small-size counter snapshot](../compiler/crates/sakura_cli/tests/snapshots/bench_counters.txt) separately records 4 direct versus 7 all-frame frames at n=1000. | **Met by counter**, per §17.199 (3). The historical ratio is "at floor" under the design's rule. No M5 carry. Read again at the §17.153 quiet window. |
| `fib`, §17.118 direct-region bar | M4-OPT reported approximately 3.9× in [REQUESTS](../compiler/REQUESTS.md), ratified in §17.199 (3). Historical advisory reading, not newly measured here. | Meets the bar on advisory evidence. Read again at the quiet window. |
| `nbody`, §17.118 direct-region bar | M4-OPT reported approximately 2.8× in [REQUESTS](../compiler/REQUESTS.md), ratified in §17.199 (3). Historical advisory reading, not newly measured here. | Meets the bar on advisory evidence. No carry of the **bar**. Its allocation ceiling remains carried below. Read again at the quiet window. |
| `deltablue`, JS representation ceiling | [WS-P13c](../research/10-evidence/ws-p13c/README.md) records the as-emitted List port at Node n=5000 as 0.32523× the historical Map baseline and 22.02× hand JS (advisory, loaded host; no contemporaneous Map measurement). The port replaces dense Planner maps with Lists under §17.217 (3a). | Fourth JS representation-bound carry retained. List path copies and allocation remain. WS-P13b reads ownership through parameters, fields, and tuple results on the List port under §17.217 (3b); §17.177 (1) reads the flat representation at its window. No baseline is promoted. |

## WASM allocation carries

Every number below is from the existing [WS-B advisory run](results/2026-10-19-wsb-wasm-advisory.md)
at `72aaf6fe`, with [raw records](results/2026-10-19-wsb-wasm-advisory.json).
The ratios divide WASM time by hand JS on Node. They are **not** the M4 envelope
against native ports. The host was loaded, with `quiet: false` and peak load 8.79.

| Program | Historical advisory ratio, Node / Wasmtime, first / second size | Allocation cause and carry |
|---|---|---|
| `nbody` | n=1000 / 5000. Node 74.42× / 74.59×. Wasmtime 107.03× / 108.22×. | Record updates and persistent `List.set` copies. §17.211 records about 2.5 KB requested and 42 dups per step. Its callback frames are separately assigned to WS-W2. Carry allocation to N0b's LIR ownership facts and owned `List.set`, N3, and N4. Read with the JS carry at §17.177's windows. |
| `sieve` | n=1000 / 10000. Node 48.94× / 48.73×. Wasmtime 81.58× / 81.81×. | Persistent sequence updates and representation overhead. Carry to N0b, N3, and N4, with owned sequence updates where proved. Read with the JS carry at §17.177's windows. |
| `spectral_norm` | n=50 / 100. Node 49.07× / 46.26×. Wasmtime 78.07× / 74.94×. | Persistent numeric sequences and allocation around indexed operations. Carry to N0b, N3, and N4. Read with the JS carry at §17.177's windows. |
| `rbtree` | n=1000 / 10000. Node 37.71× / 20.92×. Wasmtime 43.07× / 27.04×. | Persistent tree-node allocation and reference-count traffic, including deliberately retained versions. Carry to N0b, N3, and N4. Ownership does not permit mutation of retained versions. Read at §17.177's windows alongside the JS carries. |
| `binary_trees` | n=8 / 10. Node 7.77× / 7.31×. Wasmtime 13.55× / 13.54×. | Tree-node allocation and reference-count traffic. The counter snapshot already has only four direct frames at n=8. Carry to N0b, N3, and N4. Read at §17.177's windows alongside the JS carries. |
| `deltablue` | n=100 / 1000. Node 362.31× / 309.09×. Wasmtime 480.46× / 607.51×. | Planner records, persistent maps and logs, and boxed values. Carry to N0b, N3, and N4. Read with the JS carry at §17.177's windows, including question (6). |

The Wasmtime-over-Node factor is recorded, not pursued, per §17.211 (6).
The M4 envelope still needs the owner's quiet window after WS-W1 and WS-W2.
[WS-W3](../research/10-evidence/ws-w3/README.md) separately attributes the all-frame protocol.
Its 2× all-frame `fib` acceptance, twins unchanged, is M5 work and gates nothing; the WS-W3 row above reads 1.44× on counts after three fixes.

AI assistance: GPT-6-Astra prepared this ledger; GPT-6.1-Sol added the WS-S10 boundaries; the human contributor owns review.

## `v1.0.0-beta.1` envelope (`0a50bbe1`, 2026-10-21)

Counted, host loaded (advisory, direction only); every miss carried under §17.265.
Source: `bench/results/2026-10-21-0a50bbe1-envelope.md`.


## `v1.0.0-beta.4` baseline (`40af20ce`, 2026-10-22)

Counted (§17.264) at the preparation commit `40af20ce096074012bbd2344129da441589d3e6c` in
`../sakura-counted`, tree clean, every oracle green, Node 26.12.0 under §17.282's flags. Blessed
by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design §17.310 (1)); the
owner can overturn it. `baselines/d88970189598.json`, `wasm/` and `typescript/` hold the
`40af20ce` runs (the `472dea56` files are in `baselines/archive/472dea56/`), and
`baselines/compile-native/d88970189598.json` is the first counted native compile report (N2f
(iii), §17.308 (2b)): from 16,384 to 32,768 rows the median count grows 1.96× in `frontend`,
2.00× in `lowering`, 2.01× in `codegen`, 1.83× in `object` and 1.05× in `link`, so the report
names `lowering` and `codegen` superlinear, each at the 2× line; the round that reads it sets
§17.175 (4)'s budget. Spreads over 1%: no Node cell; one Wasmtime cell (`countdown` n=1000, the
first of five processes); 20 native-lane cells, all Go; Bun and `tsc` as before (advisory).
Envelope (wall-time lane, advisory, host load 4.35): `miss` on 237 of 264 gating cells (absent 4,
met 23), against 236 at beta.3; carried under §17.265. Prep `fib` and `shapes` compare `ok` on
every Node cell (largest +0.5%). Source: `bench/results/2026-10-22-40af20ce-*`.

## First counted native run-time lane (`5941d656`, 2026-10-22; advisory, not blessed)

Plan `m5-native.md` row M. `bench run --suite native --lane matched --instructions` at
`5941d656cfb64baf3e5d5c0fc93efec318a3b503` (the commit that adds the lane) in a clean
`../sakura-counted`, host `d88970189598`, tree clean, every oracle and checksum green on all 369
cells (87 Sakura native, 282 ports), Rust 1.99.1, Go 1.27.2, OCaml 5.5.1, Koka 3.2.9; host load
19.04, informational for counts (§17.264). **Not blessed**: the first native run-time baseline is
beta.5's, at its preparation commit (design §17.308 (2): "the first native number is beta.5's
baseline"; release.md §2), so every reading below is direction for the designer and no website
number. Sakura cells spread a median 0.26%, five over 1% (largest `sieve` n=200,000, 2.19%); 51 port
cells over 1%, in every port language. Sources: `bench/results/2026-10-22-5941d656-native.{json,md}`
(the `.md` regenerated from the JSON without the empty `ratio native/native` column) and
`-envelope-m5.{json,md}`.

**§17.109 M5 envelope, on instructions per call** (`bench envelope` on the one file; ratio
Sakura native / port, range over the four sizes): `miss`, 124 of 132 gating cells; met 8, all
`map_set` against Koka. Each miss is carried under §17.152 (2) with the cause this run names; a
cause marked *unattributed* is one the counters do not explain.

| Program | Against (ceiling): ratio | Carry: named cause |
|---|---|---|
| `fib` | Rust (1.5) 2.86–2.87; Go (1.0) 2.13–2.14 | No allocation per call (`requested` flat at 6,168 bytes, 4 frames, 0 boxes): code per direct call; *unattributed* between the call protocol and the checked add. |
| `countdown` | Rust (1.5) 4.26–4.92; Go (1.0) 2.31–2.61 | No allocation (4 frames, 5 drops); per-iteration code of the self-tail and mutual loops; *unattributed*. `tail_clique` (not gating): Rust 1.75–1.76, Go 1.13. |
| `sieve` | Rust (1.5) 74.6–103.4; Go (1.0) 46.6–82.1 | §17.152 carry: the `List` trie. Reuse is 1.0000 (2,191,222 of 2,191,223 at 200,000), so the gap is the representation, not misses; §17.177 (1) at its window. |
| `nbody` | Rust (1.5) 63.8–64.1; Go (1.0) 33.3–33.4 | §17.152 / §17.160 (1) carry: 0 boxes, 1,416 bytes requested and 66 reuse attempts per step at 71.2% hits; carried with the scalar-leaf and record questions (below). |
| `spectral_norm` | Rust (1.5) 20.4–20.6; Go (1.0) 18.3–18.4 | §17.152 carry: indexed `List` reads (one borrow per read, 6.4 M at 400); §17.177 (1)–(2). |
| `binary_trees` | OCaml (1.2) 11.5–13.2; Koka (1.2) 7.75–7.94; no-reuse 7.77–7.94 | Allocation and RC traffic (103 MB requested and 6.4 M dups at 14), 0 reuse attempts: nothing is taken apart and rebuilt; region share unmeasured (below). |
| `rbtree` | OCaml (1.2) 3.55–18.7; Koka (1.2) 7.31–7.44; no-reuse 7.33–7.40 | Hit rate 0.9992–1.0000 but about 1.6 attempts per insert against a rebuilt path of about log₂ n nodes; reuse coverage, not misses (paired reading below). |
| `pipeline` | OCaml (1.2) 2.55–15.1; Koka (1.2) 17.3–18.5; no-reuse 11.2–15.2 | Reuse 1.0000; 2.5 boxes and 3.5 dups per element: closure calls on boxed elements in the unfused lane; §17.177 (2). |
| `shapes` | OCaml (1.2) 1.80–13.6; Koka (1.2) 5.66–5.73; no-reuse 5.69–5.77 | 10 boxes per element and reuse 0.76–0.84 (record and `Option` construction); §17.177 record question and the niche default. |
| `map_set` | OCaml (1.2) 2.28–2.41; Koka **met** 0.61–0.86 (both lanes) | Against OCaml: HAMT nodes, reuse 0.89–0.93; carried with §17.201 (2)'s map question. |
| `handled_d1`, `_d4`, `_d16` | Koka (1.0) 4.20–4.58 | No allocation (frames 4, 0 boxes): per-perform dispatch; *unattributed*. |
| `handled_suspend` | Koka (1.0) 2.93–2.94 | No allocation (7 frames); *unattributed*. |
| `handled_fwd` | Koka (1.0) 13.2–13.4 | Two frames and about 104 bytes per forwarded operation: the frame protocol on the forwarding clause (§17.230 (2)'s cliffs). |
| `handled_reentry` | Koka (1.0) 28.0–28.2 | Two frames, two boxes and 171 bytes per re-entry through the callback; the same frame cliff. |
| `tokenizer` | Rust (1.5) 21.6–92.8 | **Superlinear on native**: 69.4 M, 6.03 G, 149 G and 595 G instructions per call at the four sizes, while WASM is linear (beta.4 baseline: 19.0 M → 1.91 G on Wasmtime); separate counts of the same build outside the run give 3.4×, 3.8× and 3.9× per doubling from 2,000 to 16,000, and `String.split` alone 3.8× and 3.9×; on non-WASM targets `runtime/rt`'s `text()` validates the whole string's UTF-8 on every call (`builtins.rs`, `from_utf8`; WASM uses `from_utf8_unchecked`), and `split` calls it once per piece. A runtime fix, not this lane's; the Rust port's own count also grows past linear (3.22 M → 6.41 G), so the ratio understates the native cost. |
| `json_validate` | Rust (1.5) 7.13–81.7 | Superlinear too: instructions 7.95× from 10,000 to 30,000 while `requested` grows 3.1×; consistent with the same per-call validation over long strings, *not attributed* by this run. |

**The §17.177 windows whose triggers have fired** (read at this run as direction; each keeps its
default until beta.5's baseline is blessed and its entry written, §17.308 (2l)):

- *`nbody`'s carry (§17.160 (1), the first window after N2):* 64.1× Rust, 33.4× Go at 100,000,
  flat across sizes, with 0 boxes natively. Default holds: carried with the scalar-leaf question;
  boxed words stay (ABI §4).
- *Reuse hit rates (first window after N3):* `sieve` 1.0000, `pipeline` 1.0000, `spectral_norm`
  0.980–0.998, `map_set` 0.891–0.926, `tokenizer` 0.875, `shapes` 0.762–0.839, `nbody` 0.712,
  `deltablue` 0.344–0.345, `binary_trees` no attempts. Recorded in every native cell's `stats`.
- *Record reuse (§17.138):* `reuses` 4,700,006 of 6,600,008 on `nbody` (100,000), 2,599,524 of
  3,099,525 on `shapes` (100,000), 0 of 0 on `binary_trees`. Default holds: constructor-site reuse
  only; any layout change is a new profile.
- *The paired `rbtree` protocol* (the program, and its twin with no version retained): hit rate
  0.9992 / 0.9999 / 1.0000 / 1.0000 retained against 1.0000 at every size unretained; attempts
  1,204 / 15,828 / 64,542 / 159,892 against 1,194 / 15,728 / 64,042 / 158,892; `requested` equal
  within 0.02% (83,941,588 against 83,928,080 bytes at 100,000), peak 52.2 MB against 4.8 MB.
  Retention costs memory, not reuse: the gap is attempts per insert. On this reading
  §17.310 (2d)'s condition for size-bounded inlining (`rbtree` outside its ceiling with N3c in)
  would hold; it is read on beta.5's blessed baseline, not here.
- *Regions (§17.174), the share of `requested` bytes dying inside the allocating twin:* no
  counter for it exists (§17.174 (2) names one behind a debug feature, not built). Default holds:
  no region work.
- *`Option` niche (§17.199 (1)):* not due. No prototype is built, and row M forbids measuring one
  before its baseline exists. Default holds: tagged stays.
- *`List` representation and scalar leaves (§17.177 (1)–(2)):* the second window, not reached.

Footprint, recorded and carried under §17.177 (1)–(2) (no target until a baseline): peak RSS of
the native executable 7.6–59.8 MB across cells (`rbtree` 100,000 the largest).

## §17.177 representation windows: prototypes and ablations (2026-10-22; advisory, not blessed)

Plan `m5-native.md` §3. Measured on the reference host `d88970189598` at `d954a806` with
`f6be3c1d`'s `text()` fix in the tree, release CLI, native lane unless named: instructions
retired per call as `exe n` minus `exe` with no argument, median of 5 processes, with the run's
`SAKURA_NATIVE_STATS` counters and peak RSS (`/usr/bin/time -l`). No harness change: these are
side builds (a second driver staticlib per prototype), so every number is direction for the
designer, not a baseline, and nothing here is blessed. "Copied bytes" is read as `requested`
(no separate counter exists). The trie numbers agree with row M's counted run (`sieve` 371.6 M
here against 370.0 M at 200,000; `spectral_norm` 2,296.5 M against 2,292.2 M at 400).

**(2) Scalar leaves.** `nbody` holds a `List<Body>` of records and has 0 boxes at every size:
unboxed scalar leaves change nothing there. `spectral_norm`'s ablation (the same loops over a
`List<Bool>`, an immediate word with no box and no count, read back as a `Float` by one branch;
its checksum differs, its work does not) bounds what unboxed leaves save on reads and pushes:

| `spectral_norm` | n | baseline | immediate leaves | Δ | boxes | `requested` |
|---|---:|---:|---:|---:|---:|---:|
| native | 100 | 146.2 M | 129.0 M | −11.8% | 4,100 → 0 | 120,814 → 55,214 |
| native | 400 | 2,296.5 M | 2,056.5 M | −10.5% | 16,400 → 0 | 472,430 → 210,030 |
| WASM (Wasmtime, warm) | 400 | 4,464.4 M | 5,070.4 M | +13.6% | | |

`drops` stay 6,401,647 at 400 (one per read either way). On WASM the branch costs more than the
box load it replaces, so no saving shows. JS lists hold JS numbers: not applicable. `shapes` and
`map_set` hold no `List<Int>` or `List<Float>` (a `List<Item>`, `Option<Int>`, maps and
`Map.entries` tuples), so the adapter cost the plan reads there is the per-element kind test
every generic `List` builtin would gain; it is unmeasured because no prototype was built: the
read ceiling is at most 10.5% on one of the two deciding programs and 0 on the other, and
`spectral_norm` stays above 18× Rust either way. **Default holds: boxed words stay (ABI §4).**

**(1) `List`: trie / flat owned sequence / hybrid.** Prototype: the tail never spills into the
trie, so a list is one flat node with copy-on-shared-write (in place when unique, doubling
capacity on push, a full copy on a shared write); `runtime/rt/src/list.rs` only, every backend's
`List` builtins unchanged, never committed. A sampled profile of `sieve` at 200,000 puts about
59% of the main thread's samples in the trie's paths (`set_unique` 29%, `list_set_in`, `at`,
`push_fields`, `push_in`, `dup_except`, `node`, `push_leaf`).

| Program | n | trie | flat | Δ | peak RSS (KB) | `requested` |
|---|---:|---:|---:|---:|---:|---:|
| `sieve` | 200,000 | 371.6 M | 240.2 M | −35.4% | 8,544 → 9,440 | 3,403,268 → 2,113,932 |
| `spectral_norm` | 400 | 2,301.2 M | 2,210.5 M | −3.9% | 7,472 → 7,488 | 472,430 → 434,710 |
| `nbody` | 100,000 | 2,630.7 M | 2,630.1 M | 0.0% | 7,520 → 7,504 | equal |
| `pipeline` | 300,000 | 919.9 M | 748.0 M | −18.7% | 14,672 → 16,096 | 19,663,700 → 18,297,568 |
| `shapes` | 100,000 | 1,950.1 M | 1,789.2 M | −8.3% | 20,032 → 20,048 | 70,393,572 → 69,778,924 |
| `map_set` | 100,000 | 4,419.5 M | 4,206.1 M | −4.8% | 41,424 → 41,872 | 264,803,363 → 262,132,543 |
| `rbtree` (retained) | 100,000 | 2,550.2 M | 2,540.8 M | −0.4% | 58,400 → 58,384 | equal within 0.01% |
| `binary_trees` | 14 | 2,376.4 M | 2,378.1 M | +0.1% | 9,312 → 9,344 | equal |
| **`deltablue`** | 5,000 | 9,239.7 M | 314,153.6 M | **34×** | 10,688 → 10,912 | |
| **`deltablue`** | 10,000 | 19,198.5 M | 1,246,509.8 M | **64.9×** | 13,904 → 13,728 | |

The suite's `List` port of `deltablue` writes shared lists, and the flat representation turns
each such write into a whole copy. A retained-versions micro (n `List.set`s on a list of n
`Int`s, every k-th version kept, as `rbtree` keeps trees; `bench/target`, not committed) shows
the same cliff: k = 1 at 10,000: 107.8 M → 6,381.7 M (59×), peak RSS 12 MB → 408 MB, and at
40,000 the flat run exhausts the heap (trie: 520.0 M, 33 MB); k = 10 at 40,000: 312.9 M →
10,122.4 M (32×), 24 MB → 649 MB; k = 100 at 40,000: 225.3 M → 1,072.2 M (4.8×). A flat
representation wins only if the persistent workloads stay within their ceilings (§17.177 (1));
it puts `deltablue` 65× further out. The hybrid that would keep `sieve`'s gain switches
representation on an ownership fact (flat until the first shared write), which §17.217 rules
out. **Default holds: the trie stays**; `sieve`, `nbody` and `spectral_norm` are carried again
under §17.152 (2) with the causes row M named. The 35% on `sieve` is the trie's unique-path
write (`set_unique`), a lead for an optimisation of the trie, not a representation.

**(5) `Option` niche.** Not built; three `shapes` variants at 100,000 bracket it (outputs equal
to the program's):

| `shapes` variant | instructions | `requested` | boxes |
|---|---:|---:|---:|
| the program | 1,950.1 M | 70,393,572 | 1,018,091 |
| `Option.unwrap_or(it.tag, 0)` as a `match` (no boxed default) | 1,793.5 M | 62,393,524 | 518,091 |
| `tag: T` at `Item<Int>`: a boxed word with no `Some` (the niche's `Some(x) = x`), `None` a boxed −1 | 1,639.7 M | 51,269,868 | 600,000 |
| `tag: Int` with a −1 sentinel (no `Option`, no box) | 1,241.7 M | 41,669,868 | 0 |

The `Some` wrappers are 518,091 objects of 24 bytes, 12.4 MB, **17.7% of `shapes`' `requested`**
(§17.199 (1)'s bound was 11.8% of a larger pre-N3 total). Without them, 153.8 M fewer
instructions (8.6% of the `match` variant, 7.9% of the program), though that variant still
allocates 81,909 `None` boxes the niche would not; the niche's tag test on every projection and
construction is not charged. The last row is beyond any niche (it also unboxes the `Int`).
`map_set`: §17.199 (1)'s 3.6% of bytes, not re-measured. **Default holds: tagged stays**; the
build (universal ABI §4.3 form, schema table, §17.99 parity gate, an ABI MINOR) is put to the
designer with these numbers (REQUESTS 2026-10-22, the §17.177 windows).

**(§17.174) Regions.** The counter of §17.174 (2), built (`a88a2bd8`, `sakura_rt` feature
`regions`, `SAKURA_NATIVE_REGIONS=1`): bytes and objects of `new_object` released before the twin
activation that allocated them returns. Reuse share is `reuses / (reuses + objects allocated)`.

| Program | n | region share (bytes) | (objects) | reuse share (objects) | trigger |
|---|---:|---:|---:|---:|---|
| `binary_trees` | 14 | 0.0000 | 0.0000 | 0.0000 | not met |
| `shapes` | 100,000 | 0.2745 | 0.3157 | 0.4853 | not met |
| `pipeline` | 300,000 | 0.2832 | 0.0967 | 0.6973 | not met |
| `rbtree` | 100,000 | 0.0002 | 0.0001 | 0.0838 | not met |
| `nbody` (not a kernel) | 100,000 | 0.7458 | 0.7619 | 0.6912 | |
| `map_set` (not a kernel) | 100,000 | 0.9738 | 0.9502 | 0.3899 | |
| `sieve` (not a kernel) | 200,000 | 0.9997 | 0.9993 | 0.9807 | |
| `spectral_norm` (not a kernel) | 400 | 0.2834 | 0.1272 | 0.4620 | |
| `deltablue` (not a kernel) | 10,000 | 0.0124 | 0.0231 | 0.1688 | |

On none of the four allocation kernels does the share exceed what reuse recovers, in bytes or in
objects. **Default holds: no region work.** Where the share is high, the allocating twin is a
whole benchmark loop (`run` of `sieve`, `map_set`), so "dies inside" there means "before the
program ends", the hold-everything region research/12 warns against. Reproduce: build the driver
with `--features native,regions`, set `SAKURA_NATIVE_LIB_DIR` to it and `SAKURA_NATIVE_REGIONS=1`
for `sakura build --target native`, then read the `region_*` fields of `SAKURA_NATIVE_STATS`.

**`text()` (row M's superlinear cells), fixed in `f6be3c1d`.** Native instructions per call,
before → after: `tokenizer` 69.5 M → 10.7 M (1,000), 6,038.8 M → 100.6 M (10,000),
148,895.2 M → 504.4 M (50,000), 1,012.9 M at 100,000 (row M: 595 G); `json_validate` 6.9 M →
6.4 M (100), 81.3 M → 60.5 M (1,000), 2,775.5 M → 603.6 M (10,000), 21,956.7 M → 1,818.5 M
(30,000). Both now grow linearly (`tokenizer` 9.4× for 10×, `json_validate` 3.0× for 3×).

## `v1.0.0-beta.5` baseline (`447dce0c`, 2026-10-22)

Counted (§17.264) at the preparation commit `447dce0c1e07e6ce1e3e08d95f1981311ebe8281` in
`../sakura-counted`, tree clean, every oracle and checksum green, Node 26.12.0 under §17.282's
flags. Blessed by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design
§17.310 (1)); the owner can overturn it. `baselines/d88970189598.json`, `wasm/`, `typescript/` and
`compile-native/` hold the `447dce0c` runs (the `40af20ce` files are in
`baselines/archive/40af20ce/`), and **`baselines/native/d88970189598.json` is the first native
run-time baseline** (row M, design §17.308 (2), release.md §2): Sakura's `--target native`
executable counted beside the Rust, Go, OCaml and Koka ports on the matched lane. The counted
`compile-native` report grows from 16,384 to 32,768 rows by 1.96× in `frontend`, 2.01× in
`lowering`, 2.01× in `codegen`, 2.14× in `object` and 1.04× in `link`, so it names `lowering`,
`codegen` and `object` superlinear (`object` joins the two named at beta.4). Host load was high
(one-minute load up to 26, other agents' builds; informational for counts): Node spread over 1%
on 10 Sakura and 7 hand-JS cells of 99 each in the `js` run (largest `shapes` n=100,000, 2.92%)
and 10 Sakura cells in the `wasm` run (largest `rbtree` n=100,000, 3.58%), against none at beta.4;
one Wasmtime cell (`countdown` n=1000, 41.85%, as at beta.4 its first process); Sakura native cells
a median 0.03%, none over 1%; 17 Go cells over 1% (largest `tokenizer` n=100,000, 26.31%); Bun and
`tsc` as before (advisory). Against the `40af20ce` baseline the `js` run's Sakura/hand-JS ratios
read `ok` on all 99 Node cells (`compare` flags only advisory Bun rows;
`2026-10-22-447dce0c-compare-js-beta4.txt`). M4 envelope (wall-time lane, advisory, host load
26.29): `miss` on 238 of 264 gating cells (absent 4, met 22), against 237 at beta.4; carried under
§17.265. M5 envelope on instructions (`envelope-m5`): `miss` on 124 of 132 gating cells, met 8, as
at the advisory `5941d656` run; `rbtree`'s counts are within 0.1% of that run's. Prep `fib` and
`shapes` compare `ok` on every Node cell (largest move −1.2%). Source:
`bench/results/2026-10-22-447dce0c-*`.

## Every program in C, Rust, Go, OCaml and Koka (`49c49e2a`, 2026-10-22; counted, not blessed)

The owner's word of 2026-10-22: native benchmarks are the main ones. `7bd0291a` adds a C port of
all 22 programs and OCaml (17) and Koka (11) ports of the programs that had none, so Sakura's
`--target native` executable is now counted beside five languages on every program.
`bench run --suite native --lane matched --instructions` at
`49c49e2aec9494d867c0ba599b4e5f8ef171a4c3` (`main`) in a clean `../sakura-native-counted`, host
`d88970189598`, tree clean, every oracle and checksum green on all 609 cells; Apple clang 21.0.0,
Rust 1.99.1, Go 1.27.2, OCaml 5.5.1, Koka 3.2.9; host load up to 68.7 (other agents),
informational for counts (§17.264). Sources: `results/2026-10-22-49c49e2a-native-ports.{json,md}`
and `-envelope-m5.{json,md}`.

**Not blessed.** The first native baseline is beta.5's (`447dce0c`, above) and it already
includes `--suite native`; a run between releases is evidence (docs/release.md §2). The new ports
join the blessed baseline at the next release's preparation commit, whose counted native run
builds every port. Until then the website shows the blessed Sakura and Rust/Go/OCaml/Koka cells
and takes only the cells the baseline lacks (C, and the new OCaml and Koka ports) from this run,
marked as such. That is sound because nothing moved between the two: every one of the 87 Sakura
cells reads within ±0.23% of the blessed count (no §17.177 window has landed in between), and the
282 port cells both files have within −2.7% to +4.2% (Go's concurrent collector; the rest within
±1.3%). Spread: a median 0.04–0.09% per language except Go (0.28%, 29 of 87 cells over 1%); no C
cell over 1%.

**Sakura native ÷ each language**, instructions per call at the largest size, the geometric mean
over the fifteen families (`handled` once), closest and furthest program of all 22:

| Against | Typical | Closest | Furthest |
|---|---:|---|---|
| C | 81.6× | `binary_trees` 1.93× | `tokenizer` 29,614× |
| Rust (overflow checks) | 27.4× | `tail_clique` 1.76× | `handled_reentry` 348× |
| Go | 15.5× | `tail_clique` 1.13× | `handled_reentry` 210× |
| OCaml | 9.7× | `handled_d16` **0.52×** (also `d4` 1.62×) | `tokenizer` 3,352× |
| Koka | 5.0× | `map_set` **0.61×**, `tail_clique` **0.73×** | `tokenizer` 93.7× |

Read with the ports' own shapes, each named in its header comment: clang folds C `countdown`'s
sum loop to a closed form (2,109×) and the contracts loops nearly so (574×, 364×); the C, OCaml
and Koka `tokenizer` ports append to one buffer while the Rust and Go ports copy the string per
word, so the native runtime's per-call UTF-8 check (row M's carry above) shows against them at
full size; OCaml 5's effect handlers allocate a continuation per perform, so native Sakura does
less work than OCaml on `handled_d16`.

**M5 envelope** (`bench envelope` on the one file): reads cleanly with the new variants; `miss`
on 124 of 132 gating cells, met 8 (`map_set` against Koka), the beta.5 reading unchanged. C has no
§17.109 ceiling and the new OCaml and Koka ports of numeric and text programs add no gating row,
so the envelope's rows are those of beta.5. Every miss stays carried under §17.152 (2) with the
cause row M names above; this run adds no new cause.

Both this run and the beta.5 baseline predate `f6be3c1d` (`text()` checks UTF-8 once per builtin
call), so their native `tokenizer` and `json_validate` cells, and every ratio against them, are the
superlinear ones; the website says so, and the next preparation commit's counted run reads the fix.

**Wall time on a hosted runner** (bench.yml `mode=native`, run 39810593780, `8ad07545`: `49c49e2a`
plus the harness's `--color never` fix; GitHub `macos-latest`, Apple M1 (Virtual), `--stable`, five
fresh processes per cell; all 609 cells and every oracle green; the runner's rustc was 1.99.0, not
the pinned 1.99.1). Sakura native time ÷ each language, geometric mean over the families: C 82.0×,
Rust 19.8×, Go 11.6×, OCaml 7.4×, Koka 3.8×. Faster in time than: Rust, Go, OCaml and Koka on
`tail_clique` (9.9 ms against 15.4, 15.9, 11.6, 18.4), OCaml on `handled_d1`, `d4` and `d16`, Koka
on `map_set` and, within noise, `nbody`. A shared virtual machine: process medians spread a median
20% per cell (90th percentile 52%), so a time ratio is direction; the counts are the comparison.
`results/2026-10-22-8ad07545-hosted-39810593780-native.json`; the website's time table reads it.


## `v1.0.0-beta.6` baseline (`70c64513`, 2026-10-23)

Counted (§17.264) at the preparation commit `70c64513666c3f2d818e722bf9b2b005f3f8b536` in
`../sakura-counted`, tree clean, every oracle and checksum green, Node 26.12.0 under §17.282's
flags. Blessed by Claude Opus 5.5 under the owner's written delegation of 2026-10-22 (design
§17.310 (1)); the owner can overturn it. `baselines/d88970189598.json`, `wasm/`, `typescript/`,
`compile-native/` and `native/` hold the `70c64513` runs (the `447dce0c` files are in
`baselines/archive/447dce0c/`). The files now include the community suites (8 Benchmarks Game and
13 Are We Fast Yet programs) and, in `native/`, the C port and the new OCaml and Koka ports: 732
`js` cells, 545 `wasm` (one absent: `json_validate` on Wasmtime, whose `json_decode` exists only
in the Node host), 1,197 `native`; the five runs took about five hours rather than one. The counted
`compile-native` report grows from 16,384 to 32,768 rows by 1.96× in `frontend`, 2.01× in
`lowering`, 2.01× in `codegen`, 2.07× in `object` and 1.04× in `link`, naming `lowering`,
`codegen` and `object` superlinear, as at beta.5. Host load was low (about 1–3): Node spread over
1% on 2 Sakura and 3 hand-JS cells of 183 each in the `js` run (largest `awfy_havlak` hand JS
n=1,500, 2.01%; Sakura `shapes` n=100,000, 1.28%) and 1 of each in the `wasm` run (Sakura `rbtree`
n=50,000, 1.35%); no Wasmtime cell; every native language a median 0.03–0.06%, Sakura native none
over 1%, Rust one (`bg_binary_trees` n=10, 1.38%), Go 28 (largest `tokenizer` n=100,000, 12.39%);
Bun as before (advisory). Against the `447dce0c` baseline the `js` run's Sakura/hand-JS ratios read
`ok` on all 99 shared Node cells (−2.0% to +0.4%; `compare` flags only advisory Bun rows;
`2026-10-22-70c64513-compare-js-beta5.txt`), and the `wasm` run reads `ok`. Sakura's native counts
against beta.5's: `f6be3c1d` (`text()` checks UTF-8 once per builtin call) cuts `tokenizer` by
584× at n=100,000 (6.9×, 60×, 293× at 1,000, 10,000, 50,000) and `json_validate` by 12× at n=30,000;
the other 79 shared cells move −0.01% to +1.3% (`contracts`, every size). M4 envelope
(wall-time lane, advisory, host load 3.24): `miss` on 237 of 264 gating cells (absent 4, met 23),
against 238 at beta.5; carried under §17.265 (1). M5 envelope on instructions (`envelope-m5`):
`miss` on 121 of 132 gating cells, met 11, against 124 and 8 at beta.5: `tokenizer` against Rust
at n=10,000, 50,000 and 100,000 now meets its ceiling beside `map_set` against Koka; every miss
stays carried under §17.152 (2). `compile`: `check_hello` (p95 15.1 ms, target 10) and
`check_cold_10k` (62.1 ms, target 45) miss §17.195 (1)'s targets as at beta.5; every budget holds.
Prep `fib` and `shapes` compare `ok` on every Node cell (largest move −1.5%, `shapes`
n=100,000). Source: `bench/results/2026-10-22-70c64513-*`.

## `v1.0.0` baseline (carried from `70c64513`, 2026-10-23)

`v1.0.0`'s blessed baselines are `v1.0.0-beta.6`'s counted run at `70c64513` (above; `5ab07b34`,
`8114c6d6`), carried under the owner's decision of 2026-10-23 and the written delegation of
2026-10-22 (design §17.310 (1)). The shipped compiler, std, runtimes and packaging at 1.0.0's
preparation commit `36e64df463e9f59a8174cd49ea54d7dbb9ecf674` equal beta.6's apart from the
version string (road-to-1.0 §7, design §17.312: the frozen set is unchanged but for the test-only
ABI 1.19.0 fixture and `tests/compat/allow.txt`), so there is no new five-hour counted run.
`bench/baselines/` is unchanged and its files keep `meta.commit` `70c64513`. The M4 envelope read
(`miss` on 237 of 264 gating cells) and the M5 one (`miss` on 121 of 132) are beta.6's, carried
under §17.265 (1) and §17.152 (2). Prep `fib` and `shapes` (docs/release.md §2), counted at
`36e64df4` in `../sakura-counted` (tree clean, Node 26.12.0 under §17.282's flags, started
2026-10-23T06:00Z), compare `ok` on every Node cell against the `70c64513` baseline: `fib` −0.0% to
+0.0%, `shapes` +0.0% to +0.8% (largest n=50,000). Blessed (carried) by Claude Opus 5.5 under the
owner's written delegation of 2026-10-22; the owner can overturn it. Source:
`bench/results/2026-10-23-36e64df4-*`.

## WASM inline object paths (`2d3f77c1`, 2026-10-23; quick, advisory, not blessed)

Five commits after `v1.0.0` (`9546140b`..`2d3f77c1`, ABI erratum 1.19.1) move the runtime calls
that dominated heap-heavy WASM programs into the emitted code: scalar memory access without a
bounds re-check, `Dup`/`Lend`/`TupleGet`/constructor reset, small-object allocation from the
free lists, the last `Drop` of a small row, and list element and `Option` payload reads.
Attribution before the work (per-function op counts on an instrumented module, `awfy_bounce`,
`awfy_richards`, `deltablue`, `binary_trees`): the wasm32 `Mem` bounds check was about 31% of
executed operators, and the rest was mostly generic runtime `release`/`row` (~130 operators
each), `Heap::alloc`/`free` (120/89) and one runtime call per `Dup`, `Lend`, `TupleGet`, reset
and element read. Scalar programs (`fib`) were already near 2× hand JS and do not move.

`bench run --suite wasm --sizes quick --instructions --engine node` on the reference host
`d88970189598` at `2d3f77c1` (the tree dirty only in `compiler/runtime/rt/ABI.md`; the host was
loaded, which the instruction counts tolerate), compared with the blessed
`bench/baselines/wasm/d88970189598.json`: verdict `ok`, 81 of 92 Node cells improved by more than
10% and none worse; geometric mean of the Sakura/JS instruction ratio 21.08 → 13.11 (−37.8%).
Selected cells (largest quick n): `awfy_bounce` 86.2 → 32.9, `awfy_richards` 111.4 → 55.8,
`deltablue` 138.6 → 79.9, `awfy_list` 53.1 → 26.2, `nbody` 38.2 → 18.9, `awfy_json` 20.2 → 9.9,
`shapes` 11.1 → 6.3, `binary_trees` 7.1 → 4.5, `tokenizer` 3.3 → 2.2, `fib` 2.00 → 1.99. Every
program's output and §10 counters equal the baseline compiler's, direct and `--no-direct`, except
`requested`, which grows by the emitter's static tables (120–272 bytes). This is advisory: the
blessed baseline moves only with a counted run. Source:
`bench/results/2026-10-23-2d3f77c1-wasm-quick*`.

## JS ownership and code shape (`6e9248d8`, 2026-10-23; quick, advisory, not blessed)

Seven commits after `v1.0.0` (`61b3d338`..`6e9248d8`) remove the causes found by profiling the
JS programs furthest behind hand JS. Attribution before the work (CPU profiles of `awfy_sieve`,
`bg_fannkuch_redux`, `bg_nbody`, `awfy_richards`, `awfy_havlak`, `awfy_bounce`, `awfy_permute`,
`awfy_towers`): the persistent `List.set` path copy where ownership was lost was 59% of
`awfy_sieve`, 50% of `bg_fannkuch_redux`, 32% of `bg_nbody` and `awfy_richards` and 22% of
`awfy_havlak`; the rest was record allocation on state-threading paths and labelled-block joins
that V8 does not inline and JSC runs slowly. The commits: a def returning no collection sinks the
list it updates (`sieve`); a pure scalar-returning reader keeps a list's elements owned
(`nbody`); reads after a consuming call are bound before it (`richards`); a list literal adopts
its array and `[]` is the empty singleton; a fresh record, and an owned record (an owned entry's
consumed parameter), is rebuilt in place; a join of two pure arms is one conditional expression.

`bench run --suite js --sizes quick --instructions` (Node and Bun) on the reference host
`d88970189598` at `6e9248d8` in a clean worktree (the host was loaded, which the instruction
counts tolerate), compared with the blessed `bench/baselines/d88970189598.json` (`70c64513`):
verdict `ok`, none worse beyond the flag threshold; improved by more than 10% in 13 of 92 Node
cells and 17 of 92 Bun cells. Geometric mean of the Sakura/JS instruction ratio over every quick
cell: Node 2.72 → 2.46 (−9.5%), Bun 4.01 → 3.49 (−12.9%); the median at each program's largest
quick n moves less (Node 3.12 → 3.00, Bun 3.69 → 3.61), and the programs at or below hand JS stay
7 of 46 on Node and 2 of 46 on Bun. Selected cells (largest quick n, Node / Bun): `awfy_sieve`
23.1 → 2.97 / 67.9 → 3.66, `bg_nbody` 11.3 → 5.13 / 8.96 → 3.73, `awfy_richards` 13.6 → 7.32 /
16.4 → 9.70, `awfy_storage` 8.36 → 6.47 / 13.7 → 7.05, `awfy_permute` 4.07 → 3.23 / 12.1 →
8.61, `awfy_towers` 3.48 → 3.01 / 8.92 → 7.39, `deltablue` 11.1 → 10.1 / 8.73 → 7.84,
`awfy_havlak` 7.20 → 6.81 / 7.84 → 7.10, `sieve` 3.05 → 2.98 / 3.38 → 2.71. One cell moved the
other way within the threshold: `rbtree` on Node at n=10,000, 3.97 → 4.14 (+4.1%; n=1,000 and
both Bun cells are flat); its JS differs only in `[]` and the runtime file, and the cause is not
found. Unchanged: `bg_fannkuch_redux` (22.6 / 17.7), whose `var perm = perm1` alias still makes
every `List.set` copy, as `map_set`'s retained `snapshot` does. This is advisory: the blessed
baseline moves only with a counted run. Source: `bench/results/2026-10-23-6e9248d8-js-quick*`.
