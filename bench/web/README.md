# Web tests (TechEmpower style)

Two of TechEmpower's [Framework Benchmarks](https://github.com/TechEmpower/FrameworkBenchmarks)
test types, JSON serialization and plaintext, served by Sakura's std `Serve` and by
reference servers in Bun, Node, Go and Rust. `web.py` builds each server, starts it on
127.0.0.1, checks both endpoints against TechEmpower's requirements, and then loads them with
a pinned load tool.

Numbers from these tests describe this workload on one machine. They are not a ranking of
languages or frameworks.

AI disclosure: written with an AI assistant (Claude); the contributor who lands it owns it.

## Layout

```
bench/web/
  web.py            # the harness (Python 3.11+, stdlib only)
  test_web.py       # its tests: python3 -m unittest discover -s bench/web
  sakura/           # sakura.toml + src/main.sak: std Serve and Router
  bun/server.mjs    # Bun.serve
  node/server.mjs   # node:http
  go/               # net/http (go.mod, main.go)
  rust/             # hyper 1.x + tokio (Cargo.toml, Cargo.lock, main.rs)
```

Build output, logs and the locally installed load tool go in `bench/target/web/`.

## What is measured

| Endpoint | Answer |
|---|---|
| `GET /json` | `{"message":"Hello, World!"}` (27 bytes), `content-type: application/json`, from an object built and serialized on every request |
| `GET /plaintext` | `Hello, World!` (13 bytes), `content-type: text/plain`, response built on every request |

Each answer also carries `server`, `date` and `content-length`.

For each server, endpoint and connection count, the result records:

- requests per second: only `200` answers of the expected size count
- latency p50 and p99 in milliseconds
- errors: transport errors plus any answer other than `200`

Each endpoint is first warmed up at its highest connection count. Then each count is measured
for `--duration` seconds (default 15, as TechEmpower uses). Default connection counts:

| Endpoint | Connections |
|---|---|
| JSON | 16 and 256 |
| plaintext | 256 and 1,024 |

## Servers and lanes

Sakura's runtime serves from one thread per process: the Node or Bun event loop on JS, and a
current-thread executor (hyper on tokio) on native. Every server in the **single** lane
therefore also serves from one thread. The **multi** lane, recorded next to it, uses Go's and
tokio's defaults (one worker per CPU) to show what the hardware allows. Sakura has no cell in the
multi lane: `Serve` cannot start more than one worker or share a port between processes.

| Server | Lane | How it runs |
|---|---|---|
| `sakura-node` | single | `sakura build`, then `node __main__.js <port>`: the same command `sakura run` executes, with the same `SAKURA_PERMISSIONS` grant for that one port |
| `sakura-bun` | single | the same build on `bun` (not a `sakura run` path; Bun's `node:http`) |
| `sakura-native` | single | `sakura build --target native` (experimental), then `main <port>` |
| `bun` | single | `Bun.serve`, one process |
| `node` | single | `node:http`, one process, no cluster |
| `go` | single | `net/http` with `GOMAXPROCS=1` |
| `rust` | single | hyper 1.x, tokio `current_thread` |
| `go-mt` | multi | `net/http`, default `GOMAXPROCS` |
| `rust-mt` | multi | hyper 1.x, tokio `multi_thread` |

The Sakura program reads its port from its first argument. `sakura.toml` lists
`127.0.0.1:8080` for running it by hand (`sakura run bench/web/sakura -- 8080`). The `net` list
has no wildcard or port range, so `web.py` picks a free port and passes the same permission
table `sakura run` would pass for a manifest that lists that port.

## TechEmpower requirements

Source: the [Framework Tests Overview](https://github.com/TechEmpower/FrameworkBenchmarks/wiki/Project-Information-Framework-Tests-Overview).
The checks follow `toolset/test_types/verifications.py`, `json/json.py` and
`plaintext/plaintext.py` at FrameworkBenchmarks `57d92fbe`. `web.py` checks every row marked
"checked" before any load. A server that fails a check is recorded in `meta.failures` and that
endpoint is not loaded. Each server's problems, warnings included, go in `meta.checks`.

**General** (all test types)

| # | Requirement | Status |
|---|---|---|
| G1 | Production-grade implementation | All servers use their language's standard or stock HTTP stack |
| G2 | Realistic HTTP, not "stripped" | Real stacks: `node:http` (Sakura on Node, `node`); Bun's (`sakura-bun` through `node:http`, `bun`); hyper (Sakura native, `rust`); `net/http` (`go`) |
| G3 | No disk logging | Met. Nothing is logged per request. stdout and stderr go to `bench/target/web/logs/` and stay empty unless a server fails |
| G4 | Exact key names and case | Met; checked (`message`) |
| G5 | `Server` and `Date` response headers; `Date` must not be cached | Met by all; checked. `Date` is read twice, 1.1 s apart (TechEmpower waits 3 s) |
| G6 | Some form of request routing | Sakura: `std/router` (URL patterns). Bun: `routes`. Go: `ServeMux`. Node and Rust: a hand-written path switch, which TechEmpower allows where it is the idiom |
| G7 | Database rules | Not applicable (no database test) |
| G8 | A 4-digit port | Met: a free port from 2000–9999 |

**JSON serialization**

| # | Requirement | Status |
|---|---|---|
| J1 | A new object for each request | Met by all. Sakura builds a `Message` record per call; the generated JS was checked for this |
| J2 | URI `/json` | Met |
| J3 | Uses a JSON serializer | Sakura `Json.encode` (derived codec), `JSON.stringify`, `encoding/json`, `serde_json` |
| J4–J5 | Body `{"message":"Hello, World!"}`, about 28 bytes | Met; checked, byte for byte (stricter than TechEmpower) |
| J6 | `content-type: application/json` | Met; checked against TechEmpower's pattern |
| J7 | `Content-Length` or `Transfer-Encoding` | Met (`content-length`); checked, and its value must match the body |
| J8 | `Server` and `Date` | See G5 |
| J9 | No gzip | Met; checked |
| J10–J11 | Keep-alive encouraged, no maximum timeout | All keep connections alive; the load tool reuses them. Node's (and Sakura-on-Node's) keep-alive idle timeout is Node's default 5 s, which matters only to idle connections |
| J12 | Concurrency 16–512 | 16 and 256 by default (`--json-connections`) |
| J13 | `GET` | Met |
| J14 | Serialization not cached | Met by all |

**Plaintext**

| # | Requirement | Status |
|---|---|---|
| P1 | URI `/plaintext` | Met |
| P2 | `content-type: text/plain` | Met; checked |
| P3 | Body `Hello, World!` | Met; checked, byte for byte |
| P4 | Response composed per request (a reused body buffer is allowed) | Met by all |
| P5–P7 | Length header, `Server`/`Date`, no gzip | Met; checked |
| P8, P10 | Keep-alive | As J10 |
| P9 | HTTP/1.1 pipelining is assumed | **Deviation in the load:** `oha` does not pipeline (see below). The check sends 16 pipelined requests on one connection, TechEmpower's depth; every server answers all 16 |
| P11 | Concurrency 256, 1,024, 4,096 and 16,384 | **Deviation:** 256 and 1,024 by default (`--plaintext-connections`). With the load tool and the server on one machine, 4,096 and more mostly measure descriptor, port and accept-queue limits |
| P12 | `GET` | Met |

## Load tool

The load tool is [`oha`](https://github.com/hatoo/oha), pinned in `bench/toolchains.toml`
(`oha = "1.16.0"`). It writes JSON (`--output-format json`), uses HTTP/1.1 keep-alive, and with
`-w` it lets requests still running at the deadline finish instead of counting them as errors.
TechEmpower uses `wrk`, which pipelines through a Lua script. We chose `oha` because:

- it installs with `cargo install --locked`, from a toolchain the bench already has;
- its machine-readable output needs no Lua script and no parsing of text output;
- it is versioned on crates.io.

The cost is pipelining. Plaintext numbers here are one request at a time per connection, not
TechEmpower's 16 deep, so they are not comparable with TechEmpower's published plaintext results.

`web.py` finds `oha` from `--oha`, then `$OHA`, then `bench/target/web/tools/bin/oha`, then
`PATH`, and records its version next to the pin. If no `oha` is found, the servers are still
built and checked, nothing is timed, and the exit code is 3.

## Running

```sh
python3 bench/web/web.py --json out.json                     # every server, defaults above
python3 bench/web/web.py --check-only                        # build, start and check; time nothing
python3 bench/web/web.py --servers sakura-node,node --duration 5 --json out.json
python3 -m unittest discover -s bench/web                    # the harness's tests
```

Install the load tool locally, inside the bench's build directory (about 0.5 GB of build files,
which can be deleted after the install):

```sh
CARGO_TARGET_DIR=bench/target/web/oha-build \
  cargo install oha --version 1.16.0 --locked --root bench/target/web/tools
```

Finding the Sakura CLI: `--sakura`, then `$SAKURA`, then
`compiler/target/release/sakura`. The native cell needs `SAKURA_NATIVE_LIB_DIR`, the native
driver's directory. It defaults to `bench/target/native-driver/release` when that directory
holds the driver. `--build-cli` builds both, the way `bench run --suite native` does: the CLI
with the `native` feature, and the driver.

Toolchains that are missing are skipped and listed in `meta.missing` (exit 3), as in
`bench/bench run`. If a server fails to build, start or pass a check, it goes in `meta.failures`
and is not timed (exit 1). A failed native build is recorded in `meta.absent` with the
compiler's message.

### In CI

```sh
python3 bench/web/web.py --build-cli --json bench-web.json
```

This needs Node, Bun, Go, Rust and `cc` at the pins, and `oha` on `PATH` or in `$OHA`.
Install `oha` with `cargo install oha --version <pin> --locked --root <dir>` and cache `<dir>`
keyed on the pin. Linux runners are the better host. macOS's `kern.ipc.somaxconn` is 128, so
1,024 connections opened at once can overflow the accept queue. In one smoke run on a Mac,
`sakura-node`, `sakura-bun` and `node` had connection errors at 1,024 connections; a rerun had
none. Such errors are recorded in the `errors` column. On a macOS runner, raise the limit first (`sudo sysctl -w kern.ipc.somaxconn=4096`).

## Result format (`sakura-web-v1`)

```
{"schema": "sakura-web-v1",
 "meta": {"commit", "dirty", "host" (bench.py's host id, CPU, OS, memory), "ci", "started",
          "tool": {"name": "oha", "version", "pinned", "worker_threads", "pipelining": 1, ...},
          "runtimes": {"node", "bun", "go", "rustc", "cc", "sakura"},
          "flags": {"duration_s", "warmup_s", "connections", "servers", "fd_limit", ...},
          "load_1m": {"start", "end"}, "missing", "absent", "failures", "checks"},
 "results": [{"server", "lane", "runtime", "endpoint", "connections", "requests_per_s",
              "latency_ms": {"p50", "p99"}, "requests", "errors", "status_codes",
              "error_kinds", "duration_s", "bytes_per_response", "size_ok", "warnings", ...}]}
```

`web.py` prints the same cells to stdout as a Markdown table.

## Limits

- The load generator runs on the same machine as the server and competes with it for CPU.
  `oha` starts one worker per physical core unless `--tool-threads` is set. This affects the
  multi lane most.
- Hosted CI runners are shared, noisy virtual machines. A run there is a trend, never a baseline,
  as in the rest of the bench. Local numbers on a loaded machine say even less.
- There is no pipelining (P9), and only part of TechEmpower's concurrency range (P11).
- The load is one fixed request over loopback. There is no database, no TLS and no real network.
  The numbers describe this workload on this machine; they are not rankings.
- Sakura's native target is experimental, and its cell uses the CLI's current native runtime.
