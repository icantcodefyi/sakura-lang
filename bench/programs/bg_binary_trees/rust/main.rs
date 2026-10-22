//! binary-trees, the Benchmarks Game's task: the stretch tree, the long-lived tree and the
//! bottom-up trees per depth, with the suite's report lines. Every node is a `Box`, leaves
//! included (their children are `None`), dropped after its check: per-node allocation with the
//! default allocator. `main output <n>` writes the program's output; run(n) is the digest of the
//! same bytes (bench/README.md).
use std::fmt::Write as _;
use std::io::Write as _;

const MAXIMUM: i64 = 21;

struct Node {
    l: Option<Box<Node>>,
    r: Option<Box<Node>>,
}

fn bottom_up(d: i64) -> Box<Node> {
    Box::new(if d > 0 {
        Node {
            l: Some(bottom_up(d - 1)),
            r: Some(bottom_up(d - 1)),
        }
    } else {
        Node { l: None, r: None }
    })
}

fn check(t: &Node) -> i64 {
    1 + t.l.as_deref().map_or(0, check) + t.r.as_deref().map_or(0, check)
}

/// What the Benchmarks Game program writes for n.
fn text(n: i64) -> String {
    let min_depth = 4;
    let max_depth = n.max(min_depth + 2);
    let stretch_depth = max_depth + 1;
    let mut out = String::new();
    let stretch = check(&bottom_up(stretch_depth));
    writeln!(
        out,
        "stretch tree of depth {stretch_depth}\t check: {stretch}"
    )
    .unwrap();
    let long_lived = bottom_up(max_depth);
    for d in (min_depth..=max_depth).step_by(2) {
        let iterations = 1i64 << (max_depth - d + min_depth);
        let mut sum = 0;
        for _ in 0..iterations {
            sum += check(&bottom_up(d));
        }
        writeln!(out, "{iterations}\t trees of depth {d}\t check: {sum}").unwrap();
    }
    let long = check(&long_lived);
    writeln!(out, "long lived tree of depth {max_depth}\t check: {long}").unwrap();
    out
}

/// h = (31·h + byte) mod 1,000,000,007 over every byte.
fn digest(bytes: &[u8]) -> i64 {
    bytes
        .iter()
        .fold(0, |h, &b| (h * 31 + b as i64) % 1_000_000_007)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "output" {
        let n: i64 = args[2].parse().expect("integer size");
        assert!((0..=MAXIMUM).contains(&n), "size outside resource bound");
        std::io::stdout().write_all(text(n).as_bytes()).unwrap();
        return;
    }
    sakura_bench_ports::main(|n| digest(text(n).as_bytes()), MAXIMUM);
}
