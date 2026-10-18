"""Matched ports of WS-B's Sakura sources. Integers stay JS-safe within runner bounds."""
import asyncio
import json
import math


def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)


def countdown(n):
    acc = 0
    for i in range(n, 0, -1):
        acc += i
    even = True
    for _ in range(n, 0, -1):
        even = not even
    return acc + int(even)


def sieve(n):
    flags = [i >= 2 for i in range(n)]
    p = 2
    while p * p < n:
        if flags[p]:
            flags = [flag and (j < p * p or j % p != 0) for j, flag in enumerate(flags)]
        p += 1
    return sum(flags)


def pipeline(n):
    xs = list(range(n))
    ys = list(map(lambda x: x * 7 % 1000, xs))
    zs = list(filter(lambda y: y % 2 == 0, ys))
    acc = 0
    for z in zs:
        acc += z
    return acc


def binary_trees(n):
    def bottom_up(d):
        return (bottom_up(d - 1), bottom_up(d - 1)) if d > 0 else (None, None)
    def check(t):
        return 0 if t is None else 1 + check(t[0]) + check(t[1])
    maximum = max(6, n)
    total = check(bottom_up(maximum + 1))
    long_lived = bottom_up(maximum)
    for d in range(4, maximum + 1, 2):
        for _ in range(2 ** (maximum - d + 4)):
            total += check(bottom_up(d))
    return total + check(long_lived)


def rbtree(n):
    red, black = 0, 1
    def is_red(t):
        return t is not None and t[0] == red
    def balance(c, l, k, r):
        if c == black:
            if is_red(l) and is_red(l[1]):
                a = l[1]
                return (red, (black, a[1], a[2], a[3]), l[2], (black, l[3], k, r))
            if is_red(l) and is_red(l[3]):
                a = l[3]
                return (red, (black, l[1], l[2], a[1]), a[2], (black, a[3], k, r))
            if is_red(r) and is_red(r[1]):
                a = r[1]
                return (red, (black, l, k, a[1]), a[2], (black, a[3], r[2], r[3]))
            if is_red(r) and is_red(r[3]):
                a = r[3]
                return (red, (black, l, k, r[1]), r[2], (black, a[1], a[2], a[3]))
        return (c, l, k, r)
    def ins(t, k):
        if t is None:
            return (red, None, k, None)
        c, l, y, r = t
        if k < y:
            return balance(c, ins(l, k), y, r)
        if k > y:
            return balance(c, l, y, ins(r, k))
        return t
    def size(t):
        return 0 if t is None else size(t[1]) + 1 + size(t[3])
    def height(t):
        return 0 if t is None else int(t[0] == black) + height(t[1])
    def member(t, k):
        while t is not None:
            if k < t[2]:
                t = t[1]
            elif k > t[2]:
                t = t[3]
            else:
                return True
        return False
    tree = None
    kept = []
    for i in range(n):
        c, l, k, r = ins(tree, i * 7919 % (2 * n + 1))
        tree = (black, l, k, r)
        if i % 100 == 0:
            kept.append(tree)
    total = size(tree)
    for j, tree in enumerate(kept):
        total += height(tree)
        if member(tree, j * 3):
            total += 1
    return total


def map_set(n):
    mi, ms, mt, s = {}, {}, {}, set()
    for i in range(n):
        mi[i] = i % 1000
        ms[f'k{i}'] = i % 1000
        mt[(i % 100, i // 100)] = i % 7
        s.add(i % 97)
    snapshot = mi.copy()
    found = 0
    for i in range(2 * n):
        found += mi.get(i, 0)
        found += ms.get(f'k{i}', 0)
        found += mt.get((i % 100, i // 100), 0)
    for i in range(0, n, 2):
        mi[i] = mi.get(i, 0) + 7
        ms[f'k{i}'] = ms.get(f'k{i}', 0) + 7
    for i in range(0, n, 3):
        mi.pop(i, None)
        ms.pop(f'k{i}', None)
        mt.pop((i % 100, i // 100), None)
    total = 0
    for m in (mi, ms, mt):
        for v in m.values():
            total += v
    snap = sum(snapshot.values())
    return found + total + snap + len(mi) + len(ms) + len(mt) + len(snapshot) + len(s)


def shapes(n):
    def area(s):
        kind, a, b = s
        return 3 * a * a if kind == 0 else a * b if kind == 1 else a * a
    def grow(item):
        ident, (kind, a, b), tag = item
        shape = (1, a, a + 1) if kind == 0 else (2, (a + b) % 100, 0) if kind == 1 else (0, a % 50 + 1, 0)
        return (ident, shape, 0 if tag is None else None if tag > 5 else tag + 1)
    seed = 42
    items = []
    for i in range(n):
        seed = seed * 48271 % 2147483647
        k = seed % 100
        shape = (0, k, 0) if k < 33 else (1, k, k % 7 + 1) if k < 66 else (2, k % 13, 0)
        items.append((i, shape, None if k % 4 == 0 else k % 9))
    total = 0
    for _ in range(5):
        items = list(map(grow, items))
        for ident, shape, tag in items:
            total += area(shape) + (0 if tag is None else tag) + ident % 3
    return total


def tokenizer(n):
    words = ['alpha', 'naïve', '日本語', '🌸', 'x1', 'Grüße', 'ok', 'mañana']
    text = ''
    for i in range(n):
        text = text + words[i % 8] + (', ' if i % 5 == 4 else ' ')
    tokens = byte_count = scalars = commas = 0
    for token in text.split(' '):
        if token:
            tokens += 1
            byte_count += len(token.encode('utf-8'))
            scalars += len(token)
            if token.endswith(','):
                commas += 1
    return tokens + 3 * byte_count + 7 * scalars + 11 * commas


def json_validate(n):
    lines = []
    for i in range(n):
        name = '' if i % 17 == 0 else f'user{i}'
        age = 200 if i % 13 == 0 else i % 90
        lines.append(f'{{"id": {i}, "name": "{name}", "age": {age}}}')
    text = '\n'.join(lines)
    valid = byte_count = ids = invalid = 0
    for line in text.split('\n'):
        if not line:
            continue
        user = json.loads(line)
        ident, name, age = user['id'], user['name'], user['age']
        if (type(ident) is not int or abs(ident) > 9007199254740991 or
                not isinstance(name, str) or not 0 < len(name.encode()) <= 32 or
                type(age) is not int or not 0 <= age <= 150):
            invalid += 1
        else:
            valid += 1
            encoded = json.dumps({'id': ident, 'name': name, 'age': age}, separators=(',', ':'), ensure_ascii=False)
            byte_count += len(encoded.encode())
            ids += ident
    return valid + byte_count + ids + 1000 * invalid


def spectral_norm(n):
    if n == 0:
        return 0
    def a(i, j):
        return 1.0 / (((i + j) * (i + j + 1)) / 2 + i + 1)
    def av(v, transpose=False):
        out = []
        for i in range(n):
            acc = 0.0
            for j in range(n):
                acc += (a(j, i) if transpose else a(i, j)) * v[j]
            out.append(acc)
        return out
    def atav(v):
        return av(av(v), True)
    u = [1.0] * n
    v = u
    for _ in range(10):
        v = atav(u)
        u = atav(v)
    vbv = vv = 0.0
    for i in range(n):
        vbv += u[i] * v[i]
        vv += v[i] * v[i]
    return math.floor(math.sqrt(vbv / vv) * 1e9 + .5)


def nbody(n):
    solar_mass = 4 * 3.141592653589793 * 3.141592653589793
    days = 365.24
    def planet(x, y, z, vx, vy, vz, mass):
        return [x, y, z, vx * days, vy * days, vz * days, mass * solar_mass]
    bodies = [[0., 0., 0., 0., 0., 0., solar_mass],
        planet(4.8414314424647209, -1.16032004402742839, -1.03622044471123109e-1, 1.66007664274403694e-3, 7.69901118419740425e-3, -6.90460016972063023e-5, 9.54791938424326609e-4),
        planet(8.34336671824457987, 4.12479856412430479, -4.03523417114321381e-1, -2.76742510726862411e-3, 4.99852801234917238e-3, 2.30417297573763929e-5, 2.85885980666130812e-4),
        planet(1.2894369562139131e1, -1.51111514016986312e1, -2.23307578892655734e-1, 2.96460137564761618e-3, 2.3784717395948095e-3, -2.96589568540237556e-5, 4.36624404335156298e-5),
        planet(1.53796971148509165e1, -2.59193146099879641e1, 1.79258772950371181e-1, 2.68067772490389322e-3, 1.62824170038242295e-3, -9.5159225451971587e-5, 5.15138902046611451e-5)]
    px = py = pz = 0.0
    for b in bodies:
        px += b[3] * b[6]
        py += b[4] * b[6]
        pz += b[5] * b[6]
    bodies[0][3:6] = [-px / solar_mass, -py / solar_mass, -pz / solar_mass]
    dt = .01
    for _ in range(n):
        for i, b in enumerate(bodies):
            for c in bodies[i + 1:]:
                dx, dy, dz = b[0] - c[0], b[1] - c[1], b[2] - c[2]
                d2 = dx * dx + dy * dy + dz * dz
                mag = dt / (d2 * math.sqrt(d2))
                b[3] -= dx * c[6] * mag
                b[4] -= dy * c[6] * mag
                b[5] -= dz * c[6] * mag
                c[3] += dx * b[6] * mag
                c[4] += dy * b[6] * mag
                c[5] += dz * b[6] * mag
        for b in bodies:
            for k in range(3):
                b[k] += dt * b[k + 3]
    energy = 0.0
    for i, b in enumerate(bodies):
        energy += .5 * b[6] * (b[3] * b[3] + b[4] * b[4] + b[5] * b[5])
        for c in bodies[i + 1:]:
            dx, dy, dz = b[0] - c[0], b[1] - c[1], b[2] - c[2]
            energy -= b[6] * c[6] / math.sqrt(dx * dx + dy * dy + dz * dz)
    scaled = energy * 1e9
    return math.copysign(math.floor(abs(scaled) + .5), scaled).__int__()


def contracts(n):
    def increment(x):
        if not x >= 0:
            raise ValueError('D020')
        result = x + 1
        if not result > x:
            raise ValueError('D021')
        return result
    def total(xs):
        if not all(x >= 0 for x in xs):
            raise ValueError('D020')
        return sum(xs)
    return _contracts(n, increment, total)


def _contracts(n, increment, total):
    xs = list(range(16))
    i = acc = 0
    while i < n:
        i = increment(i)
        if i % 16 == 0:
            acc += total(xs)
    return acc + i


def contracts_plain(n):
    return _contracts(n, lambda x: x + 1, sum)


def _counter():
    value = 0
    def tick():
        nonlocal value
        value += 1
        return value
    return tick


def handled_d1(n):
    tick = _counter()
    acc = 0
    for _ in range(n):
        acc += tick() % 7
    return acc


handled_d4 = handled_d16 = handled_d1


def handled_fwd(n):
    outer = _counter()
    tick = lambda: outer()
    acc = 0
    for _ in range(n):
        acc += tick() % 7
    return acc


def handled_reentry(n):
    from functools import reduce
    tick = _counter()
    return reduce(lambda acc, _: acc + tick() % 7, list(range(n)), 0)


def handled_suspend(n):
    async def workload():
        tick = _counter()
        acc = elapsed = 0
        for i in range(n):
            acc += tick() % 7
            if i == n // 2:
                await asyncio.sleep(0)
                elapsed += 1
        return acc + elapsed
    return asyncio.run(workload())

# Separate file retains the third-party DeltaBlue provenance.
from deltablue import run as deltablue
