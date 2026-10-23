// loadgen drives bench/http's social app with a request mix and prints one JSON result.
// Go standard library only. AI-assisted (Claude).
//
// Two modes:
//
//   - closed loop (-c N): N connections, each sends its next request when the last answer
//     arrives. Throughput is what the server sustains at that concurrency.
//   - open loop (-rate R): requests are scheduled at R per second whatever the server does,
//     over at most -conns connections. Latency is measured from each request's scheduled
//     start, so time spent queued behind a slow server counts (no coordinated omission).
//     Requests still unsent when the run ends count as errors of kind "unsent".
//
// Every connection is opened, one at a time, before the measured window starts (as wrk does),
// so a burst of simultaneous connects does not overflow the server's accept queue; a run
// measures requests, not connection setup. A connection the server closes is reopened when it
// is next needed, and that reconnect is inside the window.
//
// Every answer is read to its end. Answers count by status code; transport failures by kind.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"math/rand/v2"
	"net"
	"net/url"
	"os"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
)

type request struct {
	method, path string
	body         []byte
}

// mixes: name → weighted request makers. Ids are drawn from the seeded ranges.
type maker struct {
	weight int
	make   func(r *rand.Rand, users, posts int, n uint64) request
}

func get(path func(r *rand.Rand, users, posts int) string) func(*rand.Rand, int, int, uint64) request {
	return func(r *rand.Rand, users, posts int, _ uint64) request {
		return request{"GET", path(r, users, posts), nil}
	}
}

var (
	feed    = get(func(*rand.Rand, int, int) string { return "/feed?limit=20" })
	onePost = get(func(r *rand.Rand, _, posts int) string { return fmt.Sprintf("/posts/%d", 1+r.IntN(posts)) })
	health  = get(func(*rand.Rand, int, int) string { return "/health" })
)

func like(r *rand.Rand, users, posts int, _ uint64) request {
	return request{"POST", fmt.Sprintf("/posts/%d/like", 1+r.IntN(posts)),
		fmt.Appendf(nil, `{"user_id":%d}`, 1+r.IntN(users))}
}

func newPost(r *rand.Rand, users, _ int, n uint64) request {
	return request{"POST", "/posts", fmt.Appendf(nil, `{"user_id":%d,"body":"load test post %d"}`, 1+r.IntN(users), n)}
}

func newUser(_ *rand.Rand, _, _ int, n uint64) request {
	return request{"POST", "/users", fmt.Appendf(nil, `{"name":"load user %d"}`, n)}
}

var mixes = map[string][]maker{
	// 80% reads, 20% writes.
	"read": {{40, feed}, {40, onePost}, {10, like}, {7, newPost}, {3, newUser}},
	// 20% reads, 80% writes.
	"write":  {{10, feed}, {10, onePost}, {50, like}, {25, newPost}, {5, newUser}},
	"health": {{100, health}},
}

func pick(ms []maker, r *rand.Rand) maker {
	total := 0
	for _, m := range ms {
		total += m.weight
	}
	x := r.IntN(total)
	for _, m := range ms {
		if x < m.weight {
			return m
		}
		x -= m.weight
	}
	return ms[len(ms)-1]
}

// worker results, merged at the end.
type tally struct {
	latUS    []uint32 // completed answers of any status, microseconds
	status   map[int]int
	errs     map[string]int
	perSec   []int // 2xx answers completed in each second of the run
	sent     int
	finished int
	inWindow int // 2xx answers completed before the run's end
}

func newTally(secs int) *tally {
	return &tally{status: map[int]int{}, errs: map[string]int{}, perSec: make([]int, secs+1)}
}

// errKind names a transport failure; a failure to connect is "connect_" and its kind.
func errKind(err error) string {
	if errors.As(err, new(dialError)) {
		return "connect_" + baseKind(err)
	}
	return baseKind(err)
}

func baseKind(err error) string {
	var ne net.Error
	switch {
	case errors.Is(err, context.DeadlineExceeded), errors.As(err, &ne) && ne.Timeout():
		return "timeout"
	case errors.Is(err, syscall.ECONNREFUSED):
		return "refused"
	case errors.Is(err, syscall.ECONNRESET), errors.Is(err, syscall.EPIPE), errors.Is(err, io.EOF),
		errors.Is(err, io.ErrUnexpectedEOF), strings.Contains(err.Error(), "connection reset"):
		return "reset"
	case strings.Contains(err.Error(), "too many open files"):
		return "fd_limit"
	case strings.Contains(err.Error(), "can't assign requested address"),
		strings.Contains(err.Error(), "cannot assign requested address"):
		return "ports"
	}
	return "other"
}

func main() {
	base := flag.String("url", "http://127.0.0.1:8080", "server base URL")
	mixName := flag.String("mix", "read", "read, write or health")
	conc := flag.Int("c", 0, "closed loop: connections")
	rate := flag.Float64("rate", 0, "open loop: requests per second")
	conns := flag.Int("conns", 256, "open loop: maximum connections")
	dur := flag.Duration("d", 5*time.Second, "measured duration")
	timeout := flag.Duration("timeout", 5*time.Second, "per-request timeout")
	users := flag.Int("users", 1000, "seeded users (ids 1..users)")
	posts := flag.Int("posts", 10000, "seeded posts (ids 1..posts)")
	seed := flag.Uint64("seed", 20261023, "random seed")
	flag.Parse()

	ms, ok := mixes[*mixName]
	if !ok || (*conc > 0) == (*rate > 0) {
		fmt.Fprintln(os.Stderr, "usage: loadgen -mix read|write|health (-c N | -rate R) [flags]")
		os.Exit(2)
	}
	workers := *conc
	if *rate > 0 {
		workers = *conns
	}
	u, err := url.Parse(*base)
	if err != nil || u.Scheme != "http" || u.Port() == "" {
		fmt.Fprintln(os.Stderr, "loadgen: -url must be http://host:port")
		os.Exit(2)
	}
	secs := int(dur.Seconds())

	var n atomic.Uint64

	opened := make([]*conn, workers)
	setupFailures := 0
	for w := range opened {
		k := &conn{addr: u.Host, host: u.Host, timeout: *timeout}
		opened[w] = k
		for try := 0; ; try++ {
			if k.dial() == nil {
				break
			}
			if try == 50 {
				setupFailures++ // the worker dials again on its first request
				break
			}
			time.Sleep(20 * time.Millisecond)
		}
	}

	start := time.Now()
	end := start.Add(*dur)
	// Open loop: a schedule of intended start times, buffered so the scheduler never waits.
	var ticks chan time.Time
	if *rate > 0 {
		total := int(*rate * dur.Seconds())
		ticks = make(chan time.Time, total+1)
		go func() {
			interval := time.Duration(float64(time.Second) / *rate)
			for i := 0; i < total; i++ {
				at := start.Add(time.Duration(i) * interval)
				if d := time.Until(at); d > 0 {
					time.Sleep(d)
				}
				ticks <- at
			}
			close(ticks)
		}()
	}

	tallies := make([]*tally, workers)
	var wg sync.WaitGroup
	for w := range workers {
		t := newTally(secs)
		tallies[w] = t
		r := rand.New(rand.NewPCG(*seed, uint64(w)))
		k := opened[w]
		wg.Add(1)
		go func() {
			defer wg.Done()
			defer k.close()
			for {
				var at time.Time
				if ticks != nil {
					if time.Now().After(end) {
						return
					}
					var ok bool
					select {
					case at, ok = <-ticks:
					case <-time.After(time.Until(end)):
						return
					}
					if !ok || at.After(end) {
						return
					}
				} else {
					at = time.Now()
					if at.After(end) {
						return
					}
				}
				q := pick(ms, r).make(r, *users, *posts, n.Add(1))
				t.sent++
				code, err := k.do(q)
				done := time.Now()
				t.finished++
				if err != nil {
					t.errs[errKind(err)]++
					continue
				}
				t.status[code]++
				t.latUS = append(t.latUS, uint32(min(done.Sub(at).Microseconds(), 1<<32-1)))
				if code < 300 && !done.After(end) {
					t.inWindow++
				}
				if code < 300 {
					if s := int(done.Sub(start).Seconds()); s >= 0 && s < len(t.perSec) {
						t.perSec[s]++
					}
				}
			}
		}()
	}
	wg.Wait()
	elapsed := time.Since(start)

	all := newTally(secs)
	for _, t := range tallies {
		all.latUS = append(all.latUS, t.latUS...)
		for k, v := range t.status {
			all.status[k] += v
		}
		for k, v := range t.errs {
			all.errs[k] += v
		}
		for i, v := range t.perSec {
			all.perSec[i] += v
		}
		all.inWindow += t.inWindow
		all.sent += t.sent
		all.finished += t.finished
	}
	scheduled := all.sent
	if ticks != nil {
		unsent := 0
		for range ticks {
			unsent++
		}
		if unsent > 0 {
			all.errs["unsent"] += unsent
		}
		scheduled = int(*rate * dur.Seconds())
	}
	slices.Sort(all.latUS)
	pct := func(p float64) float64 {
		if len(all.latUS) == 0 {
			return 0
		}
		i := min(int(p*float64(len(all.latUS))), len(all.latUS)-1)
		return float64(all.latUS[i]) / 1000
	}
	ok2xx, non2xx := 0, 0
	for code, v := range all.status {
		if code < 300 {
			ok2xx += v
		} else {
			non2xx += v
		}
	}
	errs := 0
	for _, v := range all.errs {
		errs += v
	}
	attempts := ok2xx + non2xx + errs
	status := map[string]int{}
	for k, v := range all.status {
		status[fmt.Sprint(k)] = v
	}
	out := map[string]any{
		"mode":          map[bool]string{true: "rate", false: "closed"}[*rate > 0],
		"mix":           *mixName,
		"connections":   workers,
		"rate":          *rate,
		"duration_s":    dur.Seconds(),
		"elapsed_s":     elapsed.Seconds(),
		"scheduled":     scheduled,
		"ok":            ok2xx,
		"non_2xx":       non2xx,
		"errors":        errs,
		"error_kinds":   all.errs,
		"status_codes":  status,
		"error_rate":    ratio(non2xx+errs, attempts),
		"rps":           float64(all.inWindow) / dur.Seconds(),
		"ok_per_second": all.perSec[:secs],
		"latency_ms": map[string]float64{
			"p50": pct(0.50), "p90": pct(0.90), "p99": pct(0.99), "p999": pct(0.999),
			"max": pct(1),
		},
	}
	json.NewEncoder(os.Stdout).Encode(out)
}

func ratio(a, b int) float64 {
	if b == 0 {
		return 0
	}
	return float64(a) / float64(b)
}
