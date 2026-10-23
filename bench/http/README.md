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
kernel's queue, not the server. A connection the server closes is reopened inside the window. A first version used Go's `net/http` client and
reached only about 77,000 requests per second on `/health` where `oha` reached about 117,000
against the same server: the generator was the bottleneck. The hand-written client reached
140,000 against `oha`'s 144,000, and 149,000 against `oha`'s 153,000 on Bun, so it has room to
spare on every mix here. Each run also checks it with one `oha` run on `/health` at 128
connections (the last table of each result).

## Running

```sh
python3 bench/http/run.py --json out.json --md out.md      # all servers and mixes (about 100 minutes)
python3 bench/http/run.py --servers sakura-native,go --mixes read --repeats 1 --duration 2
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
