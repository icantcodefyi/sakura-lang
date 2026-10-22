// Are We Fast Yet's Towers: smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js, adapted
// to an ES module: the Benchmark base class and verifyResult are dropped (meta.toml's [verify]
// holds AWFY's expected result, 8191), and run(n) runs benchmark() n times on one instance, as
// AWFY's innerBenchmarkLoop does, and returns the sum of the results. The benchmark code is
// otherwise AWFY's, whose notice follows.
//
// This code is derived from the SOM benchmarks, see AUTHORS.md file.
//
// Copyright (c) 2015-2016 Stefan Marr <git@stefan-marr.de>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the 'Software'), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

class TowersDisk {
  constructor(size) {
    this.size = size;
    this.next = null;
  }
}

class Towers {
  constructor() {
    this.piles = null;
    this.movesDone = 0;
  }

  benchmark() {
    this.piles = new Array(3);
    this.buildTowerAt(0, 13);
    this.movesDone = 0;
    this.moveDisks(13, 0, 1);
    return this.movesDone;
  }

  pushDisk(disk, pile) {
    const top = this.piles[pile];
    if (top && disk.size >= top.size) {
      throw new Error('Cannot put a big disk on a smaller one');
    }

    disk.next = top;
    this.piles[pile] = disk;
  }

  popDiskFrom(pile) {
    const top = this.piles[pile];
    if (top === null) {
      throw new Error('Attempting to remove a disk from an empty pile');
    }

    this.piles[pile] = top.next;
    top.next = null;
    return top;
  }

  moveTopDisk(fromPile, toPile) {
    this.pushDisk(this.popDiskFrom(fromPile), toPile);
    this.movesDone += 1;
  }

  buildTowerAt(pile, disks) {
    for (let i = disks; i >= 0; i -= 1) {
      this.pushDisk(new TowersDisk(i), pile);
    }
  }

  moveDisks(disks, fromPile, toPile) {
    if (disks === 1) {
      this.moveTopDisk(fromPile, toPile);
    } else {
      const otherPile = (3 - fromPile) - toPile;
      this.moveDisks(disks - 1, fromPile, otherPile);
      this.moveTopDisk(fromPile, toPile);
      this.moveDisks(disks - 1, otherPile, toPile);
    }
  }
}

export function run(n) {
  const bench = new Towers();
  let sum = 0;
  for (let i = 0; i < n; i += 1) {
    sum += bench.benchmark();
  }
  return sum;
}
