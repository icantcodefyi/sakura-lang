// Are We Fast Yet's Bounce: smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/bounce.js and
// som.js's Random, adapted to an ES module: the Benchmark base class and verifyResult are dropped
// (meta.toml's [verify] holds AWFY's expected result, 1331), and run(n) runs benchmark() n times on
// one instance, as AWFY's innerBenchmarkLoop does, and returns the sum of the results. The
// benchmark code is otherwise AWFY's, whose notice follows.
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

// som.js's Random.
class Random {
  constructor() {
    this.seed = 74755;
  }

  next() {
    this.seed = (this.seed * 1309 + 13849) & 65535;
    return this.seed;
  }
}

class Ball {
  constructor(random) {
    this.x = random.next() % 500;
    this.y = random.next() % 500;
    this.xVel = (random.next() % 300) - 150;
    this.yVel = (random.next() % 300) - 150;
  }

  bounce() {
    const xLimit = 500;
    const yLimit = 500;
    let bounced = false;

    this.x += this.xVel;
    this.y += this.yVel;

    if (this.x > xLimit) {
      this.x = xLimit; this.xVel = 0 - Math.abs(this.xVel); bounced = true;
    }

    if (this.x < 0) {
      this.x = 0; this.xVel = Math.abs(this.xVel); bounced = true;
    }

    if (this.y > yLimit) {
      this.y = yLimit; this.yVel = 0 - Math.abs(this.yVel); bounced = true;
    }

    if (this.y < 0) {
      this.y = 0; this.yVel = Math.abs(this.yVel); bounced = true;
    }

    return bounced;
  }
}

class Bounce {
  benchmark() {
    const random = new Random();
    const ballCount = 100;
    let bounces = 0;
    const balls = new Array(ballCount);
    let i = 0;

    for (i = 0; i < ballCount; i += 1) {
      balls[i] = new Ball(random);
    }

    for (i = 0; i < 50; i += 1) {
      for (const ball of balls) {
        if (ball.bounce()) {
          bounces += 1;
        }
      }
    }
    return bounces;
  }
}

export function run(n) {
  const bench = new Bounce();
  let sum = 0;
  for (let i = 0; i < n; i += 1) {
    sum += bench.benchmark();
  }
  return sum;
}
