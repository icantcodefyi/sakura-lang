// DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE.
// Arena indices represent the JS class graph without changing traversal/queue order.
package ports

import "sort"

const (
	dbEdit byte = iota
	dbStay
	dbEquality
	dbScale
)

type dbVariable struct {
	value       int64
	constraints []int
	determined  int
	walk        int64
	stay        bool
	mark        int64
}
type dbConstraint struct {
	kind                  byte
	strength              int64
	v1, v2, scale, offset int
	dir                   int8
	satisfied             bool
}

func (c dbConstraint) unary() bool { return c.kind == dbEdit || c.kind == dbStay }
func (c dbConstraint) isSatisfied() bool {
	if c.unary() {
		return c.satisfied
	}
	return c.dir != 0
}
func (c dbConstraint) output() int {
	if c.unary() || c.dir != 1 {
		return c.v1
	}
	return c.v2
}
func (c dbConstraint) inputs() []int {
	if c.unary() {
		return nil
	}
	v := c.v2
	if c.dir == 1 {
		v = c.v1
	}
	if c.kind == dbScale {
		return []int{v, c.scale, c.offset}
	}
	return []int{v}
}
func (c dbConstraint) attached() []int {
	if c.unary() {
		return []int{c.v1}
	}
	if c.kind == dbScale {
		return []int{c.v1, c.v2, c.scale, c.offset}
	}
	return []int{c.v1, c.v2}
}
func (c *dbConstraint) unsatisfy() { c.dir = 0; c.satisfied = false }

type dbPlanner struct {
	vars []dbVariable
	cons []dbConstraint
	mark int64
}

func dbNew() *dbPlanner { return &dbPlanner{mark: 1} }
func (p *dbPlanner) variable(value int64) int {
	id := len(p.vars)
	p.vars = append(p.vars, dbVariable{value: value, determined: -1, walk: 10000, stay: true})
	return id
}
func (p *dbPlanner) newMark() int64 { p.mark++; return p.mark }
func (p *dbPlanner) add(kind byte, v1, v2 int, strength int64, scale, offset int) int {
	id := len(p.cons)
	c := dbConstraint{kind: kind, strength: strength, v1: v1, v2: v2, scale: scale, offset: offset}
	p.cons = append(p.cons, c)
	for _, v := range c.attached() {
		p.vars[v].constraints = append(p.vars[v].constraints, id)
	}
	p.incrementalAdd(id)
	return id
}
func (p *dbPlanner) removeGraph(id int) {
	for _, v := range p.cons[id].attached() {
		next := []int{}
		for _, c := range p.vars[v].constraints {
			if c != id {
				next = append(next, c)
			}
		}
		p.vars[v].constraints = next
		if p.vars[v].determined == id {
			p.vars[v].determined = -1
		}
	}
	p.cons[id].unsatisfy()
}
func (p *dbPlanner) choose(id int, mark int64) {
	c := p.cons[id]
	v1, v2 := p.vars[c.v1], p.vars[c.v2]
	if c.unary() {
		p.cons[id].satisfied = v1.mark != mark && c.strength < v1.walk
		return
	}
	var dir int8
	if v1.mark == mark {
		if v2.mark != mark && c.strength < v2.walk {
			dir = 1
		}
	} else if v2.mark == mark {
		if v1.mark != mark && c.strength < v1.walk {
			dir = -1
		}
	} else if v1.walk > v2.walk {
		if c.strength < v1.walk {
			dir = -1
		}
	} else if c.strength < v2.walk {
		dir = 1
	}
	p.cons[id].dir = dir
}
func (p *dbPlanner) execute(id int) {
	c := p.cons[id]
	if c.unary() {
		return
	}
	input := c.v2
	if c.dir == 1 {
		input = c.v1
	}
	value := p.vars[input].value
	if c.kind == dbScale {
		if c.dir == 1 {
			value = value*p.vars[c.scale].value + p.vars[c.offset].value
		} else {
			value = (value - p.vars[c.offset].value) / p.vars[c.scale].value
		}
	}
	p.vars[c.output()].value = value
}
func (p *dbPlanner) recalculate(id int) {
	c := p.cons[id]
	out := c.output()
	walk := c.strength
	stay := c.kind != dbEdit
	if !c.unary() {
		input := c.v2
		if c.dir == 1 {
			input = c.v1
		}
		v := p.vars[input]
		walk = max(c.strength, v.walk)
		stay = v.stay
		if c.kind == dbScale {
			stay = stay && p.vars[c.scale].stay && p.vars[c.offset].stay
		}
	}
	p.vars[out].walk = walk
	p.vars[out].stay = stay
	if stay {
		p.execute(id)
	}
}
func (p *dbPlanner) consuming(v int) []int {
	todo := []int{}
	for _, c := range p.vars[v].constraints {
		if c != p.vars[v].determined && p.cons[c].isSatisfied() {
			todo = append(todo, c)
		}
	}
	return todo
}
func (p *dbPlanner) satisfy(id int, mark int64) int {
	p.choose(id, mark)
	c := p.cons[id]
	if !c.isSatisfied() {
		dbExpect(c.strength != -800, "Could not satisfy a required constraint")
		return -1
	}
	for _, v := range c.inputs() {
		p.vars[v].mark = mark
	}
	out := c.output()
	overridden := p.vars[out].determined
	if overridden >= 0 {
		p.cons[overridden].unsatisfy()
	}
	p.vars[out].determined = id
	todo := []int{id}
	for head := 0; head < len(todo); head++ {
		d := todo[head]
		output := p.cons[d].output()
		if p.vars[output].mark == mark {
			p.incrementalRemove(id)
			panic("Cycle encountered")
		}
		p.recalculate(d)
		todo = append(todo, p.consuming(output)...)
	}
	p.vars[out].mark = mark
	return overridden
}
func (p *dbPlanner) incrementalAdd(id int) {
	mark := p.newMark()
	overridden := p.satisfy(id, mark)
	for overridden >= 0 {
		overridden = p.satisfy(overridden, mark)
	}
}
func (p *dbPlanner) incrementalRemove(id int) {
	out := p.cons[id].output()
	p.cons[id].unsatisfy()
	p.removeGraph(id)
	p.vars[out].determined = -1
	p.vars[out].walk = 10000
	p.vars[out].stay = true
	unsatisfied := []int{}
	todo := []int{out}
	for head := 0; head < len(todo); head++ {
		v := todo[head]
		for _, c := range p.vars[v].constraints {
			if !p.cons[c].isSatisfied() {
				unsatisfied = append(unsatisfied, c)
			}
		}
		for _, c := range p.consuming(v) {
			p.recalculate(c)
			todo = append(todo, p.cons[c].output())
		}
	}
	sort.SliceStable(unsatisfied, func(i, j int) bool { return p.cons[unsatisfied[i]].strength < p.cons[unsatisfied[j]].strength })
	for _, c := range unsatisfied {
		p.incrementalAdd(c)
	}
}
func (p *dbPlanner) destroy(id int) {
	if p.cons[id].isSatisfied() {
		p.incrementalRemove(id)
	}
	p.removeGraph(id)
}
func (p *dbPlanner) plan(constraints []int) []int {
	todo := []int{}
	for _, c := range constraints {
		if p.cons[c].kind == dbEdit && p.cons[c].isSatisfied() {
			todo = append(todo, c)
		}
	}
	mark := p.newMark()
	plan := []int{}
	for head := 0; head < len(todo); head++ {
		id := todo[head]
		c := p.cons[id]
		out := c.output()
		input := c.v2
		if c.dir == 1 {
			input = c.v1
		}
		v := p.vars[input]
		known := c.unary() || v.mark == mark || v.stay || v.determined < 0
		if p.vars[out].mark != mark && known {
			plan = append(plan, id)
			p.vars[out].mark = mark
			todo = append(todo, p.consuming(out)...)
		}
	}
	return plan
}
func (p *dbPlanner) executePlan(plan []int) {
	for _, c := range plan {
		p.execute(c)
	}
}
func (p *dbPlanner) change(v int, value int64) {
	edit := p.add(dbEdit, v, v, -400, 0, 0)
	plan := p.plan([]int{edit})
	for i := 0; i < 10; i++ {
		p.vars[v].value = value
		p.executePlan(plan)
	}
	p.destroy(edit)
}
func dbExpect(ok bool, message string) {
	if !ok {
		panic(message)
	}
}
func Deltablue(size int64) int64 {
	n := int(size)
	p := dbNew()
	vars := make([]int, n+1)
	for i := range vars {
		vars[i] = p.variable(0)
	}
	for i := 0; i < n; i++ {
		p.add(dbEquality, vars[i], vars[i+1], -800, 0, 0)
	}
	p.add(dbStay, vars[n], vars[n], -200, 0, 0)
	edit := p.add(dbEdit, vars[0], vars[0], -400, 0, 0)
	plan := p.plan([]int{edit})
	var chain int64
	for i := int64(0); i < 100; i++ {
		p.vars[vars[0]].value = i
		p.executePlan(plan)
		dbExpect(p.vars[vars[n]].value == i, "Chain test failed")
		chain += p.vars[vars[n]].value
	}
	p.destroy(edit)
	chain += int64(len(plan))
	if n == 0 {
		return chain
	}
	p = dbNew()
	scale, offset := p.variable(10), p.variable(1000)
	dests := []int{}
	src, dst := 0, 0
	for i := 1; i <= n; i++ {
		src = p.variable(int64(i))
		dst = p.variable(int64(i))
		dests = append(dests, dst)
		p.add(dbStay, src, src, 0, 0, 0)
		p.add(dbScale, src, dst, -800, scale, offset)
	}
	p.change(src, 17)
	dbExpect(p.vars[dst].value == 1170, "Projection test 1 failed")
	p.change(dst, 1050)
	dbExpect(p.vars[src].value == 5, "Projection test 2 failed")
	p.change(scale, 5)
	for i := 0; i < n-1; i++ {
		dbExpect(p.vars[dests[i]].value == int64(i+1)*5+1000, "Projection test 3 failed")
	}
	var first int64
	for _, d := range dests {
		first += p.vars[d].value
	}
	p.change(offset, 2000)
	for i := 0; i < n-1; i++ {
		dbExpect(p.vars[dests[i]].value == int64(i+1)*5+2000, "Projection test 4 failed")
	}
	var last int64
	for _, d := range dests {
		last += p.vars[d].value
	}
	return chain + 1170 + 5 + first + last
}
