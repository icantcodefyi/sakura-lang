# DeltaBlue from Are We Fast Yet; see bench/programs/deltablue/LICENSE.
# The JS class graph is represented by arena indices, as in the Sakura port.
from dataclasses import dataclass, field


@dataclass(slots=True)
class Variable:
    value: int
    constraints: list = field(default_factory=list)
    determined: int = -1
    walk: int = 10000
    stay: bool = True
    mark: int = 0


@dataclass(slots=True)
class Constraint:
    kind: str
    strength: int
    v1: int
    v2: int
    scale: int = -1
    offset: int = -1
    direction: int = 0
    satisfied: bool = False

    def unary(self):
        return self.kind in ('edit', 'stay')

    def is_satisfied(self):
        return self.satisfied if self.unary() else self.direction != 0

    def output(self):
        return self.v1 if self.unary() or self.direction != 1 else self.v2

    def inputs(self):
        if self.unary():
            return []
        primary = self.v1 if self.direction == 1 else self.v2
        return [primary, self.scale, self.offset] if self.kind == 'scale' else [primary]

    def attached(self):
        if self.unary():
            return [self.v1]
        return [self.v1, self.v2, self.scale, self.offset] if self.kind == 'scale' else [self.v1, self.v2]

    def unsatisfy(self):
        self.direction = 0
        self.satisfied = False


class Planner:
    def __init__(self):
        self.variables = []
        self.constraints = []
        self.mark = 1

    def new_var(self, value=0):
        ident = len(self.variables)
        self.variables.append(Variable(value))
        return ident

    def new_mark(self):
        self.mark += 1
        return self.mark

    def add(self, kind, v1, v2, strength, scale=-1, offset=-1):
        cid = len(self.constraints)
        con = Constraint(kind, strength, v1, v2, scale, offset)
        self.constraints.append(con)
        for v in con.attached():
            self.variables[v].constraints.append(cid)
        self.incremental_add(cid)
        return cid

    def remove_graph(self, cid):
        con = self.constraints[cid]
        for ident in con.attached():
            v = self.variables[ident]
            v.constraints = [c for c in v.constraints if c != cid]
            if v.determined == cid:
                v.determined = -1
        con.unsatisfy()

    def choose(self, cid, mark):
        c = self.constraints[cid]
        v1, v2 = self.variables[c.v1], self.variables[c.v2]
        if c.unary():
            c.satisfied = v1.mark != mark and c.strength < v1.walk
        elif v1.mark == mark:
            c.direction = 1 if v2.mark != mark and c.strength < v2.walk else 0
        elif v2.mark == mark:
            c.direction = -1 if v1.mark != mark and c.strength < v1.walk else 0
        elif v1.walk > v2.walk:
            c.direction = -1 if c.strength < v1.walk else 0
        else:
            c.direction = 1 if c.strength < v2.walk else 0

    def execute(self, cid):
        c = self.constraints[cid]
        if c.unary():
            return
        source = self.variables[c.v1 if c.direction == 1 else c.v2]
        out = self.variables[c.output()]
        if c.kind == 'equality':
            out.value = source.value
        elif c.direction == 1:
            out.value = source.value * self.variables[c.scale].value + self.variables[c.offset].value
        else:
            numerator = source.value - self.variables[c.offset].value
            divisor = self.variables[c.scale].value
            out.value = numerator // divisor

    def recalculate(self, cid):
        c = self.constraints[cid]
        out = self.variables[c.output()]
        if c.unary():
            out.walk = c.strength
            out.stay = c.kind != 'edit'
        else:
            source = self.variables[c.v1 if c.direction == 1 else c.v2]
            out.walk = max(c.strength, source.walk)
            out.stay = source.stay
            if c.kind == 'scale':
                out.stay = out.stay and self.variables[c.scale].stay and self.variables[c.offset].stay
        if out.stay:
            self.execute(cid)

    def consuming(self, ident):
        v = self.variables[ident]
        return [cid for cid in v.constraints if cid != v.determined and self.constraints[cid].is_satisfied()]

    def satisfy(self, cid, mark):
        self.choose(cid, mark)
        c = self.constraints[cid]
        if not c.is_satisfied():
            if c.strength == -800:
                raise RuntimeError('Could not satisfy a required constraint')
            return -1
        for ident in c.inputs():
            self.variables[ident].mark = mark
        out = c.output()
        overridden = self.variables[out].determined
        if overridden >= 0:
            self.constraints[overridden].unsatisfy()
        self.variables[out].determined = cid
        todo = [cid]
        head = 0
        while head < len(todo):
            d = todo[head]
            head += 1
            output = self.constraints[d].output()
            if self.variables[output].mark == mark:
                self.incremental_remove(cid)
                raise RuntimeError('Cycle encountered')
            self.recalculate(d)
            todo.extend(self.consuming(output))
        self.variables[out].mark = mark
        return overridden

    def incremental_add(self, cid):
        mark = self.new_mark()
        overridden = self.satisfy(cid, mark)
        while overridden >= 0:
            overridden = self.satisfy(overridden, mark)

    def incremental_remove(self, cid):
        out = self.constraints[cid].output()
        self.constraints[cid].unsatisfy()
        self.remove_graph(cid)
        v = self.variables[out]
        v.determined, v.walk, v.stay = -1, 10000, True
        unsatisfied, todo, head = [], [out], 0
        while head < len(todo):
            ident = todo[head]
            head += 1
            for c in self.variables[ident].constraints:
                if not self.constraints[c].is_satisfied():
                    unsatisfied.append(c)
            for c in self.consuming(ident):
                self.recalculate(c)
                todo.append(self.constraints[c].output())
        unsatisfied.sort(key=lambda c: self.constraints[c].strength)
        for c in unsatisfied:
            self.incremental_add(c)

    def destroy(self, cid):
        if self.constraints[cid].is_satisfied():
            self.incremental_remove(cid)
        self.remove_graph(cid)

    def plan(self, constraints):
        todo = [cid for cid in constraints if self.constraints[cid].kind == 'edit' and self.constraints[cid].is_satisfied()]
        mark, plan, head = self.new_mark(), [], 0
        while head < len(todo):
            cid = todo[head]
            head += 1
            c = self.constraints[cid]
            out = c.output()
            source = self.variables[c.v1 if c.direction == 1 else c.v2]
            known = c.unary() or source.mark == mark or source.stay or source.determined < 0
            if self.variables[out].mark != mark and known:
                plan.append(cid)
                self.variables[out].mark = mark
                todo.extend(self.consuming(out))
        return plan

    def execute_plan(self, plan):
        for cid in plan:
            self.execute(cid)

    def change(self, ident, value):
        edit = self.add('edit', ident, ident, -400)
        plan = self.plan([edit])
        for _ in range(10):
            self.variables[ident].value = value
            self.execute_plan(plan)
        self.destroy(edit)


def run(n):
    p = Planner()
    variables = [p.new_var() for _ in range(n + 1)]
    for i in range(n):
        p.add('equality', variables[i], variables[i + 1], -800)
    p.add('stay', variables[n], variables[n], -200)
    edit = p.add('edit', variables[0], variables[0], -400)
    plan = p.plan([edit])
    chain = 0
    for i in range(100):
        p.variables[variables[0]].value = i
        p.execute_plan(plan)
        assert p.variables[variables[n]].value == i, 'Chain test failed'
        chain += p.variables[variables[n]].value
    p.destroy(edit)
    chain += len(plan)
    if n == 0:
        return chain
    p = Planner()
    scale, offset = p.new_var(10), p.new_var(1000)
    dests = []
    for i in range(1, n + 1):
        src, dst = p.new_var(i), p.new_var(i)
        dests.append(dst)
        p.add('stay', src, src, 0)
        p.add('scale', src, dst, -800, scale, offset)
    p.change(src, 17)
    assert p.variables[dst].value == 1170, 'Projection test 1 failed'
    p.change(dst, 1050)
    assert p.variables[src].value == 5, 'Projection test 2 failed'
    p.change(scale, 5)
    for i in range(n - 1):
        assert p.variables[dests[i]].value == (i + 1) * 5 + 1000, 'Projection test 3 failed'
    first = sum(p.variables[d].value for d in dests)
    p.change(offset, 2000)
    for i in range(n - 1):
        assert p.variables[dests[i]].value == (i + 1) * 5 + 2000, 'Projection test 4 failed'
    return chain + 1170 + 5 + first + sum(p.variables[d].value for d in dests)
