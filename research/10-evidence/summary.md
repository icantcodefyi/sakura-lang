# Process-level summary

Generated from `results.json` by `python3 research/10-evidence/summarize.py`.

| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |
|---|---:|---:|---:|---:|---:|---:|
| fib | 5.881 | 9.36× | 8.85× | 4.064 | 9.98× | 10.78× |
| tail_sum | 0.273 | 2.15× | 0.44× | 0.057 | 1.85× | 0.44× |
| loop_sum | 0.485 | 3.82× | 1.01× | 0.061 | 2.30× | 0.99× |
| list_map_fold | 0.482 | 1.51× | 2.79× | 0.558 | 21.80× | 2.14× |
| records | 0.818 | 1.01× | 1.01× | 0.516 | 2.88× | 1.12× |
| strings | 0.050 | 1.25× | 0.96× | 0.061 | 1.16× | 1.03× |
| effects | 1.989 | 4.12× | 4.10× | 2.164 | 49.56× | 11.58× |
| contracts | 0.269 | 4.97× | 0.69× | 0.340 | 7.30× | 7.40× |
| contracts_plain | 0.062 | 1.77× | 2.42× | 0.211 | 13.97× | 8.32× |

The range below is min–max of five process medians, **not a confidence interval**.
RSS is median of five whole-process peaks, including warmup and common imported harness modules.

| Engine | Workload | Variant | Median ms | Process range ms | Peak RSS MiB |
|---|---|---|---:|---:|---:|
| node | fib | sakura | 5.881469 | 5.814203–6.062396 | 81.0 |
| node | fib | js | 0.628211 | 0.595449–1.651401 | 48.2 |
| node | fib | checked | 0.664909 | 0.625692–0.749022 | 48.5 |
| node | tail_sum | sakura | 0.272836 | 0.270892–0.300397 | 52.0 |
| node | tail_sum | js | 0.127019 | 0.125488–0.362012 | 49.1 |
| node | tail_sum | checked | 0.622677 | 0.620245–0.690923 | 51.7 |
| node | loop_sum | sakura | 0.485194 | 0.482154–0.493618 | 50.5 |
| node | loop_sum | js | 0.126977 | 0.125944–0.127550 | 49.0 |
| node | loop_sum | checked | 0.482547 | 0.481865–0.485784 | 49.5 |
| node | list_map_fold | sakura | 0.481715 | 0.476061–0.825626 | 118.2 |
| node | list_map_fold | js | 0.318520 | 0.300299–0.323859 | 63.2 |
| node | list_map_fold | checked | 0.172961 | 0.171358–0.176702 | 115.0 |
| node | records | sakura | 0.817635 | 0.814629–0.927770 | 53.0 |
| node | records | js | 0.807505 | 0.798546–0.916948 | 51.5 |
| node | records | checked | 0.809487 | 0.804399–0.913135 | 51.9 |
| node | strings | sakura | 0.050014 | 0.049827–0.062249 | 113.5 |
| node | strings | js | 0.039977 | 0.036784–0.066496 | 112.7 |
| node | strings | checked | 0.052223 | 0.051755–0.069668 | 113.1 |
| node | effects | sakura | 1.989104 | 1.959279–4.094011 | 82.9 |
| node | effects | js | 0.482902 | 0.480249–0.484450 | 49.3 |
| node | effects | checked | 0.484712 | 0.482046–0.531528 | 50.6 |
| node | contracts | sakura | 0.268615 | 0.249398–0.297472 | 50.5 |
| node | contracts | js | 0.054015 | 0.052761–0.054801 | 48.6 |
| node | contracts | checked | 0.390516 | 0.383512–0.457509 | 48.5 |
| node | contracts_plain | sakura | 0.061694 | 0.059828–0.111146 | 51.6 |
| node | contracts_plain | js | 0.034777 | 0.033862–0.037455 | 48.9 |
| node | contracts_plain | checked | 0.025541 | 0.025156–0.034624 | 49.5 |
| bun | fib | sakura | 4.063802 | 3.636536–9.332177 | 38.7 |
| bun | fib | js | 0.407063 | 0.395510–0.430512 | 20.7 |
| bun | fib | checked | 0.376832 | 0.375556–0.400638 | 23.0 |
| bun | tail_sum | sakura | 0.056639 | 0.051178–0.159022 | 34.5 |
| bun | tail_sum | js | 0.030568 | 0.029214–0.033224 | 21.7 |
| bun | tail_sum | checked | 0.128528 | 0.127136–0.371165 | 23.3 |
| bun | loop_sum | sakura | 0.061059 | 0.060007–0.165060 | 31.4 |
| bun | loop_sum | js | 0.026491 | 0.026144–0.030207 | 21.2 |
| bun | loop_sum | checked | 0.061755 | 0.059009–0.072633 | 22.5 |
| bun | list_map_fold | sakura | 0.557868 | 0.551058–0.574688 | 59.1 |
| bun | list_map_fold | js | 0.025587 | 0.025561–0.057014 | 59.4 |
| bun | list_map_fold | checked | 0.260315 | 0.253031–0.367099 | 55.8 |
| bun | records | sakura | 0.516359 | 0.477281–0.977089 | 35.0 |
| bun | records | js | 0.179313 | 0.167652–0.183058 | 32.3 |
| bun | records | checked | 0.459241 | 0.447758–0.494853 | 33.5 |
| bun | strings | sakura | 0.061192 | 0.059256–0.062460 | 51.5 |
| bun | strings | js | 0.052671 | 0.051147–0.095959 | 48.3 |
| bun | strings | checked | 0.059226 | 0.058500–0.065236 | 48.6 |
| bun | effects | sakura | 2.163708 | 1.808372–10.499219 | 39.6 |
| bun | effects | js | 0.043662 | 0.042776–0.049281 | 24.3 |
| bun | effects | checked | 0.186912 | 0.180779–0.554017 | 24.5 |
| bun | contracts | sakura | 0.339665 | 0.329908–0.395837 | 25.9 |
| bun | contracts | js | 0.046515 | 0.045973–0.048640 | 22.1 |
| bun | contracts | checked | 0.045899 | 0.045549–0.079282 | 22.7 |
| bun | contracts_plain | sakura | 0.211384 | 0.026148–0.222575 | 34.8 |
| bun | contracts_plain | js | 0.015132 | 0.015085–0.019108 | 21.5 |
| bun | contracts_plain | checked | 0.025410 | 0.025395–0.027792 | 22.1 |

## Whole CLI latency

15 new processes per operation after one warmup; warm filesystem cache; no persistent analysis service.

- `check` median 24.21 ms; min–max 23.47–24.61 ms.
- `build` median 25.26 ms; min–max 24.43–25.73 ms.
