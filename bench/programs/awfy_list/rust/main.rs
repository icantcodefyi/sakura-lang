// List from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/list.js;
// Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). An element's `next`
// is `Option<Rc<Element>>` (null is None): tail() returns lists that share elements with its
// arguments, so elements are reference counted; AWFY's `x.next` on a list known to be non-empty
// is `next(x)`, which panics on None where JavaScript would throw. makeList builds each element
// with its tail (no element is mutated after construction). run(n) sums n benchmark() results.
use std::rc::Rc;

struct Element {
    #[allow(dead_code)]
    val: i64,
    next: Option<Rc<Element>>,
}

type List = Option<Rc<Element>>;

impl Element {
    fn length(&self) -> i64 {
        match &self.next {
            None => 1,
            Some(next) => 1 + next.length(),
        }
    }
}

fn next(x: &List) -> List {
    x.as_ref().expect("element").next.clone()
}

fn make_list(length: i64) -> List {
    if length == 0 {
        return None;
    }
    Some(Rc::new(Element {
        val: length,
        next: make_list(length - 1),
    }))
}

fn is_shorter_than(x: &List, y: &List) -> bool {
    let mut x_tail = x.as_deref();
    let mut y_tail = y.as_deref();

    while let Some(y_elem) = y_tail {
        let Some(x_elem) = x_tail else { return true };
        x_tail = x_elem.next.as_deref();
        y_tail = y_elem.next.as_deref();
    }
    false
}

fn tail(x: List, y: List, z: List) -> List {
    if is_shorter_than(&y, &x) {
        return tail(
            tail(next(&x), y.clone(), z.clone()),
            tail(next(&y), z.clone(), x.clone()),
            tail(next(&z), x, y),
        );
    }
    z
}

fn benchmark() -> i64 {
    let result = tail(make_list(15), make_list(10), make_list(6));
    result.expect("result").length()
}

fn run(n: i64) -> i64 {
    let mut sum = 0;
    for _ in 0..n {
        sum += benchmark();
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 6000);
}
