# Process-level summary

Generated from `research-rerun.json` by `python3 research/10-evidence/summarize.py`.

| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |
|---|---:|---:|---:|---:|---:|---:|
| fib | 6.011 | 9.68× | 8.48× | 3.920 | 9.80× | 9.90× |
| tail_sum | 0.296 | 2.21× | 0.47× | 0.048 | 1.54× | 0.38× |
| loop_sum | 0.485 | 3.79× | 1.00× | 0.063 | 2.40× | 1.01× |
| list_map_fold | 0.498 | 1.54× | 2.75× | 0.638 | 23.62× | 2.32× |
| records | 0.858 | 1.03× | 1.03× | 0.503 | 2.93× | 1.10× |
| strings | 0.054 | 1.42× | 0.98× | 0.062 | 1.17× | 1.00× |
| effects | 2.255 | 4.65× | 4.66× | 2.340 | 53.00× | 12.75× |
| contracts | 0.263 | 4.81× | 0.68× | 0.335 | 7.15× | 6.89× |
| contracts_plain | 0.065 | 1.87× | 2.55× | 0.199 | 13.38× | 7.84× |

The range below is min–max of five process medians, **not a confidence interval**.
RSS is median of five whole-process peaks, including warmup and common imported harness modules.

| Engine | Workload | Variant | Median ms | Process range ms | Peak RSS MiB |
|---|---|---|---:|---:|---:|
| node | fib | sakura | 6.011156 | 5.859161–12.664458 | 81.3 |
| node | fib | js | 0.620839 | 0.589632–0.663421 | 48.0 |
| node | fib | checked | 0.708842 | 0.658130–1.001435 | 48.8 |
| node | tail_sum | sakura | 0.295734 | 0.286191–0.882531 | 52.2 |
| node | tail_sum | js | 0.133812 | 0.126797–0.246565 | 49.1 |
| node | tail_sum | checked | 0.627578 | 0.621711–0.635781 | 50.8 |
| node | loop_sum | sakura | 0.484800 | 0.482282–0.496475 | 50.8 |
| node | loop_sum | js | 0.128061 | 0.127023–0.349709 | 49.2 |
| node | loop_sum | checked | 0.485057 | 0.483085–0.891116 | 49.4 |
| node | list_map_fold | sakura | 0.497775 | 0.479837–0.630604 | 118.5 |
| node | list_map_fold | js | 0.323852 | 0.298815–0.353365 | 63.3 |
| node | list_map_fold | checked | 0.180860 | 0.173211–0.763799 | 115.0 |
| node | records | sakura | 0.858161 | 0.815977–2.129237 | 53.1 |
| node | records | js | 0.835988 | 0.806859–0.853581 | 51.7 |
| node | records | checked | 0.832348 | 0.804415–1.491776 | 51.9 |
| node | strings | sakura | 0.053906 | 0.049790–0.123725 | 113.6 |
| node | strings | js | 0.038074 | 0.037034–0.040984 | 112.6 |
| node | strings | checked | 0.054893 | 0.052910–0.060676 | 113.2 |
| node | effects | sakura | 2.255104 | 2.236010–2.526130 | 83.4 |
| node | effects | js | 0.485134 | 0.483080–0.494799 | 49.6 |
| node | effects | checked | 0.484440 | 0.483059–0.679391 | 50.9 |
| node | contracts | sakura | 0.263449 | 0.250082–0.272516 | 50.8 |
| node | contracts | js | 0.054752 | 0.053458–0.058123 | 48.5 |
| node | contracts | checked | 0.387437 | 0.383081–0.395531 | 48.5 |
| node | contracts_plain | sakura | 0.065193 | 0.062934–0.067348 | 51.6 |
| node | contracts_plain | js | 0.034917 | 0.033915–0.051780 | 49.0 |
| node | contracts_plain | checked | 0.025611 | 0.025406–0.044904 | 49.2 |
| bun | fib | sakura | 3.919510 | 3.759570–4.136719 | 39.5 |
| bun | fib | js | 0.399821 | 0.392638–0.421487 | 21.1 |
| bun | fib | checked | 0.395893 | 0.376748–0.409959 | 23.0 |
| bun | tail_sum | sakura | 0.048041 | 0.045404–0.049448 | 35.9 |
| bun | tail_sum | js | 0.031209 | 0.029804–0.057919 | 22.0 |
| bun | tail_sum | checked | 0.127455 | 0.090489–0.130620 | 23.7 |
| bun | loop_sum | sakura | 0.063392 | 0.059072–0.155184 | 32.0 |
| bun | loop_sum | js | 0.026368 | 0.026312–0.027031 | 22.0 |
| bun | loop_sum | checked | 0.062487 | 0.058699–0.064776 | 22.9 |
| bun | list_map_fold | sakura | 0.638161 | 0.580922–0.882361 | 58.8 |
| bun | list_map_fold | js | 0.027012 | 0.025279–0.040256 | 62.3 |
| bun | list_map_fold | checked | 0.275412 | 0.258172–0.279050 | 56.2 |
| bun | records | sakura | 0.502561 | 0.487247–0.565764 | 35.7 |
| bun | records | js | 0.171670 | 0.167824–0.400723 | 32.8 |
| bun | records | checked | 0.455583 | 0.445290–0.474378 | 34.1 |
| bun | strings | sakura | 0.062285 | 0.061297–0.134484 | 52.0 |
| bun | strings | js | 0.053416 | 0.051297–0.056491 | 48.6 |
| bun | strings | checked | 0.062080 | 0.059678–0.109349 | 48.7 |
| bun | effects | sakura | 2.339930 | 2.262083–2.455742 | 39.8 |
| bun | effects | js | 0.044152 | 0.042998–0.044803 | 24.6 |
| bun | effects | checked | 0.183537 | 0.181071–0.569893 | 24.4 |
| bun | contracts | sakura | 0.335278 | 0.148434–0.357833 | 26.5 |
| bun | contracts | js | 0.046896 | 0.045811–0.119699 | 21.9 |
| bun | contracts | checked | 0.048628 | 0.046159–0.164503 | 23.1 |
| bun | contracts_plain | sakura | 0.199487 | 0.195070–0.230986 | 35.2 |
| bun | contracts_plain | js | 0.014905 | 0.014221–0.021465 | 22.1 |
| bun | contracts_plain | checked | 0.025443 | 0.025397–0.090783 | 22.2 |

## Whole CLI latency

15 new processes per operation after one warmup; warm filesystem cache; no persistent analysis service.

- `check` median 26.30 ms; min–max 24.76–30.87 ms.
- `build` median 27.17 ms; min–max 26.58–28.06 ms.
