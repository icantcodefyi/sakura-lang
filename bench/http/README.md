# HTTP load test: a small social app

How many requests per second can a Sakura web server take, and what happens when it gets more?
This directory has one small app written six times over the same storage: in Sakura
(served four ways) and in plain Bun, Node and Go. `run.py` starts each one, fills it with
data, and loads it until it falls behind.

Numbers from these tests describe this app on one machine. They are not a ranking of
languages.

AI disclosure: written with an AI assistant (Claude); the contributor who lands it owns it.

## The app

Every server answers the same routes with the same SQL, status codes and JSON:

| Route | Answer |
|---|---|
| `POST /users` `{"name"}` | `201` `{"id","name"}`; `400` if the name is not 1–64 characters or the body is not JSON |
| `POST /posts` `{"user_id","body"}` | `201` `{"id","user_id","body","likes":0}`; `404` if the user does not exist; `400` for a bad body (1–280 characters) |
| `GET /posts/:id` | `200` `{"id","user_id","body","likes"}`; `404` if missing |
| `POST /posts/:id/like` `{"user_id"}` | `200` `{"post_id","likes"}` (liking twice counts once); `404` if the post is missing |
| `GET /feed?limit=20` | `200` `{"posts":[…]}`, the latest posts (highest ids) with their like counts; `limit` 1–100, 20 by default |
| `GET /health` | `200` `ok` (`text/plain`) |

**Storage: SQLite, in memory, in every server.** Three tables (`users`, `posts`,
`likes` with primary key `(post_id, user_id)`); a post's like count is a `count(*)` subquery
on every read. The database lives in the server process and is empty at start.

| Server | Code | HTTP | SQLite |
|---|---|---|---|
| `sakura-node` | [`sakura/src/main.sak`](sakura/src/main.sak) | std `Serve` + `Router`, `sakura build` run by `node` (as `sakura run` does) | std `Sql.memory()` (`node:sqlite`) |
| `sakura-bun` | the same build | run by `bun` | std `Sql.memory()` (Bun's `node:sqlite`) |
| `sakura-wasm` | the same source | `sakura build --target wasm`, run by `node sakura_wasm_host.js` (as `sakura run --target wasm` does) | std `Sql.memory()` through the host (`node:sqlite`) |
| `sakura-native` | the same source | `sakura build --target native` (experimental): hyper on a single-threaded tokio executor | std `Sql.memory()` on the runtime's database thread |
| `bun` | [`bun/server.mjs`](bun/server.mjs) | `Bun.serve` with `routes` | `bun:sqlite` |
| `node` | [`node/server.mjs`](node/server.mjs) | `node:http`, one process, a hand-written path match | `node:sqlite` |
| `go` | [`go/main.go`](go/main.go) | `net/http` `ServeMux`, `GOMAXPROCS=1` | `database/sql` + `modernc.org/sqlite` (pure Go) |
| `go-mt` | the same binary | default `GOMAXPROCS` (one thread per CPU it may use) | the same; one connection, see below |

Differences that matter:

- The Bun, Node and Go servers prepare each statement once at start, as their docs suggest.
  Sakura's `Sql` has no prepared-statement API; its runtime prepares the statement on every
  call.
- An in-memory SQLite database belongs to one connection, so Go's pool is held to one
  connection. `go-mt` serves HTTP on all its threads but every query waits for that one
  connection. In-memory SQLite was chosen so that every server has the same storage. A file
  database would have added each driver's file locking and journal settings to the comparison.
- `modernc.org/sqlite` is SQLite translated to Go. It is slower than the C SQLite inside Node
  and Bun, so the Go rows measure that driver as much as `net/http`.
- Sakura, Bun and Node serve from one JavaScript thread (or one executor thread on native).
  `go` is held to one thread to match; `go-mt` shows what Go does with more.

## Method

`run.py` does this for each server and each mix:

1. Starts a fresh server on a free port on 127.0.0.1.
2. Seeds it over HTTP with [`seed.py`](seed.py): 1,000 users, 10,000 posts by random users,
   and 20,000 distinct likes. The seed checks every status and id, so every server holds the
   same data. `run.py` then checks the JSON keys and order of `/feed` and `/posts/1`.
3. Warms up for 5 s at 32 connections. Nothing is recorded.
4. **Closed loop**: 1, 8, 32, 128, 512 and 1,024 connections. Each connection sends its next
   request as soon as the last answer arrives. This shows the most the server sustains at each
   concurrency.
5. **Open loop (fixed rate)**: requests are sent at a set rate whatever the server does:
   0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5 and 2 times the best closed-loop rate, over at most 256
   connections. Latency counts from when each request *should* have been sent, so time spent
   waiting behind a slow server shows up in the latency. The closed loop hides that wait.
6. Every point runs 3 times for 5 s. Tables show the median and the spread (min–max).
7. Stops the server. The next mix gets a new process and a new database.

Writes add rows during the run, so the database grows during each mix. Peak RSS includes that
growth.

**Mixes** (weights per 100 requests; ids drawn from the seeded ranges, so reads find their row):

| Mix | `GET /feed?limit=20` | `GET /posts/:id` | `POST /posts/:id/like` | `POST /posts` | `POST /users` |
|---|---|---|---|---|---|
| `read` (80% reads) | 40 | 40 | 10 | 7 | 3 |
| `write` (80% writes) | 10 | 10 | 50 | 25 | 5 |
| `health` (ceiling) | `GET /health` only | | | | |

**Definitions**

- *RPS*: `2xx` answers completed inside the run's window, divided by its length.
- *Error rate*: non-`2xx` answers plus transport errors, over all attempts. In the open loop,
  requests the generator could not send before the run ended (all 256 connections busy) count
  as errors of kind `unsent`.
- *p50/p99*: latency of every answer, in milliseconds.
- *Max sustained RPS*: the best closed-loop median whose error rate is under 0.1%.
- *Knee*: the highest fixed rate at which the server completed at least 95% of the offered
  rate with an error rate under 0.1%.
- *Failure mode*: what the first fixed rate past the knee looked like (`503`s or other status
  codes, timeouts, resets, requests left unsent), and whether the server process survived.
  Errors of kind `connect_*` failed while opening the connection.
- *RSS*: the server process's resident memory, sampled every 250 ms; the peak per point.
- *Load*: the host's 1-minute load average before and after each run; the table shows the
  highest.

### Load generator

[`loadgen/`](loadgen) is a small Go program (standard library only) written for this test.
None of the usual tools does everything the test needs:

- `oha` sends one request shape per run, so it cannot mix GETs and POSTs with bodies.
- `wrk` mixes requests through Lua, but its fixed-rate fork `wrk2` must be built from source,
  and `wrk` itself does not count latency from when a request should have been sent.

`loadgen` writes HTTP/1.1 over keep-alive TCP connections by hand and reads each answer to its
end (`content-length`, chunked, or close). It opens all its connections one at a time before
the measured window starts, as `wrk` does. Without that, on macOS (whose accept queue holds
128 connections, `kern.ipc.somaxconn`) a burst of 256 or 1,024 simultaneous connects was reset
by the kernel, and every reset led to a reconnect that was reset again. Those runs measured the
kernel's queue, not the server. Opening connections stops after 10 s; any still unopened are
counted (`setup_connect_failures`) and opened by their first request, inside the window. A
connection the server closes is reopened inside the window. A first version used Go's `net/http` client and
reached only about 77,000 requests per second on `/health` where `oha` reached about 117,000
against the same server: the generator was the bottleneck. The hand-written client reached
140,000 against `oha`'s 144,000, and 149,000 against `oha`'s 153,000 on Bun, so it has room to
spare on every mix here. Each run also checks it with one `oha` run on `/health` at 128
connections (the last table of each result).

## Running

```sh
python3 bench/http/run.py --json out.json --md out.md      # all servers and mixes (about 100 minutes)
python3 bench/http/run.py --servers sakura-native,go --mixes read --repeats 1 --duration 2
python3 bench/http/run.py --report bench/http/results/2026-10-23-linux-ci.json   # summary tables
python3 bench/http/seed.py http://127.0.0.1:8080           # seed a server started by hand
```

Needs Python 3.11+, Go (to build the Go server and `loadgen`), Node, Bun, and the `sakura` CLI
(`--sakura`, `$SAKURA`, or `PATH`). `oha` is optional. A server whose build fails is listed with
the failure and skipped. Build output and server logs go to `bench/target/http/`.

On Linux with `taskset` and at least 4 CPUs, the server runs on the first half of the CPUs and
the load generator on the second (`--server-cpus`, `--load-cpus`). On macOS nothing is pinned.

`.github/workflows/http-load.yml` runs the whole test on a GitHub-hosted `ubuntu-latest` runner,
by hand (`workflow_dispatch`). It installs `sakuralang` from npm, and Bun, Node and Go at the
`bench/toolchains.toml` pins. The results are uploaded as the `http-load-linux` artifact.


## Results: 2026-10-23

Both runs used `sakura 1.0.0` (`36e64df4`) and this directory at commit `cd3bb3fd`, with Node
26.12.0, Bun 1.4.3, Go 1.27.2 and `oha` 1.16.0. Raw results:
[`results/`](results). Tables below come from `python3 bench/http/run.py --report <file>.json`.

| Run | Machine | Pinning | Host load (1-minute average over the run) |
|---|---|---|---|
| [`2026-10-23-linux-ci`](results/2026-10-23-linux-ci.md), [workflow run 39968982325](https://github.com/icantcodefyi/sakuralang/actions/runs/39968982325) | GitHub-hosted `ubuntu-latest`: AMD EPYC 7763, 4 vCPUs (2 cores × 2 threads), 16 GB, Linux 6.17 ([host](results/2026-10-23-linux-ci-host.txt)) | server on CPUs 0–1, load generator on CPUs 2–3 (`taskset`). CPUs 0–1 are the two threads of **one** physical core | 0.9–4.1, median 1.2 |
| [`2026-10-23-mac-m4pro`](results/2026-10-23-mac-m4pro.md) | Apple M4 Pro, 12 cores, 24 GB, macOS (Darwin 27.2) | none | 3.1–44.8, median 7.2: other agents were building at the same time. **Noisy** |

### Linux runner

Mix `read`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 7,279 (7,268–7,291) at 32 | 4.2 / 8.4 | 6,551 (8.1) | 7,279 → 7,179/s, 71 ms, unsent ×775 | 203 | 3.4 |
| sakura-bun | 7,881 (7,869–7,900) at 8 | 0.9 / 2.3 | 7,093 (8.1) | 7,881 → 7,782/s, 66 ms, unsent ×1,404 | 100 | 1.2 |
| sakura-wasm | 3,982 (3,970–3,991) at 32 | 7.6 / 15.5 | 3,584 (10.5) | 3,982 → 3,908/s, 104 ms, unsent ×375 | 192 | 1.6 |
| sakura-native | 9,877 (9,867–9,883) at 128 | 13.4 / 16.3 | 8,889 (2.3) | 9,877 → 9,747/s, 68 ms, unsent ×1,169 | 50 | 1.5 |
| bun | 23,956 (23,718–24,266) at 8 | 0.3 / 0.7 | 21,561 (2.8) | 23,956 → 23,722/s, 47 ms, unsent ×2,785 | 64 | 1.3 |
| node | 13,135 (13,074–13,157) at 32 | 2.4 / 4.8 | 11,822 (6.2) | 13,135 → 13,037/s, 43 ms, unsent ×872 | 181 | 1.5 |
| go | 11,536 (11,526–11,586) at 8 | 0.7 / 1.5 | 10,383 (6.2) | 11,536 → 10,664/s, 378 ms, unsent ×12,817 | 71 | 1.4 |
| go-mt | 9,591 (9,546–9,593) at 8 | 0.7 / 2.8 | 8,632 (12.3) | 9,591 → 8,931/s, 374 ms, unsent ×9,541 | 87 | 2.1 |

Mix `write`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 7,054 (7,042–7,056) at 32 | 4.3 / 8.7 | 6,349 (8.1) | 7,054 → 6,823/s, 164 ms, unsent ×2,893 | 191 | 1.9 |
| sakura-bun | 7,347 (7,271–7,399) at 8 | 0.9 / 2.5 | 6,612 (5.6) | 7,347 → 7,216/s, 89 ms, unsent ×1,692 | 108 | 1.5 |
| sakura-wasm | 3,753 (3,744–3,771) at 8 | 1.8 / 3.9 | 3,378 (11.2) | 3,753 → 3,597/s, 208 ms, unsent ×1,469 | 192 | 1.4 |
| sakura-native | 9,599 (9,597–9,601) at 128 | 13.5 / 17.3 | 8,639 (2.4) | 9,599 → 9,535/s, 37 ms, unsent ×197 | 64 | 1.6 |
| bun | 26,311 (24,986–26,512) at 128 | 4.8 / 5.8 | 23,680 (3.7) | 26,311 → 26,162/s, 30 ms, unsent ×1,085 | 111 | 1.4 |
| node | 11,771 (11,763–11,801) at 32 | 2.6 / 5.3 | 11,771 (18.1) | 12,948 → 11,732/s, 466 ms, unsent ×18,182 | 194 | 1.4 |
| go | 11,411 (11,212–11,447) at 8 | 0.7 / 1.4 | 10,270 (9.7) | 11,411 → 10,614/s, 351 ms, unsent ×11,160 | 90 | 1.3 |
| go-mt | 10,894 (10,850–10,930) at 32 | 2.4 / 10.2 | 9,805 (15.1) | 10,894 → 10,163/s, 354 ms, unsent ×10,125 | 105 | 1.9 |

Mix `health`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 21,631 (21,377–21,663) at 128 | 5.8 / 6.7 | 21,631 (21.2) | 23,794 → 22,842/s, 198 ms, unsent ×13,937 | 167 | 1.9 |
| sakura-bun | 24,162 (23,923–24,452) at 8 | 0.3 / 1.0 | 21,746 (5.4) | 24,162 → 23,725/s, 90 ms, unsent ×6,493 | 93 | 1.1 |
| sakura-wasm | 9,439 (9,392–9,458) at 32 | 3.2 / 6.5 | 8,495 (6.7) | 9,439 → 9,351/s, 47 ms, unsent ×649 | 180 | 1.2 |
| sakura-native | 36,205 (36,145–36,219) at 128 | 3.5 / 4.3 | 32,585 (1.7) | 36,205 → 36,102/s, 16 ms, unsent ×2,833 | 46 | 2.3 |
| bun | 91,721 (91,608–91,725) at 32 | 0.3 / 0.7 | 82,549 (4.1) | 91,721 → 91,405/s, 18 ms, unsent ×4,868 | 46 | 2.8 |
| node | 47,780 (47,740–47,826) at 128 | 2.6 / 3.2 | 43,002 (1.9) | 47,780 → 45,974/s, 187 ms, unsent ×26,398 | 170 | 2.6 |
| go | 53,868 (53,636–53,896) at 32 | 0.6 / 1.3 | 48,481 (5.0) | 53,868 → 51,208/s, 245 ms, unsent ×39,821 | 50 | 1.9 |
| go-mt | 74,926 (74,376–75,047) at 32 | 0.4 / 1.3 | 67,433 (4.6) | 74,926 → 74,114/s, 55 ms, unsent ×11,418 | 51 | 4.1 |

Read the "Max load" column of every table as the noise label: on this runner it stayed low.

**Knee.** Every server's knee was 0.9× or 1.0× its best closed-loop rate (the ramp's steps there
are 0.9×, 1.0× and 1.1×). At every rate a server
kept up with, its p99 latency was at most 21 ms. At the first point past the knee, each server still
completed 91–99% of the offered rate; the rest waited.

**Failure mode: queueing, not errors.** No server crashed, and no server answered anything but
`2xx`, at any point: 1 to 1,024 connections in the closed loop, and up to twice the server's
capacity in the open loop. No request hit the 5 s timeout. At 2× capacity for 5 s, each server
still completed 92–105% of its best rate. Its p99 latency rose to about 2.5 s, because the
excess waited in line and the line grew for the whole run. None of them sheds load, so none
answers `503` to say it is overloaded. That includes Sakura's `Serve`: its documented `503`
is for a request that arrives while the server is stopping, or one the WASM guest heap cannot
hold, not for overload. Waiting time grows until the client gives up.

**Memory.** Peak RSS across every point: `sakura-native` 46–64 MiB, `go`/`go-mt` 50–105 MiB,
`bun` 47–111 MiB, `sakura-bun` 93–108 MiB, `node` 170–194 MiB, `sakura-wasm` 180–192 MiB,
`sakura-node` 167–203 MiB. Memory rose with the connection count. Under `/health` it then
stayed flat; under the write mix it kept rising slowly with the rows written. Runs of 5 s are too
short to rule out a slow leak.

**`oha` cross-check** (`/health`, 128 connections, one run each): `oha` measured 1–11% less than
`loadgen` on every server (for example 21,380 against 21,631 on `sakura-node`, 87,966 against
91,624 on `bun`). Both share the same two load CPUs, and `oha` uses more CPU per request. Its
roughly 127 "errors" per run are the requests still in flight at its deadline.

### Mac (noisy)

Mix `read`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 30,127 (29,882–31,101) at 512 | 16.9 / 22.4 | none | 15,064 → 10,926/s, 1,349 ms, unsent ×60,376 | 231 | 22.9 |
| sakura-bun | 34,028 (33,390–34,033) at 32 | 0.9 / 2.0 | 25,521 (171.5) | 30,625 → 4,125/s, 4,347 ms, unsent ×395,498 | 164 | 21.5 |
| sakura-wasm | 14,765 (9,487–14,871) at 512 | 34.4 / 43.3 | 13,289 (3.0) | 14,765 → 2,823/s, 4,168 ms, unsent ×146,019 | 226 | 23.0 |
| sakura-native | 37,563 (35,128–37,628) at 128 | 3.3 / 4.9 | 33,807 (8.3) | 37,563 → 36,974/s, 80 ms, unsent ×7,337 | 82 | 34.9 |
| bun | 75,290 (71,273–75,367) at 8 | 0.1 / 0.2 | 56,467 (312.6) | 37,645 → 36,679/s, 458 ms, unsent ×22,566 | 74 | 21.6 |
| node | 28,760 (17,914–29,324) at 1024 | 21.6 / 177.4 | none | 14,380 → 14,254/s, 179 ms, unsent ×7,675 | 206 | 44.8 |
| go | 17,344 (14,993–18,959) at 8 | 0.2 / 5.5 | 8,672 (10.8) | 13,008 → 12,541/s, 363 ms, unsent ×5,191 | 83 | 39.3 |
| go-mt | 42,635 (42,003–44,970) at 8 | 0.1 / 0.7 | 38,372 (31.4) | 42,635 → 39,105/s, 413 ms, unsent ×52,529 | 144 | 25.2 |

Mix `write`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 32,904 (32,823–33,021) at 32 | 0.9 / 1.9 | 29,614 (4.9) | 32,904 → 31,253/s, 248 ms, unsent ×24,373 | 266 | 7.8 |
| sakura-bun | 32,303 (31,822–32,405) at 32 | 0.9 / 2.0 | 29,073 (4.6) | 32,303 → 31,391/s, 141 ms, unsent ×13,098 | 210 | 4.5 |
| sakura-wasm | 15,398 (14,526–15,583) at 32 | 2.0 / 4.1 | 13,858 (0.9) | 15,398 → 2,570/s, 4,200 ms, unsent ×190,636 | 248 | 19.4 |
| sakura-native | 40,907 (40,473–41,065) at 128 | 3.1 / 5.2 | 36,816 (19.9) | 40,907 → 40,007/s, 111 ms, unsent ×14,259 | 149 | 20.1 |
| bun | 73,935 (73,472–74,744) at 32 | 0.4 / 0.9 | 66,542 (5.2) | 73,935 → 69,298/s, 311 ms, unsent ×72,716 | 203 | 8.7 |
| node | 58,027 (54,780–58,292) at 128 | 2.2 / 2.6 | 43,520 (0.3) | 52,224 → 52,039/s, 18 ms, unsent ×2,842 | 310 | 6.5 |
| go | 48,151 (46,386–48,539) at 32 | 0.7 / 1.5 | 36,114 (2.0) | 43,336 → 41,130/s, 249 ms, unsent ×32,796 | 272 | 6.8 |
| go-mt | 45,418 (45,286–45,767) at 8 | 0.1 / 0.6 | 34,064 (3.3) | 40,877 → 40,183/s, 90 ms, unsent ×9,290 | 285 | 7.9 |

Mix `health`

| Server | Max sustained RPS (connections) | p50 / p99 ms there | Knee RPS (p99 ms) | Past the knee: offered → done, p99, errors | Peak RSS MiB | Max load |
|---|---|---|---|---|---|---|
| sakura-node | 81,750 (81,266–83,081) at 32 | 0.4 / 0.8 | 81,750 (21.5) | 89,925 → 81,285/s, 475 ms, unsent ×127,505 | 162 | 7.7 |
| sakura-bun | 79,530 (78,922–82,797) at 8 | 0.1 / 0.4 | 71,577 (8.2) | 79,530 → 76,450/s, 191 ms, unsent ×55,065 | 155 | 5.8 |
| sakura-wasm | 39,314 (38,396–39,322) at 8 | 0.2 / 0.4 | 35,382 (1.1) | 39,314 → 38,649/s, 84 ms, unsent ×10,359 | 219 | 5.7 |
| sakura-native | 76,136 (75,812–76,861) at 32 | 0.4 / 0.5 | 68,522 (0.2) | 76,136 → 74,834/s, 86 ms, unsent ×20,678 | 57 | 6.8 |
| bun | 147,477 (147,352–147,495) at 128 | 0.9 / 1.1 | 147,477 (2.6) | 162,225 → 149,247/s, 396 ms, unsent ×194,567 | 43 | 7.8 |
| node | 119,055 (118,832–119,092) at 128 | 1.1 / 1.2 | 119,055 (3.4) | 130,960 → 116,014/s, 566 ms, unsent ×293,760 | 167 | 20.0 |
| go | 117,951 (111,599–118,658) at 32 | 0.3 / 0.6 | 106,156 (1.2) | 117,951 → 112,796/s, 215 ms, unsent ×76,952 | 50 | 17.2 |
| go-mt | 151,650 (151,548–151,932) at 512 | 3.0 / 10.7 | 75,825 (2.3) | 113,738 → 99,938/s, 604 ms, unsent ×283,551 | 62 | 42.7 |

The Mac's numbers are higher than the runner's, but other work loaded the machine
throughout. That shows in the spreads and in the load column. In the open loop, some points near
1.0× completed far less than the server's capacity (for example `sakura-wasm` on the write mix:
about 2,500/s offered 15,398/s), while the load average climbed. The same happened to hand-written
`bun` on the read mix and to `go` on the read mix, and it never happened on the Linux runner. A
direct retry at a load average of 60–96 showed `go-mt` keeping its rate while `sakura-node` fell
to a third of it. These collapses were not explained. Treat Mac open-loop points at or past 0.9×
as unreliable, and use the Linux tables for comparisons.

## Findings about Sakura

From the Linux runner unless stated:

1. **Stable under overload.** Sakura's `Serve` did not crash, leak, or return a wrong status on
   any backend, up to 1,024 connections and twice its capacity. Past capacity it queues: no
   `503`, no shedding, so latency grows without a bound set by the server.
2. **Throughput is below hand-written servers on the same runtime.** `/health`: `sakura-node`
   21,631 RPS against `node`'s 47,780; `sakura-bun` 24,162 against `bun`'s 91,721. Read mix:
   `sakura-node` 7,279 against 13,135; `sakura-bun` 7,881 against 23,956. Likely contributors, **not measured
   here**: the `Sql` runtime prepares each statement on every call (the others prepare once),
   JSON goes through the `Schema` codec, routes match through `UrlPattern`, and every request
   runs as a task in Sakura's effect runtime.
3. **Native is Sakura's fastest and leanest backend here.** On the read mix it served 9,877 RPS:
   more than Sakura on Node (7,279) or Bun (7,881), and below hand-written Node (13,135). On
   `/health` it served 36,205. Its peak RSS was 46–64 MiB, the lowest of all eight servers.
4. **WASM is the slowest backend here:** 3,982 RPS on the read mix and 9,439 on `/health`, about
   half of Sakura on Node.
5. **`HEAD` on a `GET` route answers `405`** on all four backends, with `std/router`'s
   `Router.get`. Go's `ServeMux` and `Bun.serve`'s `routes` answer `HEAD` for a `GET` route, as
   HTTP (RFC 9110 §9.3.2) expects. Recorded in `compiler/REQUESTS.md`.

Other limits checked by hand (`curl` against each Sakura backend on the Mac), all as documented: a
1.05 MB body answers `413`; a malformed request line answers `400`; an unknown method on a known
path answers `405`; a 20-digit id answers `404`; the server keeps serving after each of them.

## Caveats

- One app, one machine per run, loopback only: no TLS, no real network, no file database.
- The server and load generator share a machine. On the runner they are pinned to separate
  cores, but they still share caches and memory bandwidth. On the Mac nothing is pinned.
- Hosted runners are shared virtual machines. Treat one run as a trend, not a baseline.
- `go-mt` on the runner had one physical core (two hardware threads), and every query went
  through its one SQLite connection. It shows little of what Go does with more cores and a file
  database.
- Each point is 3 runs of 5 s. Longer runs could show slow leaks or garbage-collection cycles
  that these miss.
- The `unsent` errors are an artefact of a fixed-length run past the knee: requests still queued
  in the generator when the run ends.
