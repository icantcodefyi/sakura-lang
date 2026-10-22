// Havlak loop recognition from Are We Fast Yet (smarr/are-we-fast-yet@74306fec
// benchmarks/JavaScript/havlak.js and Java/src/havlak; Apache-2.0, Copyright 2011 Google Inc.;
// the som collections MIT, Copyright (c) 2015-2016 Stefan Marr; see ../LICENSE). run(n) is
// LoopTesterApp.main(n, 50, 10, 10, 5) as loops*1000000 + nodes.
// Objects are structs linked by pointers, as in AWFY's Java port. som's Vector is a generic
// doubling array with a first index, Set and IdentitySet wrap one, IdentityDictionary is som's
// chained hash table keyed by the block pointer with the block's customHash (its name). forEach
// and hasSome take closures, as in AWFY.
package main

import ports "sakura-bench-ports"

const initialSize = 10
const initialCapacity = 16

// ---- som: Vector

type Vector[E comparable] struct {
	storage  []E
	firstIdx int
	lastIdx  int
}

func NewVector[E comparable]() *Vector[E] { return &Vector[E]{} }

func (v *Vector[E]) At(idx int) E {
	var null E
	if v.storage == nil || idx >= len(v.storage) {
		return null
	}
	return v.storage[idx]
}

func (v *Vector[E]) AtPut(idx int, val E) {
	if v.storage == nil {
		v.storage = make([]E, max(idx+1, initialSize))
	} else if idx >= len(v.storage) {
		newLength := len(v.storage)
		for newLength <= idx {
			newLength *= 2
		}
		storage := make([]E, newLength)
		copy(storage, v.storage)
		v.storage = storage
	}
	v.storage[idx] = val
	if v.lastIdx < idx+1 {
		v.lastIdx = idx + 1
	}
}

func (v *Vector[E]) Append(elem E) {
	if v.storage == nil {
		v.storage = make([]E, initialSize)
	} else if v.lastIdx >= len(v.storage) {
		storage := make([]E, len(v.storage)*2)
		copy(storage, v.storage)
		v.storage = storage
	}
	v.storage[v.lastIdx] = elem
	v.lastIdx++
}

func (v *Vector[E]) IsEmpty() bool { return v.lastIdx == v.firstIdx }

func (v *Vector[E]) ForEach(fn func(E)) {
	for i := v.firstIdx; i < v.lastIdx; i++ {
		fn(v.storage[i])
	}
}

func (v *Vector[E]) HasSome(fn func(E) bool) bool {
	for i := v.firstIdx; i < v.lastIdx; i++ {
		if fn(v.storage[i]) {
			return true
		}
	}
	return false
}

func (v *Vector[E]) RemoveFirst() E {
	var null E
	if v.IsEmpty() {
		return null
	}
	v.firstIdx++
	return v.storage[v.firstIdx-1]
}

func (v *Vector[E]) RemoveAll() {
	v.firstIdx = 0
	v.lastIdx = 0
	if v.storage != nil {
		v.storage = make([]E, len(v.storage))
	}
}

func (v *Vector[E]) Size() int { return v.lastIdx - v.firstIdx }

// ---- som: Set and IdentitySet (Go's == on pointers is identity)

type Set[E comparable] struct{ items *Vector[E] }

func NewSet[E comparable]() *Set[E] { return &Set[E]{NewVector[E]()} }

func (s *Set[E]) Size() int                    { return s.items.Size() }
func (s *Set[E]) ForEach(fn func(E))           { s.items.ForEach(fn) }
func (s *Set[E]) HasSome(fn func(E) bool) bool { return s.items.HasSome(fn) }
func (s *Set[E]) Contains(obj E) bool          { return s.HasSome(func(e E) bool { return e == obj }) }

func (s *Set[E]) Add(obj E) {
	if !s.Contains(obj) {
		s.items.Append(obj)
	}
}

// ---- som: IdentityDictionary from a basic block to an int

type dictEntry struct {
	hash  uint32
	key   *BasicBlock
	value int
	next  *dictEntry
}

func (e *dictEntry) match(hash uint32, key *BasicBlock) bool { return e.hash == hash && e.key == key }

func hashFn(key *BasicBlock) uint32 {
	if key == nil {
		return 0
	}
	hash := key.CustomHash()
	return hash ^ (hash >> 16)
}

type IdentityDictionary struct {
	buckets []*dictEntry
	size    int
}

func NewIdentityDictionary() *IdentityDictionary {
	return &IdentityDictionary{buckets: make([]*dictEntry, initialCapacity)}
}

func (d *IdentityDictionary) bucketIdx(hash uint32) int { return int(uint32(len(d.buckets)-1) & hash) }

// At returns the value for key, or nil.
func (d *IdentityDictionary) At(key *BasicBlock) *int {
	hash := hashFn(key)
	for e := d.buckets[d.bucketIdx(hash)]; e != nil; e = e.next {
		if e.match(hash, key) {
			return &e.value
		}
	}
	return nil
}

func (d *IdentityDictionary) AtPut(key *BasicBlock, value int) {
	hash := hashFn(key)
	i := d.bucketIdx(hash)
	current := d.buckets[i]
	if current == nil {
		d.buckets[i] = &dictEntry{hash, key, value, nil}
		d.size++
	} else {
		d.insertBucketEntry(key, value, hash, current)
	}
	if d.size > len(d.buckets) {
		d.resize()
	}
}

func (d *IdentityDictionary) insertBucketEntry(key *BasicBlock, value int, hash uint32, head *dictEntry) {
	current := head
	for {
		if current.match(hash, key) {
			current.value = value
			return
		}
		if current.next == nil {
			d.size++
			current.next = &dictEntry{hash, key, value, nil}
			return
		}
		current = current.next
	}
}

func (d *IdentityDictionary) resize() {
	oldStorage := d.buckets
	d.buckets = make([]*dictEntry, len(oldStorage)*2)
	d.transferEntries(oldStorage)
}

func (d *IdentityDictionary) transferEntries(oldStorage []*dictEntry) {
	for i, current := range oldStorage {
		if current != nil {
			oldStorage[i] = nil
			if current.next == nil {
				d.buckets[current.hash&uint32(len(d.buckets)-1)] = current
			} else {
				d.splitBucket(oldStorage, i, current)
			}
		}
	}
}

func (d *IdentityDictionary) splitBucket(oldStorage []*dictEntry, i int, head *dictEntry) {
	var loHead, loTail, hiHead, hiTail *dictEntry
	for current := head; current != nil; current = current.next {
		if current.hash&uint32(len(oldStorage)) == 0 {
			if loTail == nil {
				loHead = current
			} else {
				loTail.next = current
			}
			loTail = current
		} else {
			if hiTail == nil {
				hiHead = current
			} else {
				hiTail.next = current
			}
			hiTail = current
		}
	}
	if loTail != nil {
		loTail.next = nil
		d.buckets[i] = loHead
	}
	if hiTail != nil {
		hiTail.next = nil
		d.buckets[i+len(oldStorage)] = hiHead
	}
}

func (d *IdentityDictionary) RemoveAll() {
	d.buckets = make([]*dictEntry, len(d.buckets))
	d.size = 0
}

// ---- BasicBlock, BasicBlockEdge, ControlFlowGraph

type BasicBlock struct {
	inEdges  *Vector[*BasicBlock]
	outEdges *Vector[*BasicBlock]
	name     int
}

func NewBasicBlock(name int) *BasicBlock {
	return &BasicBlock{NewVector[*BasicBlock](), NewVector[*BasicBlock](), name}
}

func (b *BasicBlock) NumPred() int               { return b.inEdges.Size() }
func (b *BasicBlock) AddOutEdge(to *BasicBlock)  { b.outEdges.Append(to) }
func (b *BasicBlock) AddInEdge(from *BasicBlock) { b.inEdges.Append(from) }
func (b *BasicBlock) CustomHash() uint32         { return uint32(b.name) }

type BasicBlockEdge struct{ from, to *BasicBlock }

func NewBasicBlockEdge(cfg *ControlFlowGraph, fromName, toName int) *BasicBlockEdge {
	e := &BasicBlockEdge{cfg.CreateNode(fromName), cfg.CreateNode(toName)}
	e.from.AddOutEdge(e.to)
	e.to.AddInEdge(e.from)
	cfg.AddEdge(e)
	return e
}

type ControlFlowGraph struct {
	basicBlockMap *Vector[*BasicBlock]
	startNode     *BasicBlock
	edgeList      *Vector[*BasicBlockEdge]
}

func NewControlFlowGraph() *ControlFlowGraph {
	return &ControlFlowGraph{NewVector[*BasicBlock](), nil, NewVector[*BasicBlockEdge]()}
}

func (c *ControlFlowGraph) CreateNode(name int) *BasicBlock {
	var node *BasicBlock
	if c.basicBlockMap.At(name) != nil {
		node = c.basicBlockMap.At(name)
	} else {
		node = NewBasicBlock(name)
		c.basicBlockMap.AtPut(name, node)
	}
	if c.NumNodes() == 1 {
		c.startNode = node
	}
	return node
}

func (c *ControlFlowGraph) AddEdge(edge *BasicBlockEdge) { c.edgeList.Append(edge) }
func (c *ControlFlowGraph) NumNodes() int                { return c.basicBlockMap.Size() }

// ---- SimpleLoop, LoopStructureGraph

type SimpleLoop struct {
	basicBlocks  *Set[*BasicBlock]
	children     *Set[*SimpleLoop]
	parent       *SimpleLoop
	header       *BasicBlock
	isReducible  bool
	isRoot       bool
	nestingLevel int
	counter      int
	depthLevel   int
}

func NewSimpleLoop(bb *BasicBlock, isReducible bool) *SimpleLoop {
	l := &SimpleLoop{basicBlocks: NewSet[*BasicBlock](), children: NewSet[*SimpleLoop](),
		header: bb, isReducible: isReducible}
	if bb != nil {
		l.basicBlocks.Add(bb)
	}
	return l
}

func (l *SimpleLoop) AddNode(bb *BasicBlock) { l.basicBlocks.Add(bb) }

func (l *SimpleLoop) SetParent(parent *SimpleLoop) {
	l.parent = parent
	l.parent.children.Add(l)
}

func (l *SimpleLoop) SetNestingLevel(level int) {
	l.nestingLevel = level
	if level == 0 {
		l.isRoot = true
	}
}

type LoopStructureGraph struct {
	root        *SimpleLoop
	loops       *Vector[*SimpleLoop]
	loopCounter int
}

func NewLoopStructureGraph() *LoopStructureGraph {
	g := &LoopStructureGraph{loops: NewVector[*SimpleLoop]()}
	g.root = NewSimpleLoop(nil, true)
	g.root.SetNestingLevel(0)
	g.root.counter = g.loopCounter
	g.loopCounter++
	g.loops.Append(g.root)
	return g
}

func (g *LoopStructureGraph) CreateNewLoop(bb *BasicBlock, isReducible bool) *SimpleLoop {
	loop := NewSimpleLoop(bb, isReducible)
	loop.counter = g.loopCounter
	g.loopCounter++
	g.loops.Append(loop)
	return loop
}

func (g *LoopStructureGraph) CalculateNestingLevel() {
	// link up all 1st level loops to artificial root node.
	g.loops.ForEach(func(liter *SimpleLoop) {
		if !liter.isRoot && liter.parent == nil {
			liter.SetParent(g.root)
		}
	})
	// recursively traverse the tree and assign levels.
	g.calculateNestingLevelRec(g.root, 0)
}

func (g *LoopStructureGraph) calculateNestingLevelRec(loop *SimpleLoop, depth int) {
	loop.depthLevel = depth
	loop.children.ForEach(func(liter *SimpleLoop) {
		g.calculateNestingLevelRec(liter, depth+1)
		loop.SetNestingLevel(max(loop.nestingLevel, 1+liter.nestingLevel))
	})
}

func (g *LoopStructureGraph) NumLoops() int { return g.loops.Size() }

// ---- UnionFindNode

type UnionFindNode struct {
	parent    *UnionFindNode
	bb        *BasicBlock
	loop      *SimpleLoop
	dfsNumber int
}

func (u *UnionFindNode) InitNode(bb *BasicBlock, dfsNumber int) {
	u.parent = u
	u.bb = bb
	u.dfsNumber = dfsNumber
	u.loop = nil
}

// FindSet is the find routine, with path compression: the nodes on the path whose parent is not
// a root are pointed at this node's parent.
func (u *UnionFindNode) FindSet() *UnionFindNode {
	nodeList := NewVector[*UnionFindNode]()
	node := u
	for node != node.parent {
		if node.parent != node.parent.parent {
			nodeList.Append(node)
		}
		node = node.parent
	}
	nodeList.ForEach(func(iter *UnionFindNode) { iter.Union(u.parent) })
	return node
}

func (u *UnionFindNode) Union(basicBlock *UnionFindNode) { u.parent = basicBlock }

// ---- HavlakLoopFinder

type bbClass int

const (
	bbTop bbClass = iota
	bbNonHeader
	bbReducible
	bbSelf
	bbIrreducible
	bbDead
)

const unvisited = 2147483647      // Marker for uninitialized nodes.
const maxNonBackPreds = 32 * 1024 // Safeguard against pathological algorithm behavior.

type HavlakLoopFinder struct {
	cfg          *ControlFlowGraph
	lsg          *LoopStructureGraph
	nonBackPreds *Vector[*Set[int]]
	backPreds    *Vector[*Vector[int]]
	number       *IdentityDictionary
	maxSize      int
	header       []int
	typ          []bbClass
	last         []int
	nodes        []*UnionFindNode
}

func NewHavlakLoopFinder(cfg *ControlFlowGraph, lsg *LoopStructureGraph) *HavlakLoopFinder {
	return &HavlakLoopFinder{cfg: cfg, lsg: lsg, nonBackPreds: NewVector[*Set[int]](),
		backPreds: NewVector[*Vector[int]](), number: NewIdentityDictionary()}
}

func (f *HavlakLoopFinder) isAncestor(w, v int) bool { return w <= v && v <= f.last[w] }

func (f *HavlakLoopFinder) doDFS(currentNode *BasicBlock, current int) int {
	f.nodes[current].InitNode(currentNode, current)
	f.number.AtPut(currentNode, current)
	lastID := current
	outerBlocks := currentNode.outEdges
	for i := 0; i < outerBlocks.Size(); i++ {
		target := outerBlocks.At(i)
		if *f.number.At(target) == unvisited {
			lastID = f.doDFS(target, lastID+1)
		}
	}
	f.last[current] = lastID
	return lastID
}

func (f *HavlakLoopFinder) initAllNodes() {
	f.cfg.basicBlockMap.ForEach(func(bb *BasicBlock) { f.number.AtPut(bb, unvisited) })
	f.doDFS(f.cfg.startNode, 0)
}

func (f *HavlakLoopFinder) identifyEdges(size int) {
	for w := 0; w < size; w++ {
		f.header[w] = 0
		f.typ[w] = bbNonHeader
		nodeW := f.nodes[w].bb
		if nodeW == nil {
			f.typ[w] = bbDead
		} else {
			f.processEdges(nodeW, w)
		}
	}
}

func (f *HavlakLoopFinder) processEdges(nodeW *BasicBlock, w int) {
	if nodeW.NumPred() > 0 {
		nodeW.inEdges.ForEach(func(nodeV *BasicBlock) {
			v := *f.number.At(nodeV)
			if v != unvisited {
				if f.isAncestor(w, v) {
					f.backPreds.At(w).Append(v)
				} else {
					f.nonBackPreds.At(w).Add(v)
				}
			}
		})
	}
}

func (f *HavlakLoopFinder) FindLoops() {
	if f.cfg.startNode == nil {
		return
	}
	size := f.cfg.NumNodes()
	f.nonBackPreds.RemoveAll()
	f.backPreds.RemoveAll()
	f.number.RemoveAll()
	if size > f.maxSize {
		f.header = make([]int, size)
		f.typ = make([]bbClass, size)
		f.last = make([]int, size)
		f.nodes = make([]*UnionFindNode, size)
		f.maxSize = size
	}
	for i := 0; i < size; i++ {
		f.nonBackPreds.Append(NewSet[int]())
		f.backPreds.Append(NewVector[int]())
		f.nodes[i] = &UnionFindNode{}
	}
	f.initAllNodes()
	f.identifyEdges(size)
	// Start node is root of all other loops.
	f.header[0] = 0

	// Step c: reverse DFS preorder, so inner loop headers come before the loops around them.
	for w := size - 1; w >= 0; w-- {
		nodePool := NewVector[*UnionFindNode]() // 'P' in Havlak's paper
		nodeW := f.nodes[w].bb
		if nodeW != nil {
			f.stepD(w, nodePool)
			// Copy nodePool to workList.
			workList := NewVector[*UnionFindNode]()
			nodePool.ForEach(func(niter *UnionFindNode) { workList.Append(niter) })
			if nodePool.Size() != 0 {
				f.typ[w] = bbReducible
			}
			// work the list...
			for !workList.IsEmpty() {
				x := workList.RemoveFirst()
				// Step e: the algorithm has degenerated past the safeguard: give up.
				nonBackSize := f.nonBackPreds.At(x.dfsNumber).Size()
				if nonBackSize > maxNonBackPreds {
					return
				}
				f.stepEProcessNonBackPreds(w, nodePool, workList, x)
			}
			// Collapse the nodes of each SCC found into one, with a loop descriptor linked in.
			if nodePool.Size() > 0 || f.typ[w] == bbSelf {
				loop := f.lsg.CreateNewLoop(nodeW, f.typ[w] != bbIrreducible)
				f.setLoopAttributes(w, nodePool, loop)
			}
		}
	}
}

func (f *HavlakLoopFinder) stepEProcessNonBackPreds(w int, nodePool, workList *Vector[*UnionFindNode], x *UnionFindNode) {
	f.nonBackPreds.At(x.dfsNumber).ForEach(func(iter int) {
		y := f.nodes[iter]
		ydash := y.FindSet()
		if !f.isAncestor(w, ydash.dfsNumber) {
			f.typ[w] = bbIrreducible
			f.nonBackPreds.At(w).Add(ydash.dfsNumber)
		} else if ydash.dfsNumber != w {
			if !nodePool.HasSome(func(e *UnionFindNode) bool { return e == ydash }) {
				workList.Append(ydash)
				nodePool.Append(ydash)
			}
		}
	})
}

func (f *HavlakLoopFinder) setLoopAttributes(w int, nodePool *Vector[*UnionFindNode], loop *SimpleLoop) {
	f.nodes[w].loop = loop
	nodePool.ForEach(func(node *UnionFindNode) {
		// Add nodes to loop descriptor.
		f.header[node.dfsNumber] = w
		node.Union(f.nodes[w])
		// Nested loops are not added, but linked together.
		if node.loop != nil {
			node.loop.SetParent(loop)
		} else {
			loop.AddNode(node.bb)
		}
	})
}

func (f *HavlakLoopFinder) stepD(w int, nodePool *Vector[*UnionFindNode]) {
	f.backPreds.At(w).ForEach(func(v int) {
		if v != w {
			nodePool.Append(f.nodes[v].FindSet())
		} else {
			f.typ[w] = bbSelf
		}
	})
}

// ---- LoopTesterApp

type LoopTesterApp struct {
	cfg *ControlFlowGraph
	lsg *LoopStructureGraph
}

func NewLoopTesterApp() *LoopTesterApp {
	a := &LoopTesterApp{NewControlFlowGraph(), NewLoopStructureGraph()}
	a.cfg.CreateNode(0)
	return a
}

func (a *LoopTesterApp) buildDiamond(start int) int {
	bb0 := start
	NewBasicBlockEdge(a.cfg, bb0, bb0+1)
	NewBasicBlockEdge(a.cfg, bb0, bb0+2)
	NewBasicBlockEdge(a.cfg, bb0+1, bb0+3)
	NewBasicBlockEdge(a.cfg, bb0+2, bb0+3)
	return bb0 + 3
}

func (a *LoopTesterApp) buildConnect(start, end int) { NewBasicBlockEdge(a.cfg, start, end) }

func (a *LoopTesterApp) buildStraight(start, n int) int {
	for i := 0; i < n; i++ {
		a.buildConnect(start+i, start+i+1)
	}
	return start + n
}

func (a *LoopTesterApp) buildBaseLoop(from int) int {
	header := a.buildStraight(from, 1)
	diamond1 := a.buildDiamond(header)
	d11 := a.buildStraight(diamond1, 1)
	diamond2 := a.buildDiamond(d11)
	footer := a.buildStraight(diamond2, 1)
	a.buildConnect(diamond2, d11)
	a.buildConnect(diamond1, header)
	a.buildConnect(footer, from)
	footer = a.buildStraight(footer, 1)
	return footer
}

func (a *LoopTesterApp) Main(numDummyLoops, findLoopIterations, parLoops, pparLoops, ppparLoops int) [2]int {
	a.constructSimpleCFG()
	a.addDummyLoops(numDummyLoops)
	a.constructCFG(parLoops, pparLoops, ppparLoops)
	// Performing Loop Recognition, 1 Iteration, then findLoopIteration
	a.findLoops(a.lsg)
	for i := 0; i < findLoopIterations; i++ {
		a.findLoops(NewLoopStructureGraph())
	}
	a.lsg.CalculateNestingLevel()
	return [2]int{a.lsg.NumLoops(), a.cfg.NumNodes()}
}

func (a *LoopTesterApp) constructCFG(parLoops, pparLoops, ppparLoops int) {
	n := 2
	for parlooptrees := 0; parlooptrees < parLoops; parlooptrees++ {
		a.cfg.CreateNode(n + 1)
		a.buildConnect(2, n+1)
		n++
		for i := 0; i < pparLoops; i++ {
			top := n
			n = a.buildStraight(n, 1)
			for j := 0; j < ppparLoops; j++ {
				n = a.buildBaseLoop(n)
			}
			bottom := a.buildStraight(n, 1)
			a.buildConnect(n, top)
			n = bottom
		}
		a.buildConnect(n, 1)
	}
}

func (a *LoopTesterApp) addDummyLoops(numDummyLoops int) {
	for dummyloop := 0; dummyloop < numDummyLoops; dummyloop++ {
		a.findLoops(a.lsg)
	}
}

func (a *LoopTesterApp) findLoops(loopStructure *LoopStructureGraph) {
	NewHavlakLoopFinder(a.cfg, loopStructure).FindLoops()
}

func (a *LoopTesterApp) constructSimpleCFG() {
	a.cfg.CreateNode(0)
	a.buildBaseLoop(0)
	a.cfg.CreateNode(1)
	NewBasicBlockEdge(a.cfg, 0, 2)
}

func run(n int64) int64 {
	result := NewLoopTesterApp().Main(int(n), 50, 10, 10, 5)
	return int64(result[0])*1000000 + int64(result[1])
}

func main() { ports.Main(run, 15000) }
