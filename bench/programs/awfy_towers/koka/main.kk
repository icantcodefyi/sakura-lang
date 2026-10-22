// Towers from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). As the Sakura
// source, a disk is an immutable struct whose `next` is a `maybe` (null is Nothing) and a pile is
// its top: pushDisk stores the disk updated with the old top as `next`, popDiskFrom returns the
// top updated with Nothing (both copy updates, made in place when the disk is uniquely owned).
// The benchmark object's `piles` (a vector of three) and `movesDone` are local `var`s of
// benchmark(), and the methods are local functions that write them. An illegal move throws with
// AWFY's message. run(n) sums n benchmark() results.
import runner

struct disk
  size: int
  next: maybe<disk>

fun benchmark(): <div,exn> int {
  var piles : vector<maybe<disk>> := vector(3, Nothing)
  var moves-done := 0
  var i := 0

  fun push-disk(d: disk, pile: int) {
    val top = piles[pile]
    match top {
      Just(t) -> if d.size >= t.size then throw("Cannot put a big disk on a smaller one")
      Nothing -> ()
    }
    piles[pile] := Just(d(next = top))
  }

  fun pop-disk-from(pile: int) {
    match piles[pile] {
      Nothing -> throw("Attempting to remove a disk from an empty pile")
      Just(top) -> {
        piles[pile] := top.next
        top(next = Nothing)
      }
    }
  }

  fun move-top-disk(from-pile: int, to-pile: int) {
    push-disk(pop-disk-from(from-pile), to-pile)
    moves-done := moves-done + 1
  }

  fun build-tower-at(pile: int, disks: int) {
    i := disks
    while { i >= 0 } {
      push-disk(Disk(i, Nothing), pile)
      i := i - 1
    }
  }

  fun move-disks(disks: int, from-pile: int, to-pile: int) {
    if disks == 1 then move-top-disk(from-pile, to-pile)
    else {
      val other-pile = (3 - from-pile) - to-pile
      move-disks(disks - 1, from-pile, other-pile)
      move-top-disk(from-pile, to-pile)
      move-disks(disks - 1, other-pile, to-pile)
    }
  }

  build-tower-at(0, 13)
  moves-done := 0
  move-disks(13, 0, 1)
  moves-done
}

fun run(n: int): <div,exn> int {
  var sum := 0
  var j := 0
  while { j < n } {
    sum := sum + benchmark()
    j := j + 1
  }
  sum
}

fun main() { bench-main(run, 2500) }
