// Towers from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
// port: heap disks linked through a mutable `next`, three piles as a fixed array of tops, disks
// relinked in place by every move. An illegal move panics with AWFY's message. run(n) sums n
// benchmark() results.
package main

import ports "sakura-bench-ports"

type TowersDisk struct {
	size int64
	next *TowersDisk
}

type Towers struct {
	piles     [3]*TowersDisk
	movesDone int64
}

func (t *Towers) Benchmark() int64 {
	t.piles = [3]*TowersDisk{}
	t.buildTowerAt(0, 13)
	t.movesDone = 0
	t.moveDisks(13, 0, 1)
	return t.movesDone
}

func (t *Towers) pushDisk(disk *TowersDisk, pile int64) {
	top := t.piles[pile]
	if top != nil && disk.size >= top.size {
		panic("Cannot put a big disk on a smaller one")
	}

	disk.next = top
	t.piles[pile] = disk
}

func (t *Towers) popDiskFrom(pile int64) *TowersDisk {
	top := t.piles[pile]
	if top == nil {
		panic("Attempting to remove a disk from an empty pile")
	}

	t.piles[pile] = top.next
	top.next = nil
	return top
}

func (t *Towers) moveTopDisk(fromPile, toPile int64) {
	t.pushDisk(t.popDiskFrom(fromPile), toPile)
	t.movesDone++
}

func (t *Towers) buildTowerAt(pile, disks int64) {
	for i := disks; i >= 0; i-- {
		t.pushDisk(&TowersDisk{size: i}, pile)
	}
}

func (t *Towers) moveDisks(disks, fromPile, toPile int64) {
	if disks == 1 {
		t.moveTopDisk(fromPile, toPile)
	} else {
		otherPile := (3 - fromPile) - toPile
		t.moveDisks(disks-1, fromPile, otherPile)
		t.moveTopDisk(fromPile, toPile)
		t.moveDisks(disks-1, otherPile, toPile)
	}
}

func run(n int64) int64 {
	bench := &Towers{}
	sum := int64(0)
	for i := int64(0); i < n; i++ {
		sum += bench.Benchmark()
	}
	return sum
}

func main() { ports.Main(run, 2500) }
