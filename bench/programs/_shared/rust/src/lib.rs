use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub fn main(run: impl Fn(i64) -> i64, maximum: i64) {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("values or measure");
    let mut stable = false;
    let sizes: Vec<i64> = args
        .filter_map(|a| {
            if a == "--stable" {
                stable = true;
                None
            } else {
                Some(a.parse().expect("integer size"))
            }
        })
        .collect();
    assert!(
        !sizes.is_empty() && sizes.iter().all(|&n| n >= 0 && n <= maximum),
        "size outside oracle resource bound"
    );
    if mode == "values" {
        let values: serde_json::Map<String, serde_json::Value> = sizes
            .iter()
            .map(|&n| (n.to_string(), run(n).to_string().into()))
            .collect();
        println!("{}", serde_json::json!({"values": values}));
        return;
    }
    assert!(
        mode == "measure" && sizes.len() == 1,
        "measure <n> [--stable]"
    );
    let n = sizes[0];
    let mut sink = 0;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(if stable { 2000 } else { 250 }) {
        sink = std::hint::black_box(run(std::hint::black_box(n)));
    }
    let mut count = 1;
    loop {
        let start = Instant::now();
        for _ in 0..count {
            sink = std::hint::black_box(run(std::hint::black_box(n)));
        }
        if start.elapsed() >= Duration::from_millis(30) || count >= 1 << 20 {
            break;
        }
        count *= 2;
    }
    for _ in 0..9 {
        let start = Instant::now();
        for _ in 0..count {
            sink = std::hint::black_box(run(std::hint::black_box(n)));
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(count);
        println!(
            "{}",
            serde_json::json!({"n": n, "ms": ms, "checksum": sink.to_string()})
        );
    }
}

pub fn fib(n: i64) -> i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
pub fn countdown(n: i64) -> i64 {
    let mut acc = 0;
    for i in (1..=n).rev() {
        acc += i;
    }
    let mut even = true;
    for _ in (1..=n).rev() {
        even = !even;
    }
    acc + i64::from(even)
}
pub fn tail_clique(mut n: i64) -> i64 {
    enum State {
        A,
        B,
        C,
    }
    let mut state = State::A;
    let mut acc = 0;
    while n != 0 {
        let (next, weight) = match state {
            State::A => {
                if n % 2 == 0 {
                    (State::B, 1)
                } else {
                    (State::C, 2)
                }
            }
            State::B => {
                if n % 3 == 0 {
                    (State::A, 3)
                } else {
                    (State::C, 4)
                }
            }
            State::C => {
                if n % 5 == 0 {
                    (State::A, 5)
                } else {
                    (State::B, 6)
                }
            }
        };
        n -= 1;
        acc += weight;
        state = next;
    }
    acc
}
pub fn sieve(n: i64) -> i64 {
    let n = usize::try_from(n).expect("size");
    let mut flags: Vec<bool> = (0..n).map(|i| i >= 2).collect();
    let mut p = 2usize;
    while p * p < n {
        if flags[p] {
            for j in (p * p..n).step_by(p) {
                flags[j] = false;
            }
        }
        p += 1;
    }
    flags.iter().filter(|&&f| f).count() as i64
}
#[expect(
    clippy::unnecessary_fold,
    reason = "Preserve the specified map/filter/fold stages."
)]
pub fn pipeline(n: i64) -> i64 {
    let xs: Vec<i64> = (0..n).collect();
    let ys: Vec<i64> = xs.iter().map(|x| x * 7 % 1000).collect();
    let zs: Vec<i64> = ys.iter().copied().filter(|y| y % 2 == 0).collect();
    zs.iter().fold(0, |a, z| a + z)
}
struct Binary {
    l: Option<Box<Binary>>,
    r: Option<Box<Binary>>,
}
fn bottom_up(d: i64) -> Box<Binary> {
    Box::new(if d > 0 {
        Binary {
            l: Some(bottom_up(d - 1)),
            r: Some(bottom_up(d - 1)),
        }
    } else {
        Binary { l: None, r: None }
    })
}
fn check(t: &Binary) -> i64 {
    1 + t.l.as_ref().map_or(0, |l| check(l)) + t.r.as_ref().map_or(0, |r| check(r))
}
pub fn binary_trees(n: i64) -> i64 {
    let maximum = n.max(6);
    let mut total = check(&bottom_up(maximum + 1));
    let long_lived = bottom_up(maximum);
    for d in (4..=maximum).step_by(2) {
        for _ in 0..(1i64 << (maximum - d + 4)) {
            total += check(&bottom_up(d));
        }
    }
    total + check(&long_lived)
}
type Tree = Option<Rc<Node>>;
struct Node {
    red: bool,
    l: Tree,
    k: i64,
    r: Tree,
}
fn node(red: bool, l: Tree, k: i64, r: Tree) -> Tree {
    Some(Rc::new(Node { red, l, k, r }))
}
fn red(t: &Tree) -> bool {
    t.as_ref().is_some_and(|t| t.red)
}
fn balance(c: bool, l: Tree, k: i64, r: Tree) -> Tree {
    if !c {
        if red(&l) {
            let a = l.as_ref().unwrap();
            if red(&a.l) {
                let b = a.l.as_ref().unwrap();
                return node(
                    true,
                    node(false, b.l.clone(), b.k, b.r.clone()),
                    a.k,
                    node(false, a.r.clone(), k, r),
                );
            }
            if red(&a.r) {
                let b = a.r.as_ref().unwrap();
                return node(
                    true,
                    node(false, a.l.clone(), a.k, b.l.clone()),
                    b.k,
                    node(false, b.r.clone(), k, r),
                );
            }
        }
        if red(&r) {
            let a = r.as_ref().unwrap();
            if red(&a.l) {
                let b = a.l.as_ref().unwrap();
                return node(
                    true,
                    node(false, l, k, b.l.clone()),
                    b.k,
                    node(false, b.r.clone(), a.k, a.r.clone()),
                );
            }
            if red(&a.r) {
                let b = a.r.as_ref().unwrap();
                return node(
                    true,
                    node(false, l, k, a.l.clone()),
                    a.k,
                    node(false, b.l.clone(), b.k, b.r.clone()),
                );
            }
        }
    }
    node(c, l, k, r)
}
fn ins(t: &Tree, k: i64) -> Tree {
    match t {
        None => node(true, None, k, None),
        Some(a) if k < a.k => balance(a.red, ins(&a.l, k), a.k, a.r.clone()),
        Some(a) if k > a.k => balance(a.red, a.l.clone(), a.k, ins(&a.r, k)),
        _ => t.clone(),
    }
}
fn member(t: &Tree, k: i64) -> bool {
    let mut t = t;
    while let Some(a) = t {
        if k < a.k {
            t = &a.l;
        } else if k > a.k {
            t = &a.r;
        } else {
            return true;
        }
    }
    false
}
fn size(t: &Tree) -> i64 {
    t.as_ref().map_or(0, |a| size(&a.l) + 1 + size(&a.r))
}
fn black_height(t: &Tree) -> i64 {
    t.as_ref()
        .map_or(0, |a| i64::from(!a.red) + black_height(&a.l))
}
pub fn rbtree(n: i64) -> i64 {
    let mut tree = None;
    let mut kept = Vec::new();
    for i in 0..n {
        let a = ins(&tree, i * 7919 % (2 * n + 1)).unwrap();
        tree = node(false, a.l.clone(), a.k, a.r.clone());
        if i % 100 == 0 {
            kept.push(tree.clone());
        }
    }
    let mut total = size(&tree);
    for (j, t) in kept.iter().enumerate() {
        total += black_height(t);
        if member(t, j as i64 * 3) {
            total += 1;
        }
    }
    total
}
pub fn map_set(n: i64) -> i64 {
    let (mut mi, mut ms, mut mt, mut set) = (
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashSet::new(),
    );
    for i in 0..n {
        mi.insert(i, i % 1000);
        ms.insert(format!("k{i}"), i % 1000);
        mt.insert((i % 100, i / 100), i % 7);
        set.insert(i % 97);
    }
    let snapshot = mi.clone();
    let mut found = 0;
    for i in 0..2 * n {
        found += mi.get(&i).copied().unwrap_or(0);
        found += ms.get(&format!("k{i}")).copied().unwrap_or(0);
        found += mt.get(&(i % 100, i / 100)).copied().unwrap_or(0);
    }
    for i in (0..n).step_by(2) {
        *mi.entry(i).or_default() += 7;
        *ms.entry(format!("k{i}")).or_default() += 7;
    }
    for i in (0..n).step_by(3) {
        mi.remove(&i);
        ms.remove(&format!("k{i}"));
        mt.remove(&(i % 100, i / 100));
    }
    // Original keys are visited in insertion order, independent of hash randomisation.
    let mut total = 0;
    for i in 0..n {
        total += mi.get(&i).copied().unwrap_or(0);
    }
    for i in 0..n {
        total += ms.get(&format!("k{i}")).copied().unwrap_or(0);
    }
    for i in 0..n {
        total += mt.get(&(i % 100, i / 100)).copied().unwrap_or(0);
    }
    let mut snap = 0;
    for i in 0..n {
        snap += snapshot[&i];
    }
    found + total + snap + (mi.len() + ms.len() + mt.len() + snapshot.len() + set.len()) as i64
}
#[derive(Clone, Copy)]
enum Shape {
    Circle(i64),
    Rect(i64, i64),
    Square(i64),
}
#[derive(Clone, Copy)]
struct Item {
    id: i64,
    shape: Shape,
    tag: Option<i64>,
}
fn area(s: Shape) -> i64 {
    match s {
        Shape::Circle(r) => 3 * r * r,
        Shape::Rect(w, h) => w * h,
        Shape::Square(a) => a * a,
    }
}
fn grow(it: &Item) -> Item {
    let shape = match it.shape {
        Shape::Circle(r) => Shape::Rect(r, r + 1),
        Shape::Rect(w, h) => Shape::Square((w + h) % 100),
        Shape::Square(a) => Shape::Circle(a % 50 + 1),
    };
    let tag = match it.tag {
        None => Some(0),
        Some(t) if t > 5 => None,
        Some(t) => Some(t + 1),
    };
    Item { shape, tag, ..*it }
}
pub fn shapes(n: i64) -> i64 {
    let mut seed = 42;
    let mut items = Vec::new();
    for id in 0..n {
        seed = seed * 48271 % 2147483647;
        let k = seed % 100;
        let shape = if k < 33 {
            Shape::Circle(k)
        } else if k < 66 {
            Shape::Rect(k, k % 7 + 1)
        } else {
            Shape::Square(k % 13)
        };
        items.push(Item {
            id,
            shape,
            tag: if k % 4 == 0 { None } else { Some(k % 9) },
        });
    }
    let mut total = 0;
    for _ in 0..5 {
        items = items.iter().map(grow).collect();
        for it in &items {
            total += area(it.shape) + it.tag.unwrap_or(0) + it.id % 3;
        }
    }
    total
}
pub fn tokenizer(n: i64) -> i64 {
    let words = [
        "alpha",
        "naïve",
        "日本語",
        "🌸",
        "x1",
        "Grüße",
        "ok",
        "mañana",
    ];
    let mut s = String::new();
    for i in 0..n {
        s = format!(
            "{s}{}{}",
            words[(i % 8) as usize],
            if i % 5 == 4 { ", " } else { " " }
        );
    }
    let (mut tokens, mut bytes, mut scalars, mut commas) = (0, 0, 0, 0);
    for t in s.split(' ') {
        if !t.is_empty() {
            tokens += 1;
            bytes += t.len() as i64;
            scalars += t.chars().count() as i64;
            if t.ends_with(',') {
                commas += 1;
            }
        }
    }
    tokens + 3 * bytes + 7 * scalars + 11 * commas
}
pub fn json_validate(n: i64) -> i64 {
    let mut lines = Vec::new();
    for i in 0..n {
        let name = if i % 17 == 0 {
            String::new()
        } else {
            format!("user{i}")
        };
        let age = if i % 13 == 0 { 200 } else { i % 90 };
        lines.push(format!(
            "{{\"id\": {i}, \"name\": \"{name}\", \"age\": {age}}}"
        ));
    }
    let text = lines.join("\n");
    let (mut valid, mut bytes, mut ids, mut invalid) = (0, 0, 0, 0);
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            invalid += 1;
            continue;
        };
        match (
            value["id"].as_f64(),
            value["name"].as_str(),
            value["age"].as_f64(),
        ) {
            (Some(id), Some(name), Some(age))
                if id.is_finite()
                    && id.fract() == 0.0
                    && id.abs() <= 9_007_199_254_740_991.0
                    && !name.is_empty()
                    && name.len() <= 32
                    && age.fract() == 0.0
                    && (0.0..=150.0).contains(&age) =>
            {
                valid += 1;
                let id = id as i64;
                let age = age as i64;
                let name = serde_json::to_string(name).unwrap();
                let encoded = format!("{{\"id\":{id},\"name\":{name},\"age\":{age}}}");
                bytes += encoded.len() as i64;
                ids += id;
            }
            _ => invalid += 1,
        }
    }
    valid + bytes + ids + 1000 * invalid
}
fn a(i: usize, j: usize) -> f64 {
    1.0 / (((i + j) * (i + j + 1)) as f64 / 2.0 + i as f64 + 1.0)
}
fn av(v: &[f64], transpose: bool) -> Vec<f64> {
    (0..v.len())
        .map(|i| {
            let mut sum = 0.0;
            for (j, x) in v.iter().enumerate() {
                sum += if transpose { a(j, i) } else { a(i, j) } * x;
            }
            sum
        })
        .collect()
}
fn atav(v: &[f64]) -> Vec<f64> {
    av(&av(v, false), true)
}
pub fn spectral_norm(n: i64) -> i64 {
    if n == 0 {
        return 0;
    }
    let mut u = vec![1.0; n as usize];
    let mut v = u.clone();
    for _ in 0..10 {
        v = atav(&u);
        u = atav(&v);
    }
    let (mut vbv, mut vv) = (0.0, 0.0);
    for i in 0..n as usize {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    ((vbv / vv).sqrt() * 1e9).round() as i64
}
type Body = [f64; 7];
fn planet(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, m: f64) -> Body {
    [
        x,
        y,
        z,
        vx * 365.24,
        vy * 365.24,
        vz * 365.24,
        m * (4.0 * std::f64::consts::PI * std::f64::consts::PI),
    ]
}
#[expect(
    clippy::excessive_precision,
    reason = "Keep the source initial conditions verbatim across ports."
)]
pub fn nbody(n: i64) -> i64 {
    let solar = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
    let mut bs = [
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, solar],
        planet(
            4.8414314424647209,
            -1.16032004402742839,
            -1.03622044471123109e-1,
            1.66007664274403694e-3,
            7.69901118419740425e-3,
            -6.90460016972063023e-5,
            9.54791938424326609e-4,
        ),
        planet(
            8.34336671824457987,
            4.12479856412430479,
            -4.03523417114321381e-1,
            -2.76742510726862411e-3,
            4.99852801234917238e-3,
            2.30417297573763929e-5,
            2.85885980666130812e-4,
        ),
        planet(
            1.2894369562139131e1,
            -1.51111514016986312e1,
            -2.23307578892655734e-1,
            2.96460137564761618e-3,
            2.3784717395948095e-3,
            -2.96589568540237556e-5,
            4.36624404335156298e-5,
        ),
        planet(
            1.53796971148509165e1,
            -2.59193146099879641e1,
            1.79258772950371181e-1,
            2.68067772490389322e-3,
            1.62824170038242295e-3,
            -9.5159225451971587e-5,
            5.15138902046611451e-5,
        ),
    ];
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for b in &bs {
        px += b[3] * b[6];
        py += b[4] * b[6];
        pz += b[5] * b[6];
    }
    bs[0][3] = 0.0 - px / solar;
    bs[0][4] = 0.0 - py / solar;
    bs[0][5] = 0.0 - pz / solar;
    for _ in 0..n {
        for i in 0..5 {
            for j in i + 1..5 {
                let dx = bs[i][0] - bs[j][0];
                let dy = bs[i][1] - bs[j][1];
                let dz = bs[i][2] - bs[j][2];
                let d2 = dx * dx + dy * dy + dz * dz;
                let mag = 0.01 / (d2 * d2.sqrt());
                bs[i][3] -= dx * bs[j][6] * mag;
                bs[i][4] -= dy * bs[j][6] * mag;
                bs[i][5] -= dz * bs[j][6] * mag;
                bs[j][3] += dx * bs[i][6] * mag;
                bs[j][4] += dy * bs[i][6] * mag;
                bs[j][5] += dz * bs[i][6] * mag;
            }
        }
        for b in &mut bs {
            b[0] += 0.01 * b[3];
            b[1] += 0.01 * b[4];
            b[2] += 0.01 * b[5];
        }
    }
    let mut energy = 0.0;
    for i in 0..5 {
        let b = bs[i];
        energy += 0.5 * b[6] * (b[3] * b[3] + b[4] * b[4] + b[5] * b[5]);
        for c in &bs[i + 1..] {
            let dx = b[0] - c[0];
            let dy = b[1] - c[1];
            let dz = b[2] - c[2];
            energy -= b[6] * c[6] / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    (energy * 1e9).round() as i64
}
fn contract_run(n: i64, checked: bool) -> i64 {
    let xs: Vec<i64> = (0..16).collect();
    let (mut i, mut acc) = (0, 0);
    while i < n {
        if checked {
            assert!(i >= 0, "D020");
        }
        let next = i + 1;
        if checked {
            assert!(next > i, "D021");
        }
        i = next;
        if i % 16 == 0 {
            if checked {
                assert!(xs.iter().all(|&x| x >= 0), "D020");
            }
            acc += xs.iter().sum::<i64>();
        }
    }
    acc + i
}
pub fn contracts(n: i64) -> i64 {
    contract_run(n, true)
}
pub fn contracts_plain(n: i64) -> i64 {
    contract_run(n, false)
}
fn counter() -> impl FnMut() -> i64 {
    let mut value = 0;
    move || {
        value += 1;
        value
    }
}
pub fn handled_d1(n: i64) -> i64 {
    let mut tick = counter();
    let mut sum = 0;
    for _ in 0..n {
        sum += tick() % 7;
    }
    sum
}
pub fn handled_d4(n: i64) -> i64 {
    handled_d1(n)
}
pub fn handled_d16(n: i64) -> i64 {
    handled_d1(n)
}
pub fn handled_fwd(n: i64) -> i64 {
    let mut outer = counter();
    let mut tick = || outer();
    let mut sum = 0;
    for _ in 0..n {
        sum += tick() % 7;
    }
    sum
}
pub fn handled_reentry(n: i64) -> i64 {
    let mut tick = counter();
    let xs: Vec<i64> = (0..n).collect();
    xs.iter().fold(0, |acc, _| acc + tick() % 7)
}
pub fn handled_suspend(n: i64) -> i64 {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    let workload = async {
        let mut tick = counter();
        let (mut acc, mut elapsed) = (0, 0);
        for i in 0..n {
            acc += tick() % 7;
            if i == n / 2 {
                let mut suspended = false;
                std::future::poll_fn(|cx| {
                    if suspended {
                        Poll::Ready(())
                    } else {
                        suspended = true;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                })
                .await;
                elapsed += 1;
            }
        }
        acc + elapsed
    };
    let mut future = std::pin::pin!(workload);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

mod deltablue;
pub use deltablue::run as deltablue;
