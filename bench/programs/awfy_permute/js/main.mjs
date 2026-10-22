// Are We Fast Yet's Permute: smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/permute.js,
// adapted to an ES module: the Benchmark base class and verifyResult are dropped (meta.toml's
// [verify] holds AWFY's expected result, 8660), and run(n) runs benchmark() n times on one
// instance, as AWFY's innerBenchmarkLoop does, and returns the sum of the results. The benchmark
// code is otherwise AWFY's, whose notice follows.
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

class Permute {
  constructor() {
    this.count = 0;
    this.v = null;
  }

  benchmark() {
    this.count = 0;
    this.v = new Array(6).fill(0);
    this.permute(6);
    return this.count;
  }

  permute(n) {
    this.count += 1;
    if (n !== 0) {
      const n1 = n - 1;
      this.permute(n1);
      for (let i = n1; i >= 0; i -= 1) {
        this.swap(n1, i);
        this.permute(n1);
        this.swap(n1, i);
      }
    }
  }

  swap(i, j) {
    const tmp = this.v[i];
    this.v[i] = this.v[j];
    this.v[j] = tmp;
  }
}

export function run(n) {
  const bench = new Permute();
  let sum = 0;
  for (let i = 0; i < n; i += 1) {
    sum += bench.benchmark();
  }
  return sum;
}
