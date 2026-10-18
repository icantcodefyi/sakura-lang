# Road to 1.0 (provisional outline)

**Provisional.** Design §17.168. No stage carries a date; every stage boundary is the owner's.
STABILITY.md is the policy; this is the order.

| Stage | What | Evidence that it is done |
|---|---|---|
| 1 | `v0.1.0-alpha.1` (§17.157 channels; M3 closed) | the owner's dispatch succeeded; installer, npm and tap verified on a fresh host |
| 2 | M4 exit (§17.166) and `v0.1.0`, the scoped first non-alpha preview | the M4 exit gate signed; release notes name what is and is not stable |
| 3 | **Normative spec** in `docs/spec/`: grammar (§3), static semantics (types, rows, refinements, contracts), dynamic semantics (effects, handlers, defects, determinism), the diagnostic contract (§12.2), the ABI as an annex | every normative sentence pinned by a conformance fixture; grammar and diagnostics first because agents consume them; written from the implementation, after stage 2 |
| 4 | **Stability guarantees**: source compatibility within a major; diagnostic codes never reused; `--json` schemas additive within a major; ABI MAJOR rule (ABI §0); a deprecation path of at least one minor; an MSRV policy | stated in STABILITY.md with the first `0.x` that honours each |
| 5 | **Packages** (plan §6 question 1, milestone P): MVS resolution, lockfile hashing, capability ceilings, a registry decision | after M5 unless a consumer appears; its own research and plan |
| 6 | **Platforms** B then C (§17.157 (2)) | each after the previous has shipped and been accepted on a fresh host |
| 7 | **Evidence from real usage** before `1.0` | three programs outside this repository, maintained for one release cycle by someone other than the owner and the agents, each with a `sakura test` suite and a reported minor upgrade; the agent-DX suite at a stated pass rate under a stated budget on two independent model families; zero open soundness bugs in checker or verifier for one cycle; the benchmark envelope held at a quiet window for two consecutive releases |

1.0 is a claim about other people's programs; stage 7 is the only evidence for it.
