# Process-level summary

Generated from `research-first-green-exact.json` by `python3 research/10-evidence/summarize.py`.

| Workload | Node Sakura ms | Node / JS | Node / checked | Bun Sakura ms | Bun / JS | Bun / checked |
|---|---:|---:|---:|---:|---:|---:|
| fib | 5.824 | 9.94× | 8.79× | 3.675 | 9.34× | 9.84× |
| tail_sum | 0.276 | 2.18× | 0.45× | 0.045 | 1.51× | 0.35× |
| loop_sum | 0.485 | 3.86× | 1.01× | 0.059 | 2.24× | 1.00× |
| list_map_fold | 0.470 | 1.57× | 2.74× | 0.545 | 21.23× | 2.16× |
| records | 0.816 | 1.02× | 1.01× | 0.486 | 2.70× | 1.07× |
| strings | 0.050 | 1.33× | 0.97× | 0.059 | 1.17× | 1.01× |
| effects | 2.267 | 4.68× | 4.73× | 2.179 | 50.91× | 12.04× |
| contracts | 0.249 | 4.68× | 0.65× | 0.330 | 7.17× | 7.21× |
| contracts_plain | 0.061 | 1.80× | 2.42× | 0.213 | 14.57× | 8.39× |

The range below is min–max of five process medians, **not a confidence interval**.
RSS is median of five whole-process peaks, including warmup and common imported harness modules.

| Engine | Workload | Variant | Median ms | Process range ms | Peak RSS MiB |
|---|---|---|---:|---:|---:|
| node | fib | sakura | 5.824031 | 5.781083–5.891708 | 81.2 |
| node | fib | js | 0.585969 | 0.579021–0.597779 | 48.0 |
| node | fib | checked | 0.662713 | 0.624868–0.686993 | 48.5 |
| node | tail_sum | sakura | 0.275802 | 0.267117–0.443185 | 52.0 |
| node | tail_sum | js | 0.126800 | 0.125584–0.141625 | 49.0 |
| node | tail_sum | checked | 0.614994 | 0.612553–0.623690 | 50.6 |
| node | loop_sum | sakura | 0.484519 | 0.477520–0.561836 | 50.7 |
| node | loop_sum | js | 0.125544 | 0.125434–0.127452 | 49.1 |
| node | loop_sum | checked | 0.481831 | 0.476331–0.487972 | 49.7 |
| node | list_map_fold | sakura | 0.470304 | 0.463410–0.479930 | 118.5 |
| node | list_map_fold | js | 0.298916 | 0.295738–0.303405 | 62.9 |
| node | list_map_fold | checked | 0.171531 | 0.167874–0.177218 | 115.0 |
| node | records | sakura | 0.815931 | 0.808440–0.853123 | 53.0 |
| node | records | js | 0.803484 | 0.798928–0.806329 | 51.6 |
| node | records | checked | 0.807278 | 0.798298–1.396573 | 51.8 |
| node | strings | sakura | 0.050317 | 0.048318–0.050995 | 113.1 |
| node | strings | js | 0.037894 | 0.037755–0.039679 | 112.6 |
| node | strings | checked | 0.051912 | 0.050701–0.052517 | 113.1 |
| node | effects | sakura | 2.267391 | 2.216857–5.781576 | 83.3 |
| node | effects | js | 0.484376 | 0.477661–0.671135 | 49.5 |
| node | effects | checked | 0.479486 | 0.476574–0.846184 | 50.8 |
| node | contracts | sakura | 0.249073 | 0.247868–0.266918 | 50.5 |
| node | contracts | js | 0.053258 | 0.052682–0.069692 | 48.9 |
| node | contracts | checked | 0.384608 | 0.381734–0.599974 | 48.5 |
| node | contracts_plain | sakura | 0.061260 | 0.058925–0.063087 | 51.3 |
| node | contracts_plain | js | 0.034085 | 0.033467–0.037555 | 49.0 |
| node | contracts_plain | checked | 0.025324 | 0.025075–0.028085 | 49.2 |
| bun | fib | sakura | 3.675469 | 3.658602–10.076766 | 40.1 |
| bun | fib | js | 0.393714 | 0.388865–0.430820 | 21.0 |
| bun | fib | checked | 0.373491 | 0.372520–0.377013 | 23.4 |
| bun | tail_sum | sakura | 0.044595 | 0.043054–0.061680 | 35.8 |
| bun | tail_sum | js | 0.029472 | 0.029231–0.029618 | 21.8 |
| bun | tail_sum | checked | 0.126010 | 0.125396–0.140902 | 23.8 |
| bun | loop_sum | sakura | 0.058854 | 0.058163–0.062525 | 35.1 |
| bun | loop_sum | js | 0.026226 | 0.026018–0.026334 | 22.2 |
| bun | loop_sum | checked | 0.059017 | 0.057849–0.125429 | 22.8 |
| bun | list_map_fold | sakura | 0.544688 | 0.540357–0.558592 | 59.1 |
| bun | list_map_fold | js | 0.025652 | 0.025296–0.039703 | 58.2 |
| bun | list_map_fold | checked | 0.252676 | 0.251299–0.253772 | 56.1 |
| bun | records | sakura | 0.485846 | 0.469988–0.504977 | 35.4 |
| bun | records | js | 0.180054 | 0.165990–0.182214 | 32.8 |
| bun | records | checked | 0.452149 | 0.440532–0.457410 | 33.4 |
| bun | strings | sakura | 0.058806 | 0.057004–0.059958 | 53.2 |
| bun | strings | js | 0.050266 | 0.050013–0.083185 | 48.2 |
| bun | strings | checked | 0.058076 | 0.057954–0.060406 | 48.6 |
| bun | effects | sakura | 2.179490 | 2.159172–2.329323 | 39.8 |
| bun | effects | js | 0.042809 | 0.042472–0.042908 | 24.5 |
| bun | effects | checked | 0.181035 | 0.177974–0.183766 | 24.8 |
| bun | contracts | sakura | 0.329598 | 0.325868–0.330626 | 26.5 |
| bun | contracts | js | 0.045939 | 0.045210–0.055780 | 22.5 |
| bun | contracts | checked | 0.045715 | 0.045153–0.046576 | 22.7 |
| bun | contracts_plain | sakura | 0.213431 | 0.192113–0.217427 | 35.3 |
| bun | contracts_plain | js | 0.014653 | 0.012953–0.021233 | 21.6 |
| bun | contracts_plain | checked | 0.025447 | 0.025209–0.037355 | 22.3 |

## Whole CLI latency

15 new processes per operation after one warmup; warm filesystem cache; no persistent analysis service.

- `check` median 25.19 ms; min–max 25.00–26.67 ms.
- `build` median 25.66 ms; min–max 25.20–26.78 ms.
