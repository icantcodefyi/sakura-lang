"""Native/context sample protocol from bench/README.md; no output inside a workload."""
import json
import sys
import time


def main(run, maximum):
    mode, *args = sys.argv[1:]
    stable = '--stable' in args
    args = [arg for arg in args if arg != '--stable']
    sizes = [int(arg) for arg in args]
    if not sizes or any(n < 0 or n > maximum for n in sizes):
        raise ValueError('size outside the oracle corpus resource bound')
    if mode == 'values':
        print(json.dumps({'values': {str(n): str(run(n)) for n in sizes}}))
        return
    if mode != 'measure' or len(sizes) != 1:
        raise ValueError('expected values <n>... or measure <n> [--stable]')
    n = sizes[0]
    sink = 0
    start = time.perf_counter()
    while time.perf_counter() - start < (2 if stable else .25):
        sink = run(n)
    count = 1
    while True:
        start = time.perf_counter()
        for _ in range(count):
            sink = run(n)
        if time.perf_counter() - start >= .03 or count >= 1 << 20:
            break
        count *= 2
    for _ in range(9):
        start = time.perf_counter()
        for _ in range(count):
            sink = run(n)
        elapsed = (time.perf_counter() - start) * 1000 / count
        print(json.dumps({'n': n, 'ms': elapsed, 'checksum': str(sink)}))
