// Package ports implements the matched Sakura corpus and the JSON sample protocol.
package ports

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
)

var sink int64

func Main(run func(int64) int64, maximum int64) {
	if len(os.Args) < 3 {
		panic("values <n>... or measure <n> [--stable]")
	}
	mode := os.Args[1]
	stable := false
	sizes := []int64{}
	for _, arg := range os.Args[2:] {
		if arg == "--stable" {
			stable = true
			continue
		}
		n, err := strconv.ParseInt(arg, 10, 64)
		if err != nil || n < 0 || n > maximum {
			panic("size outside oracle resource bound")
		}
		sizes = append(sizes, n)
	}
	enc := json.NewEncoder(os.Stdout)
	if mode == "values" {
		values := map[string]string{}
		for _, n := range sizes {
			values[strconv.FormatInt(n, 10)] = strconv.FormatInt(run(n), 10)
		}
		if err := enc.Encode(map[string]any{"values": values}); err != nil {
			panic(err)
		}
		return
	}
	if mode != "measure" || len(sizes) != 1 {
		panic("measure <n> [--stable]")
	}
	n := sizes[0]
	warmup := 250 * time.Millisecond
	if stable {
		warmup = 2 * time.Second
	}
	start := time.Now()
	for time.Since(start) < warmup {
		sink = run(n)
	}
	count := 1
	for {
		start = time.Now()
		for i := 0; i < count; i++ {
			sink = run(n)
		}
		if time.Since(start) >= 30*time.Millisecond || count >= 1<<20 {
			break
		}
		count *= 2
	}
	for sample := 0; sample < 9; sample++ {
		start = time.Now()
		for i := 0; i < count; i++ {
			sink = run(n)
		}
		ms := float64(time.Since(start).Nanoseconds()) / 1e6 / float64(count)
		if err := enc.Encode(map[string]any{"n": n, "ms": ms, "checksum": strconv.FormatInt(sink, 10)}); err != nil {
			panic(err)
		}
	}
}
func Fib(n int64) int64 {
	if n < 2 {
		return n
	}
	return Fib(n-1) + Fib(n-2)
}
func Countdown(n int64) int64 {
	var acc int64
	for i := n; i > 0; i-- {
		acc += i
	}
	even := true
	for i := n; i > 0; i-- {
		even = !even
	}
	if even {
		acc++
	}
	return acc
}
func Sieve(n int64) int64 {
	flags := make([]bool, n)
	for i := range flags {
		flags[i] = i >= 2
	}
	for p := int64(2); p*p < n; p++ {
		if flags[p] {
			next := make([]bool, len(flags))
			for j, f := range flags {
				next[j] = f && (int64(j) < p*p || int64(j)%p != 0)
			}
			flags = next
		}
	}
	var count int64
	for _, f := range flags {
		if f {
			count++
		}
	}
	return count
}
func Pipeline(n int64) int64 {
	xs := make([]int64, n)
	for i := range xs {
		xs[i] = int64(i)
	}
	ys := make([]int64, n)
	for i, x := range xs {
		ys[i] = x * 7 % 1000
	}
	zs := []int64{}
	for _, y := range ys {
		if y%2 == 0 {
			zs = append(zs, y)
		}
	}
	var total int64
	for _, z := range zs {
		total += z
	}
	return total
}

type binary struct{ l, r *binary }

func bottomUp(d int64) *binary {
	if d > 0 {
		return &binary{bottomUp(d - 1), bottomUp(d - 1)}
	}
	return &binary{}
}
func check(t *binary) int64 {
	if t == nil {
		return 0
	}
	return 1 + check(t.l) + check(t.r)
}
func BinaryTrees(n int64) int64 {
	maximum := max(int64(6), n)
	total := check(bottomUp(maximum + 1))
	long := bottomUp(maximum)
	for d := int64(4); d <= maximum; d += 2 {
		iterations := int64(1) << (maximum - d + 4)
		for i := int64(0); i < iterations; i++ {
			total += check(bottomUp(d))
		}
	}
	return total + check(long)
}

type tree struct {
	red bool
	l   *tree
	k   int64
	r   *tree
}

func node(c bool, l *tree, k int64, r *tree) *tree { return &tree{c, l, k, r} }
func red(t *tree) bool                             { return t != nil && t.red }
func balance(c bool, l *tree, k int64, r *tree) *tree {
	if !c {
		if red(l) && red(l.l) {
			a := l.l
			return node(true, node(false, a.l, a.k, a.r), l.k, node(false, l.r, k, r))
		}
		if red(l) && red(l.r) {
			a := l.r
			return node(true, node(false, l.l, l.k, a.l), a.k, node(false, a.r, k, r))
		}
		if red(r) && red(r.l) {
			a := r.l
			return node(true, node(false, l, k, a.l), a.k, node(false, a.r, r.k, r.r))
		}
		if red(r) && red(r.r) {
			a := r.r
			return node(true, node(false, l, k, r.l), r.k, node(false, a.l, a.k, a.r))
		}
	}
	return node(c, l, k, r)
}
func ins(t *tree, k int64) *tree {
	if t == nil {
		return node(true, nil, k, nil)
	}
	if k < t.k {
		return balance(t.red, ins(t.l, k), t.k, t.r)
	}
	if k > t.k {
		return balance(t.red, t.l, t.k, ins(t.r, k))
	}
	return t
}
func member(t *tree, k int64) bool {
	for t != nil {
		if k < t.k {
			t = t.l
		} else if k > t.k {
			t = t.r
		} else {
			return true
		}
	}
	return false
}
func size(t *tree) int64 {
	if t == nil {
		return 0
	}
	return size(t.l) + 1 + size(t.r)
}
func height(t *tree) int64 {
	if t == nil {
		return 0
	}
	h := height(t.l)
	if !t.red {
		h++
	}
	return h
}
func Rbtree(n int64) int64 {
	var t *tree
	kept := []*tree{}
	for i := int64(0); i < n; i++ {
		s := ins(t, i*7919%(2*n+1))
		t = node(false, s.l, s.k, s.r)
		if i%100 == 0 {
			kept = append(kept, t)
		}
	}
	total := size(t)
	for j, v := range kept {
		total += height(v)
		if member(v, int64(j)*3) {
			total++
		}
	}
	return total
}
func MapSet(n int64) int64 {
	mi := map[int64]int64{}
	ms := map[string]int64{}
	mt := map[[2]int64]int64{}
	set := map[int64]bool{}
	for i := int64(0); i < n; i++ {
		mi[i] = i % 1000
		ms[fmt.Sprintf("k%d", i)] = i % 1000
		mt[[2]int64{i % 100, i / 100}] = i % 7
		set[i%97] = true
	}
	snapshot := map[int64]int64{}
	for k, v := range mi {
		snapshot[k] = v
	}
	var found int64
	for i := int64(0); i < 2*n; i++ {
		found += mi[i]
		found += ms[fmt.Sprintf("k%d", i)]
		found += mt[[2]int64{i % 100, i / 100}]
	}
	for i := int64(0); i < n; i += 2 {
		mi[i] += 7
		ms[fmt.Sprintf("k%d", i)] += 7
	}
	for i := int64(0); i < n; i += 3 {
		delete(mi, i)
		delete(ms, fmt.Sprintf("k%d", i))
		delete(mt, [2]int64{i % 100, i / 100})
	}
	// Restore insertion order explicitly; Go's map iteration order is unspecified.
	var total, snap int64
	for i := int64(0); i < n; i++ {
		total += mi[i]
	}
	for i := int64(0); i < n; i++ {
		total += ms[fmt.Sprintf("k%d", i)]
	}
	for i := int64(0); i < n; i++ {
		total += mt[[2]int64{i % 100, i / 100}]
	}
	for i := int64(0); i < n; i++ {
		snap += snapshot[i]
	}
	return found + total + snap + int64(len(mi)+len(ms)+len(mt)+len(snapshot)+len(set))
}

type shape struct {
	kind int
	a, b int64
}
type item struct {
	id    int64
	shape shape
	tag   *int64
}

func some(x int64) *int64 { return &x }
func area(s shape) int64 {
	switch s.kind {
	case 0:
		return 3 * s.a * s.a
	case 1:
		return s.a * s.b
	default:
		return s.a * s.a
	}
}
func grow(it item) item {
	s := it.shape
	var next shape
	switch s.kind {
	case 0:
		next = shape{1, s.a, s.a + 1}
	case 1:
		next = shape{2, (s.a + s.b) % 100, 0}
	default:
		next = shape{0, s.a%50 + 1, 0}
	}
	var tag *int64
	if it.tag == nil {
		tag = some(0)
	} else if *it.tag <= 5 {
		tag = some(*it.tag + 1)
	}
	return item{it.id, next, tag}
}
func Shapes(n int64) int64 {
	seed := int64(42)
	items := []item{}
	for i := int64(0); i < n; i++ {
		seed = seed * 48271 % 2147483647
		k := seed % 100
		var s shape
		if k < 33 {
			s = shape{0, k, 0}
		} else if k < 66 {
			s = shape{1, k, k%7 + 1}
		} else {
			s = shape{2, k % 13, 0}
		}
		var tag *int64
		if k%4 != 0 {
			tag = some(k % 9)
		}
		items = append(items, item{i, s, tag})
	}
	var total int64
	for r := 0; r < 5; r++ {
		next := make([]item, len(items))
		for i, it := range items {
			next[i] = grow(it)
		}
		items = next
		for _, it := range items {
			tag := int64(0)
			if it.tag != nil {
				tag = *it.tag
			}
			total += area(it.shape) + tag + it.id%3
		}
	}
	return total
}
func Tokenizer(n int64) int64 {
	words := []string{"alpha", "naïve", "日本語", "🌸", "x1", "Grüße", "ok", "mañana"}
	s := ""
	for i := int64(0); i < n; i++ {
		end := " "
		if i%5 == 4 {
			end = ", "
		}
		s = s + words[i%8] + end
	}
	var tokens, bytes, scalars, commas int64
	for _, t := range strings.Split(s, " ") {
		if t != "" {
			tokens++
			bytes += int64(len(t))
			scalars += int64(utf8.RuneCountInString(t))
			if strings.HasSuffix(t, ",") {
				commas++
			}
		}
	}
	return tokens + 3*bytes + 7*scalars + 11*commas
}
func JsonValidate(n int64) int64 {
	lines := []string{}
	for i := int64(0); i < n; i++ {
		name := ""
		if i%17 != 0 {
			name = fmt.Sprintf("user%d", i)
		}
		age := i % 90
		if i%13 == 0 {
			age = 200
		}
		lines = append(lines, fmt.Sprintf("{\"id\": %d, \"name\": \"%s\", \"age\": %d}", i, name, age))
	}
	text := strings.Join(lines, "\n")
	var valid, bytes, ids, invalid int64
	for _, line := range strings.Split(text, "\n") {
		if line == "" {
			continue
		}
		var value map[string]any
		if err := json.Unmarshal([]byte(line), &value); err != nil {
			panic(err)
		}
		id, okid := value["id"].(float64)
		name, okname := value["name"].(string)
		age, okage := value["age"].(float64)
		if !okid || math.Trunc(id) != id || math.Abs(id) > 9007199254740991 || !okname || len(name) == 0 || len(name) > 32 || !okage || math.Trunc(age) != age || age < 0 || age > 150 {
			invalid++
			continue
		}
		valid++
		user := struct {
			ID   int64  `json:"id"`
			Name string `json:"name"`
			Age  int64  `json:"age"`
		}{int64(id), name, int64(age)}
		encoded, err := json.Marshal(user)
		if err != nil {
			panic(err)
		}
		bytes += int64(len(encoded))
		ids += int64(id)
	}
	return valid + bytes + ids + 1000*invalid
}
func a(i, j int) float64 { return 1 / (float64((i+j)*(i+j+1))/2 + float64(i) + 1) }
func av(v []float64, transpose bool) []float64 {
	out := make([]float64, len(v))
	for i := range out {
		sum := 0.0
		for j, x := range v {
			entry := a(i, j)
			if transpose {
				entry = a(j, i)
			}
			sum += float64(entry * x)
		}
		out[i] = sum
	}
	return out
}
func atav(v []float64) []float64 { return av(av(v, false), true) }
func SpectralNorm(n int64) int64 {
	if n == 0 {
		return 0
	}
	u := make([]float64, n)
	for i := range u {
		u[i] = 1
	}
	v := u
	for i := 0; i < 10; i++ {
		v = atav(u)
		u = atav(v)
	}
	vbv, vv := 0.0, 0.0
	for i := range u {
		vbv += float64(u[i] * v[i])
		vv += float64(v[i] * v[i])
	}
	return int64(math.Round(math.Sqrt(vbv/vv) * 1e9))
}

type body [7]float64

func planet(x, y, z, vx, vy, vz, m float64) body {
	pi := float64(math.Pi)
	solar := float64(4*pi) * pi
	return body{x, y, z, vx * 365.24, vy * 365.24, vz * 365.24, m * solar}
}
func Nbody(n int64) int64 {
	pi := float64(math.Pi)
	solar := float64(4*pi) * pi
	bs := []body{{0, 0, 0, 0, 0, 0, solar},
		planet(4.8414314424647209, -1.16032004402742839, -1.03622044471123109e-1, 1.66007664274403694e-3, 7.69901118419740425e-3, -6.90460016972063023e-5, 9.54791938424326609e-4),
		planet(8.34336671824457987, 4.12479856412430479, -4.03523417114321381e-1, -2.76742510726862411e-3, 4.99852801234917238e-3, 2.30417297573763929e-5, 2.85885980666130812e-4),
		planet(1.2894369562139131e1, -1.51111514016986312e1, -2.23307578892655734e-1, 2.96460137564761618e-3, 2.3784717395948095e-3, -2.96589568540237556e-5, 4.36624404335156298e-5),
		planet(1.53796971148509165e1, -2.59193146099879641e1, 1.79258772950371181e-1, 2.68067772490389322e-3, 1.62824170038242295e-3, -9.5159225451971587e-5, 5.15138902046611451e-5)}
	px, py, pz := 0.0, 0.0, 0.0
	for _, b := range bs {
		px += float64(b[3] * b[6])
		py += float64(b[4] * b[6])
		pz += float64(b[5] * b[6])
	}
	bs[0][3] = 0 - px/solar
	bs[0][4] = 0 - py/solar
	bs[0][5] = 0 - pz/solar
	for step := int64(0); step < n; step++ {
		for i := range bs {
			b := &bs[i]
			for j := i + 1; j < len(bs); j++ {
				c := &bs[j]
				dx, dy, dz := b[0]-c[0], b[1]-c[1], b[2]-c[2]
				d2 := float64(dx*dx) + float64(dy*dy) + float64(dz*dz)
				mag := .01 / (d2 * math.Sqrt(d2))
				b[3] -= float64(dx * c[6] * mag)
				b[4] -= float64(dy * c[6] * mag)
				b[5] -= float64(dz * c[6] * mag)
				c[3] += float64(dx * b[6] * mag)
				c[4] += float64(dy * b[6] * mag)
				c[5] += float64(dz * b[6] * mag)
			}
		}
		for i := range bs {
			b := &bs[i]
			b[0] += float64(.01 * b[3])
			b[1] += float64(.01 * b[4])
			b[2] += float64(.01 * b[5])
		}
	}
	energy := 0.0
	for i, b := range bs {
		energy += float64(.5 * b[6] * (float64(b[3]*b[3]) + float64(b[4]*b[4]) + float64(b[5]*b[5])))
		for _, c := range bs[i+1:] {
			dx, dy, dz := b[0]-c[0], b[1]-c[1], b[2]-c[2]
			energy -= float64(b[6] * c[6] / math.Sqrt(float64(dx*dx)+float64(dy*dy)+float64(dz*dz)))
		}
	}
	return int64(math.Round(energy * 1e9))
}
func contractRun(n int64, checked bool) int64 {
	xs := make([]int64, 16)
	for i := range xs {
		xs[i] = int64(i)
	}
	var i, acc int64
	for i < n {
		if checked && i < 0 {
			panic("D020")
		}
		next := i + 1
		if checked && next <= i {
			panic("D021")
		}
		i = next
		if i%16 == 0 {
			if checked {
				for _, x := range xs {
					if x < 0 {
						panic("D020")
					}
				}
			}
			var total int64
			for _, x := range xs {
				total += x
			}
			acc += total
		}
	}
	return acc + i
}
func Contracts(n int64) int64      { return contractRun(n, true) }
func ContractsPlain(n int64) int64 { return contractRun(n, false) }
func counter() func() int64        { var value int64; return func() int64 { value++; return value } }
func HandledD1(n int64) int64 {
	tick := counter()
	var acc int64
	for i := int64(0); i < n; i++ {
		acc += tick() % 7
	}
	return acc
}
func HandledD4(n int64) int64  { return HandledD1(n) }
func HandledD16(n int64) int64 { return HandledD1(n) }
func HandledFwd(n int64) int64 {
	outer := counter()
	tick := func() int64 { return outer() }
	var acc int64
	for i := int64(0); i < n; i++ {
		acc += tick() % 7
	}
	return acc
}
func fold(xs []int64, f func(int64, int64) int64) int64 {
	var acc int64
	for _, x := range xs {
		acc = f(acc, x)
	}
	return acc
}
func HandledReentry(n int64) int64 {
	tick := counter()
	xs := make([]int64, n)
	for i := range xs {
		xs[i] = int64(i)
	}
	return fold(xs, func(acc, _ int64) int64 { return acc + tick()%7 })
}
func HandledSuspend(n int64) int64 {
	// A single coroutine is parked and resumed on the calling thread (no goroutine).
	tick := counter()
	var acc, elapsed, i int64
	suspended := false
	resume := func() bool {
		for i < n {
			acc += tick() % 7
			at := i
			i++
			if at == n/2 && !suspended {
				suspended = true
				return false
			}
		}
		return true
	}
	if !resume() {
		elapsed++
		if !resume() {
			panic("multiple suspensions")
		}
	}
	return acc + elapsed
}
