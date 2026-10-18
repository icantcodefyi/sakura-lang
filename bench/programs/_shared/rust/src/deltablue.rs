// DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE.
// Arena indices represent the JS object graph without reference-count cycles.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Edit,
    Stay,
    Equality,
    Scale(usize, usize),
}
#[derive(Clone, Copy)]
struct Constraint {
    kind: Kind,
    strength: i64,
    v1: usize,
    v2: usize,
    dir: i8,
    satisfied: bool,
}
impl Constraint {
    fn unary(self) -> bool {
        matches!(self.kind, Kind::Edit | Kind::Stay)
    }
    fn satisfied(self) -> bool {
        if self.unary() {
            self.satisfied
        } else {
            self.dir != 0
        }
    }
    fn output(self) -> usize {
        if self.unary() || self.dir != 1 {
            self.v1
        } else {
            self.v2
        }
    }
    fn inputs(self) -> Vec<usize> {
        let primary = if self.dir == 1 { self.v1 } else { self.v2 };
        match self.kind {
            Kind::Edit | Kind::Stay => vec![],
            Kind::Equality => vec![primary],
            Kind::Scale(s, o) => vec![primary, s, o],
        }
    }
    fn attached(self) -> Vec<usize> {
        match self.kind {
            Kind::Edit | Kind::Stay => vec![self.v1],
            Kind::Equality => vec![self.v1, self.v2],
            Kind::Scale(s, o) => vec![self.v1, self.v2, s, o],
        }
    }
    fn unsatisfy(&mut self) {
        self.dir = 0;
        self.satisfied = false;
    }
}
struct Variable {
    value: i64,
    constraints: Vec<usize>,
    determined: Option<usize>,
    walk: i64,
    stay: bool,
    mark: i64,
}
struct Planner {
    vars: Vec<Variable>,
    cons: Vec<Constraint>,
    mark: i64,
}
impl Planner {
    fn new() -> Self {
        Self {
            vars: vec![],
            cons: vec![],
            mark: 1,
        }
    }
    fn var(&mut self, value: i64) -> usize {
        let id = self.vars.len();
        self.vars.push(Variable {
            value,
            constraints: vec![],
            determined: None,
            walk: 10000,
            stay: true,
            mark: 0,
        });
        id
    }
    fn new_mark(&mut self) -> i64 {
        self.mark += 1;
        self.mark
    }
    fn add(&mut self, kind: Kind, v1: usize, v2: usize, strength: i64) -> usize {
        let id = self.cons.len();
        let c = Constraint {
            kind,
            v1,
            v2,
            strength,
            dir: 0,
            satisfied: false,
        };
        self.cons.push(c);
        for v in c.attached() {
            self.vars[v].constraints.push(id);
        }
        self.incremental_add(id);
        id
    }
    fn remove_graph(&mut self, id: usize) {
        for v in self.cons[id].attached() {
            self.vars[v].constraints.retain(|&c| c != id);
            if self.vars[v].determined == Some(id) {
                self.vars[v].determined = None;
            }
        }
        self.cons[id].unsatisfy();
    }
    fn choose(&mut self, id: usize, mark: i64) {
        let c = self.cons[id];
        let v1 = &self.vars[c.v1];
        let v2 = &self.vars[c.v2];
        if c.unary() {
            self.cons[id].satisfied = v1.mark != mark && c.strength < v1.walk;
            return;
        }
        self.cons[id].dir = if v1.mark == mark {
            if v2.mark != mark && c.strength < v2.walk {
                1
            } else {
                0
            }
        } else if v2.mark == mark {
            if v1.mark != mark && c.strength < v1.walk {
                -1
            } else {
                0
            }
        } else if v1.walk > v2.walk {
            if c.strength < v1.walk { -1 } else { 0 }
        } else if c.strength < v2.walk {
            1
        } else {
            0
        };
    }
    fn execute(&mut self, id: usize) {
        let c = self.cons[id];
        let out = c.output();
        let value = match c.kind {
            Kind::Edit | Kind::Stay => return,
            Kind::Equality => self.vars[if c.dir == 1 { c.v1 } else { c.v2 }].value,
            Kind::Scale(s, o) if c.dir == 1 => {
                self.vars[c.v1].value * self.vars[s].value + self.vars[o].value
            }
            Kind::Scale(s, o) => (self.vars[c.v2].value - self.vars[o].value) / self.vars[s].value,
        };
        self.vars[out].value = value;
    }
    fn recalculate(&mut self, id: usize) {
        let c = self.cons[id];
        let out = c.output();
        let (walk, stay) = if c.unary() {
            (c.strength, c.kind != Kind::Edit)
        } else {
            let input = &self.vars[if c.dir == 1 { c.v1 } else { c.v2 }];
            let stay = match c.kind {
                Kind::Scale(s, o) => input.stay && self.vars[s].stay && self.vars[o].stay,
                _ => input.stay,
            };
            (c.strength.max(input.walk), stay)
        };
        self.vars[out].walk = walk;
        self.vars[out].stay = stay;
        if stay {
            self.execute(id);
        }
    }
    fn consuming(&self, v: usize) -> Vec<usize> {
        self.vars[v]
            .constraints
            .iter()
            .copied()
            .filter(|&c| self.vars[v].determined != Some(c) && self.cons[c].satisfied())
            .collect()
    }
    fn satisfy(&mut self, id: usize, mark: i64) -> Option<usize> {
        self.choose(id, mark);
        let c = self.cons[id];
        if !c.satisfied() {
            assert!(
                c.strength != -800,
                "Could not satisfy a required constraint"
            );
            return None;
        }
        for v in c.inputs() {
            self.vars[v].mark = mark;
        }
        let out = c.output();
        let overridden = self.vars[out].determined;
        if let Some(old) = overridden {
            self.cons[old].unsatisfy();
        }
        self.vars[out].determined = Some(id);
        let mut todo = vec![id];
        let mut head = 0;
        while head < todo.len() {
            let d = todo[head];
            head += 1;
            let output = self.cons[d].output();
            if self.vars[output].mark == mark {
                self.incremental_remove(id);
                panic!("Cycle encountered");
            }
            self.recalculate(d);
            todo.extend(self.consuming(output));
        }
        self.vars[out].mark = mark;
        overridden
    }
    fn incremental_add(&mut self, id: usize) {
        let mark = self.new_mark();
        let mut overridden = self.satisfy(id, mark);
        while let Some(c) = overridden {
            overridden = self.satisfy(c, mark);
        }
    }
    fn incremental_remove(&mut self, id: usize) {
        let out = self.cons[id].output();
        self.cons[id].unsatisfy();
        self.remove_graph(id);
        self.vars[out].determined = None;
        self.vars[out].walk = 10000;
        self.vars[out].stay = true;
        let mut unsatisfied = vec![];
        let mut todo = vec![out];
        let mut head = 0;
        while head < todo.len() {
            let v = todo[head];
            head += 1;
            for &c in &self.vars[v].constraints {
                if !self.cons[c].satisfied() {
                    unsatisfied.push(c);
                }
            }
            for c in self.consuming(v) {
                self.recalculate(c);
                todo.push(self.cons[c].output());
            }
        }
        unsatisfied.sort_by_key(|&c| self.cons[c].strength);
        for c in unsatisfied {
            self.incremental_add(c);
        }
    }
    fn destroy(&mut self, id: usize) {
        if self.cons[id].satisfied() {
            self.incremental_remove(id);
        }
        self.remove_graph(id);
    }
    fn plan(&mut self, constraints: &[usize]) -> Vec<usize> {
        let mut todo: Vec<usize> = constraints
            .iter()
            .copied()
            .filter(|&c| self.cons[c].kind == Kind::Edit && self.cons[c].satisfied())
            .collect();
        let mark = self.new_mark();
        let mut plan = vec![];
        let mut head = 0;
        while head < todo.len() {
            let id = todo[head];
            head += 1;
            let c = self.cons[id];
            let out = c.output();
            let input = &self.vars[if c.dir == 1 { c.v1 } else { c.v2 }];
            let known = c.unary() || input.mark == mark || input.stay || input.determined.is_none();
            if self.vars[out].mark != mark && known {
                plan.push(id);
                self.vars[out].mark = mark;
                todo.extend(self.consuming(out));
            }
        }
        plan
    }
    fn execute_plan(&mut self, plan: &[usize]) {
        for &c in plan {
            self.execute(c);
        }
    }
    fn change(&mut self, v: usize, value: i64) {
        let edit = self.add(Kind::Edit, v, v, -400);
        let plan = self.plan(&[edit]);
        for _ in 0..10 {
            self.vars[v].value = value;
            self.execute_plan(&plan);
        }
        self.destroy(edit);
    }
}
pub fn run(n: i64) -> i64 {
    let n = n as usize;
    let mut p = Planner::new();
    let vars: Vec<usize> = (0..=n).map(|_| p.var(0)).collect();
    for i in 0..n {
        p.add(Kind::Equality, vars[i], vars[i + 1], -800);
    }
    p.add(Kind::Stay, vars[n], vars[n], -200);
    let edit = p.add(Kind::Edit, vars[0], vars[0], -400);
    let plan = p.plan(&[edit]);
    let mut chain = 0;
    for i in 0..100 {
        p.vars[vars[0]].value = i;
        p.execute_plan(&plan);
        assert_eq!(p.vars[vars[n]].value, i, "Chain test failed");
        chain += p.vars[vars[n]].value;
    }
    p.destroy(edit);
    chain += plan.len() as i64;
    if n == 0 {
        return chain;
    }
    let mut p = Planner::new();
    let scale = p.var(10);
    let offset = p.var(1000);
    let mut dests = vec![];
    let (mut src, mut dst) = (0, 0);
    for i in 1..=n {
        src = p.var(i as i64);
        dst = p.var(i as i64);
        dests.push(dst);
        p.add(Kind::Stay, src, src, 0);
        p.add(Kind::Scale(scale, offset), src, dst, -800);
    }
    p.change(src, 17);
    assert_eq!(p.vars[dst].value, 1170, "Projection test 1 failed");
    p.change(dst, 1050);
    assert_eq!(p.vars[src].value, 5, "Projection test 2 failed");
    p.change(scale, 5);
    for (i, &d) in dests.iter().take(n - 1).enumerate() {
        assert_eq!(
            p.vars[d].value,
            (i as i64 + 1) * 5 + 1000,
            "Projection test 3 failed"
        );
    }
    let first: i64 = dests.iter().map(|&d| p.vars[d].value).sum();
    p.change(offset, 2000);
    for (i, &d) in dests.iter().take(n - 1).enumerate() {
        assert_eq!(
            p.vars[d].value,
            (i as i64 + 1) * 5 + 2000,
            "Projection test 4 failed"
        );
    }
    chain + 1170 + 5 + first + dests.iter().map(|&d| p.vars[d].value).sum::<i64>()
}
