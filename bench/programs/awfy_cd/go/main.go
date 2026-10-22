// CD from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/cd.js and
// Java/src/cd; Copyright (c) 2001-2010, Purdue University; Copyright (C) 2015 Apple Inc.; see
// ../LICENSE). AWFY's RedBlackTree is a generic parent-pointer tree mutated in place, keys
// comparing through a compareTo method; SOM's Vector is a slice. sin and cos are Go's math
// package; every product is materialised with float64() so Go does not fuse multiply-adds.
// run(n) is cd(n - n % 2): AWFY's Simulator reads aircraft i + 1 for every even i, so it is
// defined for even n only.
package main

import (
	"math"

	ports "sakura-bench-ports"
)

// ---- vectors

type Vector2D struct{ x, y float64 }

func (v Vector2D) plus(other Vector2D) Vector2D  { return Vector2D{v.x + other.x, v.y + other.y} }
func (v Vector2D) minus(other Vector2D) Vector2D { return Vector2D{v.x - other.x, v.y - other.y} }

func compareNumbers(a, b float64) int {
	if a == b {
		return 0
	}
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	// We say that NaN is smaller than non-NaN.
	if a == a {
		return 1
	}
	return -1
}

func (v Vector2D) compareTo(other Vector2D) int {
	result := compareNumbers(v.x, other.x)
	if result != 0 {
		return result
	}
	return compareNumbers(v.y, other.y)
}

type Vector3D struct{ x, y, z float64 }

func (v Vector3D) plus(other Vector3D) Vector3D {
	return Vector3D{v.x + other.x, v.y + other.y, v.z + other.z}
}

func (v Vector3D) minus(other Vector3D) Vector3D {
	return Vector3D{v.x - other.x, v.y - other.y, v.z - other.z}
}

func (v Vector3D) dot(other Vector3D) float64 {
	return float64(v.x*other.x) + float64(v.y*other.y) + float64(v.z*other.z)
}

func (v Vector3D) squaredMagnitude() float64 { return v.dot(v) }
func (v Vector3D) magnitude() float64        { return math.Sqrt(v.squaredMagnitude()) }

func (v Vector3D) times(amount float64) Vector3D {
	return Vector3D{float64(v.x * amount), float64(v.y * amount), float64(v.z * amount)}
}

type CallSign struct{ value int64 }

func (c CallSign) compareTo(other CallSign) int {
	if c.value == other.value {
		return 0
	}
	if c.value < other.value {
		return -1
	}
	return 1
}

// ---- red-black tree

type Key[K any] interface{ compareTo(K) int }

type node[K Key[K], V any] struct {
	key                 K
	value               V
	left, right, parent *node[K, V]
	red                 bool
}

func treeMinimum[K Key[K], V any](x *node[K, V]) *node[K, V] {
	current := x
	for current.left != nil {
		current = current.left
	}
	return current
}

func (x *node[K, V]) successor() *node[K, V] {
	if x.right != nil {
		return treeMinimum(x.right)
	}
	y := x.parent
	for y != nil && x == y.right {
		x = y
		y = y.parent
	}
	return y
}

func isRed[K Key[K], V any](n *node[K, V]) bool { return n != nil && n.red }

type RedBlackTree[K Key[K], V any] struct{ root *node[K, V] }

// put is AWFY's: the replaced value and true when the key was present.
func (t *RedBlackTree[K, V]) put(key K, value V) (V, bool) {
	x, old, found := t.treeInsert(key, value)
	if found {
		return old, true
	}

	for x != t.root && x.parent.red {
		if x.parent == x.parent.parent.left {
			y := x.parent.parent.right
			if isRed(y) {
				// Case 1
				x.parent.red = false
				y.red = false
				x.parent.parent.red = true
				x = x.parent.parent
			} else {
				if x == x.parent.right {
					// Case 2
					x = x.parent
					t.leftRotate(x)
				}
				// Case 3
				x.parent.red = false
				x.parent.parent.red = true
				t.rightRotate(x.parent.parent)
			}
		} else {
			// Same as "then" clause with "right" and "left" exchanged.
			y := x.parent.parent.left
			if isRed(y) {
				// Case 1
				x.parent.red = false
				y.red = false
				x.parent.parent.red = true
				x = x.parent.parent
			} else {
				if x == x.parent.left {
					// Case 2
					x = x.parent
					t.rightRotate(x)
				}
				// Case 3
				x.parent.red = false
				x.parent.parent.red = true
				t.leftRotate(x.parent.parent)
			}
		}
	}
	t.root.red = false
	var none V
	return none, false
}

// remove is AWFY's: the removed value and true when the key was present.
func (t *RedBlackTree[K, V]) remove(key K) (V, bool) {
	z := t.findNode(key)
	if z == nil {
		var none V
		return none, false
	}

	// Y is the node to be unlinked from the tree.
	var y *node[K, V]
	if z.left == nil || z.right == nil {
		y = z
	} else {
		y = z.successor()
	}

	// X is the child of y which might potentially replace y in the tree. X might be nil.
	var x *node[K, V]
	if y.left != nil {
		x = y.left
	} else {
		x = y.right
	}

	var xParent *node[K, V]
	if x != nil {
		x.parent = y.parent
		xParent = x.parent
	} else {
		xParent = y.parent
	}
	if y.parent == nil {
		t.root = x
	} else if y == y.parent.left {
		y.parent.left = x
	} else {
		y.parent.right = x
	}

	if y != z {
		if !y.red {
			t.removeFixup(x, xParent)
		}
		y.parent = z.parent
		y.red = z.red
		y.left = z.left
		y.right = z.right

		if z.left != nil {
			z.left.parent = y
		}
		if z.right != nil {
			z.right.parent = y
		}
		if z.parent != nil {
			if z.parent.left == z {
				z.parent.left = y
			} else {
				z.parent.right = y
			}
		} else {
			t.root = y
		}
	} else if !y.red {
		t.removeFixup(x, xParent)
	}
	return z.value, true
}

func (t *RedBlackTree[K, V]) get(key K) (V, bool) {
	n := t.findNode(key)
	if n == nil {
		var none V
		return none, false
	}
	return n.value, true
}

// forEach visits every entry in key order.
func (t *RedBlackTree[K, V]) forEach(f func(K, V)) {
	if t.root == nil {
		return
	}
	for current := treeMinimum(t.root); current != nil; current = current.successor() {
		f(current.key, current.value)
	}
}

func (t *RedBlackTree[K, V]) findNode(key K) *node[K, V] {
	for current := t.root; current != nil; {
		comparison := key.compareTo(current.key)
		if comparison == 0 {
			return current
		}
		if comparison < 0 {
			current = current.left
		} else {
			current = current.right
		}
	}
	return nil
}

func (t *RedBlackTree[K, V]) treeInsert(key K, value V) (*node[K, V], V, bool) {
	var y *node[K, V]
	x := t.root
	for x != nil {
		y = x
		comparison := key.compareTo(x.key)
		if comparison < 0 {
			x = x.left
		} else if comparison > 0 {
			x = x.right
		} else {
			old := x.value
			x.value = value
			return nil, old, true
		}
	}

	z := &node[K, V]{key: key, value: value, parent: y, red: true}
	if y == nil {
		t.root = z
	} else if key.compareTo(y.key) < 0 {
		y.left = z
	} else {
		y.right = z
	}
	var none V
	return z, none, false
}

func (t *RedBlackTree[K, V]) leftRotate(x *node[K, V]) {
	y := x.right

	// Turn y's left subtree into x's right subtree.
	x.right = y.left
	if y.left != nil {
		y.left.parent = x
	}

	// Link x's parent to y.
	y.parent = x.parent
	if x.parent == nil {
		t.root = y
	} else if x == x.parent.left {
		x.parent.left = y
	} else {
		x.parent.right = y
	}

	// Put x on y's left.
	y.left = x
	x.parent = y
}

func (t *RedBlackTree[K, V]) rightRotate(y *node[K, V]) {
	x := y.left

	// Turn x's right subtree into y's left subtree.
	y.left = x.right
	if x.right != nil {
		x.right.parent = y
	}

	// Link y's parent to x.
	x.parent = y.parent
	if y.parent == nil {
		t.root = x
	} else if y == y.parent.left {
		y.parent.left = x
	} else {
		y.parent.right = x
	}

	x.right = y
	y.parent = x
}

func (t *RedBlackTree[K, V]) removeFixup(x, xParent *node[K, V]) {
	for x != t.root && !isRed(x) {
		if x == xParent.left {
			// The text points out that w cannot be nil: it follows from the red-black properties.
			w := xParent.right
			if w.red {
				// Case 1
				w.red = false
				xParent.red = true
				t.leftRotate(xParent)
				w = xParent.right
			}
			if !isRed(w.left) && !isRed(w.right) {
				// Case 2
				w.red = true
				x = xParent
				xParent = x.parent
			} else {
				if !isRed(w.right) {
					// Case 3
					w.left.red = false
					w.red = true
					t.rightRotate(w)
					w = xParent.right
				}
				// Case 4
				w.red = xParent.red
				xParent.red = false
				if w.right != nil {
					w.right.red = false
				}
				t.leftRotate(xParent)
				x = t.root
				xParent = x.parent
			}
		} else {
			// Same as "then" clause with "right" and "left" exchanged.
			w := xParent.left
			if w.red {
				// Case 1
				w.red = false
				xParent.red = true
				t.rightRotate(xParent)
				w = xParent.left
			}
			if !isRed(w.right) && !isRed(w.left) {
				// Case 2
				w.red = true
				x = xParent
				xParent = x.parent
			} else {
				if !isRed(w.left) {
					// Case 3
					w.right.red = false
					w.red = true
					t.leftRotate(w)
					w = xParent.left
				}
				// Case 4
				w.red = xParent.red
				xParent.red = false
				if w.left != nil {
					w.left.red = false
				}
				t.rightRotate(xParent)
				x = t.root
				xParent = x.parent
			}
		}
	}
	if x != nil {
		x.red = false
	}
}

// ---- the simulation

const (
	minX            = 0.0
	minY            = 0.0
	maxX            = 1000.0
	maxY            = 1000.0
	minZ            = 0.0
	maxZ            = 10.0
	proximityRadius = 1.0
	goodVoxelSize   = proximityRadius * 2.0
)

var horizontal = Vector2D{goodVoxelSize, 0}
var vertical = Vector2D{0, goodVoxelSize}

type Motion struct {
	callsign       CallSign
	posOne, posTwo Vector3D
}

type Collision struct {
	aircraftA, aircraftB CallSign
	position             Vector3D
}

type Aircraft struct {
	callsign CallSign
	position Vector3D
}

func (m *Motion) delta() Vector3D { return m.posTwo.minus(m.posOne) }

func (m *Motion) findIntersection(other *Motion) (Vector3D, bool) {
	init1 := m.posOne
	init2 := other.posOne
	vec1 := m.delta()
	vec2 := other.delta()
	radius := proximityRadius

	// a = (V2 - V1)^T * (V2 - V1)
	a := vec2.minus(vec1).squaredMagnitude()

	if a != 0 {
		// b = 2 * <I1-I2, V1-V2>
		b := float64(2 * init1.minus(init2).dot(vec1.minus(vec2)))
		// c = -r^2 + (I2 - I1)^T * (I2 - I1)
		c := float64(-radius*radius) + init2.minus(init1).squaredMagnitude()

		discr := float64(b*b) - float64(4*a*c)
		if discr < 0 {
			return Vector3D{}, false
		}

		v1 := (-b - math.Sqrt(discr)) / (2 * a)
		v2 := (-b + math.Sqrt(discr)) / (2 * a)

		if v1 <= v2 && ((v1 <= 1 && 1 <= v2) || (v1 <= 0 && 0 <= v2) || (0 <= v1 && v2 <= 1)) {
			// Pick a good "time" at which to report the collision.
			v := v1
			if v1 <= 0 {
				v = 0
			}

			result1 := init1.plus(vec1.times(v))
			result2 := init2.plus(vec2.times(v))

			result := result1.plus(result2).times(0.5)
			if result.x >= minX && result.x <= maxX && result.y >= minY && result.y <= maxY &&
				result.z >= minZ && result.z <= maxZ {
				return result, true
			}
		}
		return Vector3D{}, false
	}

	// The planes have the same speeds and are moving in parallel (or not moving at all).
	dist := init2.minus(init1).magnitude()
	if dist <= radius {
		return init1.plus(init2).times(0.5), true
	}
	return Vector3D{}, false
}

func isInVoxel(voxel Vector2D, motion *Motion) bool {
	if voxel.x > maxX || voxel.x < minX || voxel.y > maxY || voxel.y < minY {
		return false
	}

	init := motion.posOne
	fin := motion.posTwo

	vS := goodVoxelSize
	r := proximityRadius / 2.0

	vX := voxel.x
	x0 := init.x
	xv := fin.x - init.x

	vY := voxel.y
	y0 := init.y
	yv := fin.y - init.y

	lowX := (vX - r - x0) / xv
	highX := (vX + vS + r - x0) / xv

	if xv < 0 {
		lowX, highX = highX, lowX
	}

	lowY := (vY - r - y0) / yv
	highY := (vY + vS + r - y0) / yv

	if yv < 0 {
		lowY, highY = highY, lowY
	}

	return ((xv == 0 && vX <= x0+r && x0-r <= vX+vS) /* no motion in x */ ||
		((lowX <= 1 && 1 <= highX) || (lowX <= 0 && 0 <= highX) || (0 <= lowX && highX <= 1))) &&
		((yv == 0 && vY <= y0+r && y0-r <= vY+vS) /* no motion in y */ ||
			((lowY <= 1 && 1 <= highY) || (lowY <= 0 && 0 <= highY) || (0 <= lowY && highY <= 1))) &&
		(xv == 0 || yv == 0 /* no motion in x or y or both */ ||
			(lowY <= highX && highX <= highY) ||
			(lowY <= lowX && lowX <= highY) ||
			(lowX <= lowY && highY <= highX))
}

type VoxelMap = RedBlackTree[Vector2D, *[]Motion]

func putIntoMap(voxelMap *VoxelMap, voxel Vector2D, motion *Motion) {
	vec, ok := voxelMap.get(voxel)
	if !ok {
		vec = &[]Motion{}
		voxelMap.put(voxel, vec)
	}
	*vec = append(*vec, *motion)
}

func recurse(voxelMap *VoxelMap, seen *RedBlackTree[Vector2D, bool], nextVoxel Vector2D, motion *Motion) {
	if !isInVoxel(nextVoxel, motion) {
		return
	}
	if old, _ := seen.put(nextVoxel, true); old {
		return
	}

	putIntoMap(voxelMap, nextVoxel, motion)

	recurse(voxelMap, seen, nextVoxel.minus(horizontal), motion)
	recurse(voxelMap, seen, nextVoxel.plus(horizontal), motion)
	recurse(voxelMap, seen, nextVoxel.minus(vertical), motion)
	recurse(voxelMap, seen, nextVoxel.plus(vertical), motion)
	recurse(voxelMap, seen, nextVoxel.minus(horizontal).minus(vertical), motion)
	recurse(voxelMap, seen, nextVoxel.minus(horizontal).plus(vertical), motion)
	recurse(voxelMap, seen, nextVoxel.plus(horizontal).minus(vertical), motion)
	recurse(voxelMap, seen, nextVoxel.plus(horizontal).plus(vertical), motion)
}

func voxelHash(position Vector3D) Vector2D {
	xDiv := int32(position.x / goodVoxelSize)
	yDiv := int32(position.y / goodVoxelSize)

	result := Vector2D{float64(goodVoxelSize * float64(xDiv)), float64(goodVoxelSize * float64(yDiv))}

	if position.x < 0 {
		result.x -= goodVoxelSize
	}
	if position.y < 0 {
		result.y -= goodVoxelSize
	}
	return result
}

func drawMotionOnVoxelMap(voxelMap *VoxelMap, motion *Motion) {
	seen := &RedBlackTree[Vector2D, bool]{}
	recurse(voxelMap, seen, voxelHash(motion.posOne), motion)
}

func reduceCollisionSet(motions []Motion) [][]Motion {
	voxelMap := &VoxelMap{}
	for i := range motions {
		drawMotionOnVoxelMap(voxelMap, &motions[i])
	}

	var result [][]Motion
	voxelMap.forEach(func(_ Vector2D, vec *[]Motion) {
		if len(*vec) > 1 {
			result = append(result, *vec)
		}
	})
	return result
}

type CollisionDetector struct {
	state RedBlackTree[CallSign, Vector3D]
}

func (d *CollisionDetector) handleNewFrame(frame []Aircraft) []Collision {
	var motions []Motion
	seen := &RedBlackTree[CallSign, bool]{}

	for _, aircraft := range frame {
		oldPosition, found := d.state.put(aircraft.callsign, aircraft.position)
		newPosition := aircraft.position
		seen.put(aircraft.callsign, true)

		if !found {
			// Treat newly introduced aircraft as if they were stationary.
			oldPosition = newPosition
		}
		motions = append(motions, Motion{aircraft.callsign, oldPosition, newPosition})
	}

	// Remove aircraft that are no longer present.
	var toRemove []CallSign
	d.state.forEach(func(callsign CallSign, _ Vector3D) {
		if wasSeen, _ := seen.get(callsign); !wasSeen {
			toRemove = append(toRemove, callsign)
		}
	})
	for _, callsign := range toRemove {
		d.state.remove(callsign)
	}

	allReduced := reduceCollisionSet(motions)
	var collisions []Collision

	for _, reduced := range allReduced {
		for i := 0; i < len(reduced); i++ {
			motion1 := &reduced[i]
			for j := i + 1; j < len(reduced); j++ {
				motion2 := &reduced[j]
				if collision, ok := motion1.findIntersection(motion2); ok {
					collisions = append(collisions, Collision{motion1.callsign, motion2.callsign, collision})
				}
			}
		}
	}
	return collisions
}

type Simulator struct{ aircraft []CallSign }

func newSimulator(numAircraft int64) *Simulator {
	s := &Simulator{}
	for i := int64(0); i < numAircraft; i++ {
		s.aircraft = append(s.aircraft, CallSign{i})
	}
	return s
}

func (s *Simulator) simulate(time float64) []Aircraft {
	var frame []Aircraft
	for i := 0; i < len(s.aircraft); i += 2 {
		frame = append(frame, Aircraft{s.aircraft[i], Vector3D{time, float64(math.Cos(time)*2) + float64(i*3), 10}})
		frame = append(frame, Aircraft{s.aircraft[i+1], Vector3D{time, float64(math.Sin(time)*2) + float64(i*3), 10}})
	}
	return frame
}

func cd(numAircrafts int64) int64 {
	numFrames := 200
	simulator := newSimulator(numAircrafts)
	detector := &CollisionDetector{}

	var actualCollisions int64
	for i := 0; i < numFrames; i++ {
		time := float64(i) / 10
		collisions := detector.handleNewFrame(simulator.simulate(time))
		actualCollisions += int64(len(collisions))
	}
	return actualCollisions
}

func run(n int64) int64 { return cd(n - n%2) }

func main() { ports.Main(run, 1000) }
