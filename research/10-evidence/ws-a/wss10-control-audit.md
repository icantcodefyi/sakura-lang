# WS-S10 hand-JS arithmetic-check audit

2026-10-18; design §17.150, performance-r1 §9 WS-S10. Static review by
GPT-6.1-Sol of all fifteen families (the fourteen other than countdown are
listed below), including all six handled entries and contracts_plain: 21
program directories in total. Source identities are in
[wss10-control-source-blobs.json](wss10-control-source-blobs.json).
The human contributor owns review. A subsequent rebase brought in WS-B10's
map_set key-kind entries; [follow-up identities](wss10-rebase-audit.json)
pin those sources. The added run_int/run_string/run_tuple controls retain
the same unchecked arithmetic listed below.

## Countdown fix

The control now runs parity before the sum, as Sakura's `run` does. Both
loops test `i !== 0`, preserving the source's zero base case. Each parity
`i - 1`, each sum-tail `i - 1` and `acc + i`, and the final sum plus parity
are immediately followed by `Number.isSafeInteger`; failure throws
`RangeError("Int overflow")`. The sum loop checks the next index before
adding the old index, matching left-to-right recursive argument evaluation.
No closed-form sum or recursion-depth shortcut is introduced. The control's
exception is an explicit overflow signal, not Sakura's diagnostic protocol.

[wss10-countdown-checks.log](wss10-countdown-checks.log) records a focused
Node check: the ten checks for n=3 appear in source operation order;
MIN_SAFE_INTEGER fails on the first decrement; n=134217728 overflows the
sum; n=134217727 still returns 9007199187632128. Ordinary oracle values
remain unchanged, including n=1000000.

## Other fourteen families

Every path below is relative to `bench/programs/`; inspect both
`<program>/sakura/main.sakura` and `<program>/js/main.mjs`. These are
source-level obligations. Compiler-proved redundant guards may disappear;
this audit does not assert that every source operator remains checked in
emitted code. Float operations do not require Int overflow guards. A fixed
nonzero divisor cannot fail a zero check. The bounded oracle corpus does
not replace §17.109's requirement to detect overflow at the same points.

| Family / entries | Check placement found in hand JS | Finding / follow-up |
|---|---|---|
| fib | `fib(n - 1) + fib(n - 2)` uses raw Number arithmetic. | Missing subtraction and recursive-result addition guards; the existing README warning remains applicable. |
| sieve | `p * p`, `p++`, `j += p`, initial `i++`, and final `count++` are unchecked. | Missing Int guards even though every oracle input fits. Indexed clearing order is already matched. |
| nbody | Body/energy computations use Float operators in both sources; index additions and step increments use raw Number; `round` has no safe-integer result check. | No Float boxing/overflow-check fix is warranted. Int index/step guards and the Float.round-to-Int result guard remain absent. The separate definition change only replaces the persistent update helper. |
| spectral_norm | `a(i,j)` uses unchecked `i+j`, multiplication, division by 2 and subsequent additions; all loop increments are raw; final rounding is unchecked. | Missing intermediate Int guards and rounded-result guard. Division by 2 is exact for the corpus's consecutive-integer product, but raw floating division is not a general replacement for checked truncating Int division. |
| binary_trees | Depth decrements, size/check additions, `maxDepth - d + minDepth`, `2 ** exponent`, accumulation and increments are unchecked. | Missing Int guards, including power overflow; matching tree counts do not establish check parity. |
| pipeline | `(x * 7) % 1000` and reduction `acc + z` are unchecked. | Missing multiply and fold-add guards; constant modulo divisors are nonzero. Fresh stages remain matched. |
| rbtree | Key `i * 7919 % (2 * n + 1)`, recursive size/black-height additions, `j * 3`, sums and increments are raw. | Missing Int guards and the computed modulo divisor check for general inputs. Retained immutable versions are present. |
| map_set | `2*n`, updates `value+7`, all found/total/snapshot/final additions and increments are raw; `/100` is truncated explicitly. | Missing overflow guards. Truncation is present and the literal divisor is nonzero; the retained Int map is copied. |
| shapes | MINSTD `seed * 48271`, shape areas, grow/tag additions, checksums and increments are raw. | Missing guards before modulo as well as at area/checksum arithmetic; a safe final seed does not prove the preceding product safe for general input. |
| tokenizer | Token/byte/scalar/comma counters, weighted checksum multiplies/adds and construction-loop increments are raw. | Missing Int guards. UTF-8/scalar counting is explicit; string correctness does not establish arithmetic-check parity. |
| json_validate | `decode` checks `Number.isSafeInteger(id/age)` and name/age refinements; counters, byte/id sums, weighted checksum and loop increments are raw. | Decode/refinement validation is present; arithmetic overflow checks are absent. Parse checks do not cover later sums. |
| deltablue | Planner marks, queue/test counters, `n+1`, index additions, scale multiply/add, backward subtract/divide and checksum sums are raw. AWFY result assertions are present. | Missing arithmetic guards; scale division also lacks the general Int truncation/zero behavior. Corpus divisibility and AWFY assertions do not supply those checks. |
| handled: handled_d1, handled_d4, handled_d16, handled_fwd, handled_reentry, handled_suspend | Counter `++value`, accumulation and ordinary-loop increments are raw; suspend additionally uses raw elapsed/final addition and `Math.trunc(n/2)`. | All six omit Int overflow guards. Literal division by 2 is truncated and nonzero. The permitted closure-call control still has no handler stack/resumption/cancellation. |
| contracts, contracts_plain | Enabled control checks pre/postconditions around `increment` and the collection precondition; `x+1`, reducer sums, outer accumulation and final addition remain raw in both. | Contract checks are present only in contracts; arithmetic overflow checks are missing in both. Ensures `result > x` is not a safe-integer guard. |

Only countdown is repaired in this workstream's control sources. Other fixes
need their own definition boundaries and fresh measurements; old ratios must
not be presented as fully checked, semantics-matched results. No compiler,
std, native/context port, checksum metadata, or baseline is changed here.

## Definition and measurement compatibility

[nbody-definition-change.json](nbody-definition-change.json) and
[countdown-definition-change.json](countdown-definition-change.json) pin the
old/new source blobs and unchanged metadata. Exclude all pre-change nbody
and countdown ratios from comparisons and noise summaries. Keep their raw
historical runs; the old candidate is not eligible for these two cells.
The existing local and CI noise subsets contain neither family, so their
unchanged-source noise statistics require no recalculation for WS-S10.
All WS-S10 timing samples are advisory under §17.153, not optimisation
claims or evidence that a ceiling is met. Full validation commands and
results are in [wss10-validation.md](wss10-validation.md).
