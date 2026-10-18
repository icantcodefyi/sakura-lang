# WASM lane advisory run (§17.191 (2)), 2026-10-19

**Advisory, not a baseline (§17.153).** `bench/bench run --suite wasm --sizes quick --processes 1`
at `72aaf6fe` (clean tree) on the Apple M4 Pro reference host, while other agents were building:
`meta.host.quiet` is `false`, the peak one-minute load 8.79 on 12 CPUs (busy: `claude`, `rustc`,
`sakura`). One process per cell, the first two sizes; Node 26.11.0, Wasmtime 49.0.3. Every
oracle passed on both engines. `json_validate` has no Wasmtime cell: `json_decode` exists only
in the Node host's JS (`meta.absent`). Ratios are Sakura on WASM over hand JS on Node. These
are not the §17.109 M4 envelope, which is measured against the native ports at a quiet window.

| Program | n | js/node ms | sakura/wasm/node ms | sakura/wasm/wasmtime ms | ratio wasm/node | ratio wasm/wasmtime |
|---|---:|---:|---:|---:|---:|---:|
| binary_trees | 8 | 0.1577 | 1.225 | 2.137 | 7.77× | 13.55× |
| binary_trees | 10 | 0.8594 | 6.278 | 11.63 | 7.31× | 13.54× |
| contracts | 1000 | 0.00179 | 0.3848 | 0.5706 | 214.98× | 318.73× |
| contracts | 10000 | 0.01438 | 3.696 | 5.39 | 256.93× | 374.68× |
| contracts_plain | 1000 | 0.001278 | 0.2077 | 0.2722 | 162.56× | 213.03× |
| contracts_plain | 10000 | 0.008304 | 2.011 | 2.672 | 242.19× | 321.81× |
| countdown | 1000 | 0.001508 | 0.004721 | 0.004452 | 3.13× | 2.95× |
| countdown | 10000 | 0.01501 | 0.02595 | 0.03059 | 1.73× | 2.04× |
| deltablue | 100 | 0.04366 | 15.82 | 20.98 | 362.31× | 480.46× |
| deltablue | 1000 | 0.6375 | 197 | 387.3 | 309.09× | 607.51× |
| fib | 20 | 0.06038 | 0.07312 | 0.1094 | 1.21× | 1.81× |
| fib | 24 | 0.2458 | 0.471 | 0.6925 | 1.92× | 2.82× |
| handled_d1 | 1000 | 0.001582 | 0.1529 | 0.2169 | 96.66× | 137.11× |
| handled_d1 | 10000 | 0.01309 | 1.475 | 2.13 | 112.64× | 162.69× |
| handled_d16 | 1000 | 0.001304 | 0.2079 | 0.2728 | 159.42× | 209.20× |
| handled_d16 | 10000 | 0.01299 | 2.001 | 2.554 | 154.08× | 196.61× |
| handled_d4 | 1000 | 0.001307 | 0.1561 | 0.2194 | 119.37× | 167.84× |
| handled_d4 | 10000 | 0.01304 | 1.546 | 2.146 | 118.57× | 164.53× |
| handled_fwd | 1000 | 0.001845 | 0.2486 | 0.3627 | 134.78× | 196.64× |
| handled_fwd | 10000 | 0.01864 | 2.477 | 3.552 | 132.89× | 190.56× |
| handled_reentry | 1000 | 0.0305 | 0.3706 | 0.6668 | 12.15× | 21.87× |
| handled_reentry | 10000 | 0.2483 | 3.722 | 5.37 | 14.99× | 21.63× |
| handled_suspend | 1000 | 0.002309 | 0.1547 | 0.2288 | 66.97× | 99.07× |
| handled_suspend | 10000 | 0.02042 | 1.518 | 2.207 | 74.36× | 108.09× |
| json_validate | 100 | 0.1856 | 0.5091 | — | 2.74× | — |
| json_validate | 1000 | 0.6396 | 5.65 | — | 8.83× | — |
| map_set | 1000 | 0.5025 | 3.516 | 4.342 | 7.00× | 8.64× |
| map_set | 10000 | 8.879 | 38.8 | 44.62 | 4.37× | 5.03× |
| map_set:int | 1000 | 0.05917 | 1.024 | 1.291 | 17.31× | 21.82× |
| map_set:int | 10000 | 0.8199 | 11.04 | 18.07 | 13.47× | 22.04× |
| map_set:string | 1000 | 0.1965 | 1.211 | 1.453 | 6.16× | 7.40× |
| map_set:string | 10000 | 2.305 | 12.77 | 15.34 | 5.54× | 6.65× |
| map_set:tuple | 1000 | 0.1882 | 1.144 | 1.353 | 6.08× | 7.19× |
| map_set:tuple | 10000 | 2.073 | 12.22 | 14.2 | 5.89× | 6.85× |
| nbody | 1000 | 0.04581 | 3.409 | 4.903 | 74.42× | 107.03× |
| nbody | 5000 | 0.2254 | 16.81 | 24.39 | 74.59× | 108.22× |
| pipeline | 1000 | 0.03403 | 0.5239 | 0.827 | 15.40× | 24.31× |
| pipeline | 10000 | 0.3301 | 5.277 | 7.192 | 15.99× | 21.79× |
| rbtree | 1000 | 0.08753 | 3.301 | 3.77 | 37.71× | 43.07× |
| rbtree | 10000 | 1.946 | 40.71 | 52.61 | 20.92× | 27.04× |
| shapes | 1000 | 0.1471 | 2.279 | 2.939 | 15.50× | 19.98× |
| shapes | 10000 | 1.792 | 22.2 | 29.77 | 12.39× | 16.62× |
| sieve | 1000 | 0.003023 | 0.1479 | 0.2466 | 48.94× | 81.58× |
| sieve | 10000 | 0.0337 | 1.642 | 2.757 | 48.73× | 81.81× |
| spectral_norm | 50 | 0.07468 | 3.665 | 5.831 | 49.07× | 78.07× |
| spectral_norm | 100 | 0.3041 | 14.07 | 22.79 | 46.26× | 74.94× |
| tokenizer | 1000 | 0.1784 | 1.015 | 1.3 | 5.69× | 7.29× |
| tokenizer | 10000 | 1.88 | 21.76 | 25.29 | 11.57× | 13.45× |
