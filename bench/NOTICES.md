# Third-party notices for the community suites

The `bg_*` and `awfy_*` programs implement the tasks of two public benchmark suites, and some
of their files adapt those suites' own published code. This file credits the sources and keeps
their licence texts, as the licences require. Each adapted file also says what it adapts in its
header comment. Everything else in `bench/` is licensed like the repository.

## The Computer Language Benchmarks Game (`bg_*`)

Task definitions, input sizes, the n-body initial conditions, the fasta sequences and the
expected output files (whose MD5s are in each `meta.toml`'s `official` table) come from
<https://benchmarksgame-team.pages.debian.net/benchmarksgame/>, fetched 2026-10-22. A port that
adapts a contributed program keeps that program's "contributed by" line in its header.

> Revised BSD license
>
> This is a specific instance of the Open Source Initiative (OSI) BSD license template.
>
> Copyright © 2004-2008 Brent Fulgham, 2005-2025 Isaac Gouy
>
> All rights reserved.
>
> Redistribution and use in source and binary forms, with or without modification, are permitted
> provided that the following conditions are met:
>
> - Redistributions of source code must retain the above copyright notice, this list of
>   conditions and the following disclaimer.
> - Redistributions in binary form must reproduce the above copyright notice, this list of
>   conditions and the following disclaimer in the documentation and/or other materials provided
>   with the distribution.
> - Neither the name "The Computer Language Benchmarks Game" nor the name "The Benchmarks Game"
>   nor the name "The Computer Language Shootout Benchmarks" nor the names of its contributors may
>   be used to endorse or promote products derived from this software without specific prior
>   written permission.
>
> THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR
> IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
> FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR
> CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
> CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
> SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
> THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
> OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
> POSSIBILITY OF SUCH DAMAGE.

## Are We Fast Yet (`awfy_*`, `deltablue`)

Stefan Marr, Benoit Daloze and Hanspeter Mössenböck, *Cross-Language Compiler Benchmarking: Are
We Fast Yet?*, DLS 2016. Code from <https://github.com/smarr/are-we-fast-yet> at commit
`74306fec151070fd07157cefeacf19e7e0bcdc89` (2026-03-22): the benchmarks' algorithms, the `som`
support library (`Vector`, `Dictionary`, `IdentityDictionary`, `Set`, `Random`), the expected
results of each `verifyResult`, and the JavaScript files the hand-JS controls adapt. Its source
files carry this licence:

> Copyright (c) 2015-2016 Stefan Marr <git@stefan-marr.de>
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software
> and associated documentation files (the 'Software'), to deal in the Software without
> restriction, including without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
> Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
> BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
> DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Some files name later years (2015-2022) or other authors listed in the repository's
`AUTHORS.md`; each adapted file keeps its own copyright line.

Richards and DeltaBlue are, in Are We Fast Yet's words, "derived from the Smalltalk sources
provided by Mario Wolczko"; licence details at
<http://web.archive.org/web/20050825101121/http://www.sunlabs.com/people/mario/java_benchmarking/index.html>.
`programs/deltablue/LICENSE` and `programs/awfy_richards/LICENSE` record that provenance; as
for DeltaBlue, a maintainer must confirm the original terms before these two programs are
redistributed outside this repository.

Some Are We Fast Yet benchmarks carry their own origin's licence, kept in the files that adapt them:

- **CD** derives from CDx (Purdue University) by way of WebKit's JetStream: the Purdue and Apple
  BSD notice is in `programs/awfy_cd/LICENSE` and every file of that program. The Sakura port also
  carries a port of fdlibm's `sin`/`cos` (Sakura's std has neither), under Sun Microsystems'
  notice, also in that `LICENSE`.
- **Mandelbrot** in Are We Fast Yet is adapted from the Benchmarks Game (© 2004-2013 Brent Fulgham,
  the revised BSD licence above); `programs/awfy_mandelbrot/js/main.mjs` keeps that header.
- **Havlak** is Google's loop-recognition benchmark (Robert Hundt, 2011), © 2011 Google Inc.
  under the Apache License 2.0, whose full text is the repository's `LICENSE-APACHE`;
  `programs/awfy_havlak/LICENSE` records the notice and says how each file was modified.

## klib `khash.h` (`bg_k_nucleotide`'s C port)

`programs/bg_k_nucleotide/c/khash.h` is klib's khash 0.2.8 (<https://github.com/attractivechaos/klib>,
commit `5fc2090`), the hash table the Benchmarks Game's C k-nucleotide programs use, unmodified
with its MIT licence header (© 2008, 2009, 2011 Attractive Chaos).
