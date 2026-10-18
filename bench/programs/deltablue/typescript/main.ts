// DeltaBlue from Are We Fast Yet's JavaScript version (benchmarks/JavaScript/deltablue.js),
// itself derived from the SOM version of Mario Wolczko's Smalltalk DeltaBlue; see ../LICENSE.
// Changes: SOM's Vector is a plain array (queues walk a head index, as SOM's removeFirst is
// O(1)); the unsatisfied list is sorted strongest-first with the stable Array.prototype.sort;
// the two tests return the values they check, and run(n) adds them (projectionTest(0) is 0).
class Sym {
  declare hash: number;
  constructor(hash: number) {
    this.hash = hash;
  }

  customHash() {
    return this.hash;
  }
}

const ABSOLUTE_STRONGEST = new Sym(0);
const REQUIRED = new Sym(1);
const STRONG_PREFERRED = new Sym(2);
const PREFERRED = new Sym(3);
const STRONG_DEFAULT = new Sym(4);
const DEFAULT = new Sym(5);
const WEAK_DEFAULT = new Sym(6);
const ABSOLUTE_WEAKEST = new Sym(7);

function createStrengthTable(): Map<Sym, number> {
  const strengthTable = new Map<Sym, number>();
  strengthTable.set(ABSOLUTE_STRONGEST, -10000);
  strengthTable.set(REQUIRED, -800);
  strengthTable.set(STRONG_PREFERRED, -600);
  strengthTable.set(PREFERRED, -400);
  strengthTable.set(STRONG_DEFAULT, -200);
  strengthTable.set(DEFAULT, 0);
  strengthTable.set(WEAK_DEFAULT, 500);
  strengthTable.set(ABSOLUTE_WEAKEST, 10000);
  return strengthTable;
}

class Strength {
  declare arithmeticValue: number;
  constructor(symbolicValue: Sym) {
    const value = Strength.strengthTable.get(symbolicValue);
    if (value === undefined) throw new Error("unknown strength");
    this.arithmeticValue = value;
  }

  sameAs(s: Strength) {
    return this.arithmeticValue === s.arithmeticValue;
  }

  stronger(s: Strength) {
    return this.arithmeticValue < s.arithmeticValue;
  }

  weaker(s: Strength) {
    return this.arithmeticValue > s.arithmeticValue;
  }

  strongest(s: Strength) {
    return s.stronger(this) ? s : this;
  }

  weakest(s: Strength) {
    return s.weaker(this) ? s : this;
  }

  static of(strength: Sym): Strength {
    const value = Strength.strengthConstant.get(strength);
    if (value === undefined) throw new Error("unknown strength");
    return value;
  }

  static strengthTable = createStrengthTable();

  static createStrengthConstants() {
    const strengthConstant = new Map<Sym, Strength>();
    Strength.strengthTable.forEach((_value, key) => {
      strengthConstant.set(key, new Strength(key));
    });
    return strengthConstant;
  }

  static strengthConstant = Strength.createStrengthConstants();

  static absoluteWeakest = Strength.of(ABSOLUTE_WEAKEST);

  static required = Strength.of(REQUIRED);
}

abstract class AbstractConstraint {
  declare strength: Strength;
  abstract isSatisfied(): boolean;
  abstract addToGraph(): void;
  abstract removeFromGraph(): void;
  abstract chooseMethod(mark: number): 'forward' | 'backward' | null;
  abstract inputsDo(fn: (v: Variable) => void): void;
  abstract inputsHasOne(fn: (v: Variable) => boolean): boolean;
  abstract markUnsatisfied(): void;
  abstract getOutput(): Variable;
  abstract recalculate(): void;
  abstract execute(): void;
  constructor(strengthSym: Sym) {
    this.strength = Strength.of(strengthSym);
  }

  isInput() {
    return false;
  }

  addConstraint(planner: Planner) {
    this.addToGraph();
    planner.incrementalAdd(this);
  }

  destroyConstraint(planner: Planner) {
    if (this.isSatisfied()) {
      planner.incrementalRemove(this);
    }
    this.removeFromGraph();
  }

  inputsKnown(mark: number) {
    return !this.inputsHasOne((v) => !(v.mark === mark || v.stay || v.determinedBy === null));
  }

  satisfy(mark: number, planner: Planner): AbstractConstraint | null {
    let overridden: AbstractConstraint | null;
    this.chooseMethod(mark);

    if (this.isSatisfied()) {
      // constraint can be satisfied
      // mark inputs to allow cycle detection in addPropagate
      this.inputsDo((i) => { i.mark = mark; });

      const out = this.getOutput();
      overridden = out.determinedBy;
      if (overridden !== null) {
        overridden.markUnsatisfied();
      }
      out.determinedBy = this;
      if (!planner.addPropagate(this, mark)) {
        throw new Error('Cycle encountered');
      }
      out.mark = mark;
    } else {
      overridden = null;
      if (this.strength.sameAs(Strength.required)) {
        throw new Error('Could not satisfy a required constraint');
      }
    }
    return overridden;
  }
}

abstract class BinaryConstraint extends AbstractConstraint {
  declare v1: Variable;
  declare v2: Variable;
  declare direction: 'forward' | 'backward' | null;
  // eslint-disable-next-line no-unused-vars
  constructor(var1: Variable, var2: Variable, strength: Sym, planner: Planner) {
    super(strength);
    this.v1 = var1;
    this.v2 = var2;
    this.direction = null;
  }

  isSatisfied() {
    return this.direction !== null;
  }

  addToGraph() {
    this.v1.addConstraint(this);
    this.v2.addConstraint(this);
    this.direction = null;
  }

  removeFromGraph() {
    if (this.v1 !== null) {
      this.v1.removeConstraint(this);
    }
    if (this.v2 !== null) {
      this.v2.removeConstraint(this);
    }
    this.direction = null;
  }

  chooseMethod(mark: number) {
    if (this.v1.mark === mark) {
      if (this.v2.mark !== mark && this.strength.stronger(this.v2.walkStrength)) {
        this.direction = 'forward';
        return this.direction;
      }
      this.direction = null;
      return this.direction;
    }

    if (this.v2.mark === mark) {
      if (this.v1.mark !== mark && this.strength.stronger(this.v1.walkStrength)) {
        this.direction = 'backward';
        return this.direction;
      }
      this.direction = null;
      return this.direction;
    }

    // If we get here, neither variable is marked, so we have a choice.
    if (this.v1.walkStrength.weaker(this.v2.walkStrength)) {
      if (this.strength.stronger(this.v1.walkStrength)) {
        this.direction = 'backward';
        return this.direction;
      }
      this.direction = null;
      return this.direction;
    }
    if (this.strength.stronger(this.v2.walkStrength)) {
      this.direction = 'forward';
      return this.direction;
    }
    this.direction = null;
    return this.direction;
  }

  inputsDo(fn: (v: Variable) => void) {
    if (this.direction === 'forward') {
      fn(this.v1);
    } else {
      fn(this.v2);
    }
  }

  inputsHasOne(fn: (v: Variable) => boolean) {
    if (this.direction === 'forward') {
      return fn(this.v1);
    }
    return fn(this.v2);
  }

  markUnsatisfied() {
    this.direction = null;
  }

  getOutput() {
    return this.direction === 'forward' ? this.v2 : this.v1;
  }

  recalculate() {
    let ihn: Variable;
    let out: Variable;

    if (this.direction === 'forward') {
      ihn = this.v1; out = this.v2;
    } else {
      ihn = this.v2; out = this.v1;
    }

    out.walkStrength = this.strength.weakest(ihn.walkStrength);
    out.stay = ihn.stay;
    if (out.stay) {
      this.execute();
    }
  }
}

abstract class UnaryConstraint extends AbstractConstraint {
  declare output: Variable;
  declare satisfied: boolean;
  constructor(v: Variable, strength: Sym, planner: Planner) {
    super(strength);
    this.output = v;
    this.satisfied = false;

    this.addConstraint(planner);
  }

  isSatisfied() {
    return this.satisfied;
  }

  addToGraph() {
    this.output.addConstraint(this);
    this.satisfied = false;
  }

  removeFromGraph() {
    if (this.output !== null) {
      this.output.removeConstraint(this);
    }
    this.satisfied = false;
  }

  chooseMethod(mark: number) {
    this.satisfied = this.output.mark !== mark
      && this.strength.stronger(this.output.walkStrength);
    return null;
  }

  // eslint-disable-next-line no-unused-vars
  inputsDo(fn: (v: Variable) => void) {
    // I have no input variables
  }

  // eslint-disable-next-line no-unused-vars
  inputsHasOne(fn: (v: Variable) => boolean) {
    return false;
  }

  markUnsatisfied() {
    this.satisfied = false;
  }

  getOutput() {
    return this.output;
  }

  recalculate() {
    this.output.walkStrength = this.strength;
    this.output.stay = !this.isInput();
    if (this.output.stay) {
      this.execute(); // stay optimization
    }
  }
}

class EditConstraint extends UnaryConstraint {
  isInput() {
    return true;
  }

  execute() {}
}

class EqualityConstraint extends BinaryConstraint {
  constructor(var1: Variable, var2: Variable, strength: Sym, planner: Planner) {
    super(var1, var2, strength, planner);
    this.addConstraint(planner);
  }

  execute() {
    if (this.direction === 'forward') {
      this.v2.value = this.v1.value;
    } else {
      this.v1.value = this.v2.value;
    }
  }
}

class ScaleConstraint extends BinaryConstraint {
  declare scale: Variable;
  declare offset: Variable;
  constructor(src: Variable, scale: Variable, offset: Variable, dest: Variable, strength: Sym, planner: Planner) {
    super(src, dest, strength, planner);
    this.scale = scale;
    this.offset = offset;

    this.addConstraint(planner);
  }

  addToGraph() {
    this.v1.addConstraint(this);
    this.v2.addConstraint(this);
    this.scale.addConstraint(this);
    this.offset.addConstraint(this);
    this.direction = null;
  }

  removeFromGraph() {
    if (this.v1 !== null) { this.v1.removeConstraint(this); }
    if (this.v2 !== null) { this.v2.removeConstraint(this); }
    if (this.scale !== null) { this.scale.removeConstraint(this); }
    if (this.offset !== null) { this.offset.removeConstraint(this); }
    this.direction = null;
  }

  execute() {
    if (this.direction === 'forward') {
      this.v2.value = this.v1.value * this.scale.value + this.offset.value;
    } else {
      this.v1.value = (this.v2.value - this.offset.value) / this.scale.value;
    }
  }

  inputsDo(fn: (v: Variable) => void) {
    if (this.direction === 'forward') {
      fn(this.v1);
      fn(this.scale);
      fn(this.offset);
    } else {
      fn(this.v2);
      fn(this.scale);
      fn(this.offset);
    }
  }

  recalculate() {
    let ihn: Variable;
    let out: Variable;

    if (this.direction === 'forward') {
      ihn = this.v1; out = this.v2;
    } else {
      out = this.v1; ihn = this.v2;
    }

    out.walkStrength = this.strength.weakest(ihn.walkStrength);
    out.stay = ihn.stay && this.scale.stay && this.offset.stay;
    if (out.stay) {
      this.execute(); // stay optimization
    }
  }
}

class StayConstraint extends UnaryConstraint {
  execute() {}
}

class Variable {
  declare value: number;
  declare constraints: AbstractConstraint[];
  declare determinedBy: AbstractConstraint | null;
  declare walkStrength: Strength;
  declare stay: boolean;
  declare mark: number;
  constructor() {
    this.value = 0;
    this.constraints = [];
    this.determinedBy = null;
    this.walkStrength = Strength.absoluteWeakest;
    this.stay = true;
    this.mark = 0;
  }

  addConstraint(c: AbstractConstraint) {
    this.constraints.push(c);
  }

  removeConstraint(c: AbstractConstraint) {
    this.constraints = this.constraints.filter((x) => x !== c);
    if (this.determinedBy === c) {
      this.determinedBy = null;
    }
  }

  static value(aValue: number) {
    const v = new Variable();
    v.value = aValue;
    return v;
  }
}

class Planner {
  declare currentMark: number;
  constructor() {
    this.currentMark = 1;
  }

  newMark() {
    this.currentMark += 1;
    return this.currentMark;
  }

  incrementalAdd(c: AbstractConstraint) {
    const mark = this.newMark();
    let overridden = c.satisfy(mark, this);

    while (overridden !== null) {
      overridden = overridden.satisfy(mark, this);
    }
  }

  incrementalRemove(c: AbstractConstraint) {
    const out = c.getOutput();
    c.markUnsatisfied();
    c.removeFromGraph();

    const unsatisfied = this.removePropagateFrom(out);
    unsatisfied.forEach((u) => this.incrementalAdd(u));
  }

  extractPlanFromConstraints(constraints: AbstractConstraint[]) {
    const sources = constraints.filter((c) => c.isInput() && c.isSatisfied());

    return this.makePlan(sources);
  }

  makePlan(sources: AbstractConstraint[]) {
    const mark = this.newMark();
    const plan: AbstractConstraint[] = [];
    const todo = sources;

    for (let head = 0; head < todo.length; head += 1) {
      const c = todo[head];

      if (c.getOutput().mark !== mark && c.inputsKnown(mark)) {
        // not in plan already and eligible for inclusion
        plan.push(c);
        c.getOutput().mark = mark;
        this.addConstraintsConsumingTo(c.getOutput(), todo);
      }
    }
    return plan;
  }

  propagateFrom(v: Variable) {
    const todo: AbstractConstraint[] = [];
    this.addConstraintsConsumingTo(v, todo);

    for (let head = 0; head < todo.length; head += 1) {
      const c = todo[head];
      c.execute();
      this.addConstraintsConsumingTo(c.getOutput(), todo);
    }
  }

  addConstraintsConsumingTo(v: Variable, coll: AbstractConstraint[]) {
    const determiningC = v.determinedBy;

    v.constraints.forEach((c) => {
      if (c !== determiningC && c.isSatisfied()) {
        coll.push(c);
      }
    });
  }

  addPropagate(c: AbstractConstraint, mark: number) {
    const todo = [c];

    for (let head = 0; head < todo.length; head += 1) {
      const d = todo[head];

      if (d.getOutput().mark === mark) {
        this.incrementalRemove(c);
        return false;
      }
      d.recalculate();
      this.addConstraintsConsumingTo(d.getOutput(), todo);
    }
    return true;
  }

  change(v: Variable, newValue: number) {
    const editC = new EditConstraint(v, PREFERRED, this);
    const plan = this.extractPlanFromConstraints([editC]);

    for (let i = 0; i < 10; i += 1) {
      v.value = newValue;
      plan.forEach((c) => c.execute());
    }
    editC.destroyConstraint(this);
  }

  constraintsConsuming(v: Variable, fn: (c: AbstractConstraint) => void) {
    const determiningC = v.determinedBy;
    v.constraints.forEach((c) => {
      if (c !== determiningC && c.isSatisfied()) {
        fn(c);
      }
    });
  }

  removePropagateFrom(out: Variable) {
    const unsatisfied: AbstractConstraint[] = [];

    out.determinedBy = null;
    out.walkStrength = Strength.absoluteWeakest;
    out.stay = true;

    const todo = [out];

    for (let head = 0; head < todo.length; head += 1) {
      const v = todo[head];

      v.constraints.forEach((c) => {
        if (!c.isSatisfied()) { unsatisfied.push(c); }
      });

      this.constraintsConsuming(v, (c) => {
        c.recalculate();
        todo.push(c.getOutput());
      });
    }

    // Strongest first, stable (AWFY uses SOM's in-place sort; see ../LICENSE).
    unsatisfied.sort((c1, c2) => c1.strength.arithmeticValue - c2.strength.arithmeticValue);
    return unsatisfied;
  }

  static chainTest(n: number) {
    const planner = new Planner();
    const vars = new Array<Variable>(n + 1);

    for (let i = 0; i < n + 1; i += 1) {
      vars[i] = new Variable();
    }

    // Build chain of n equality constraints
    for (let i = 0; i < n; i += 1) {
      const v1 = vars[i];
      const v2 = vars[i + 1];
      new EqualityConstraint(v1, v2, REQUIRED, planner);
    }

    new StayConstraint(vars[n], STRONG_DEFAULT, planner);

    const editC = new EditConstraint(vars[0], PREFERRED, planner);
    const plan = planner.extractPlanFromConstraints([editC]);

    let sum = 0;
    for (let i = 0; i < 100; i += 1) {
      vars[0].value = i;
      plan.forEach((c) => c.execute());
      if (vars[n].value !== i) {
        throw new Error('Chain test failed!');
      }
      sum += vars[n].value;
    }
    editC.destroyConstraint(planner);
    return sum + plan.length;
  }

  static projectionTest(n: number) {
    if (n === 0) return 0;
    const planner = new Planner();
    const dests: Variable[] = [];
    const scale = Variable.value(10);
    const offset = Variable.value(1000);

    let src: Variable | null = null;
    let dst: Variable | null = null;

    for (let i = 1; i <= n; i += 1) {
      src = Variable.value(i);
      dst = Variable.value(i);
      dests.push(dst);
      new StayConstraint(src, DEFAULT, planner);
      new ScaleConstraint(src, scale, offset, dst, REQUIRED, planner);
    }

    if (src === null || dst === null) throw new Error("empty projection graph");
    planner.change(src, 17);
    if (dst.value !== 1170) {
      throw new Error('Projection test 1 failed!');
    }

    planner.change(dst, 1050);
    if (src.value !== 5) {
      throw new Error('Projection test 2 failed!');
    }

    planner.change(scale, 5);
    for (let i = 0; i < n - 1; i += 1) {
      if (dests[i].value !== (i + 1) * 5 + 1000) {
        throw new Error('Projection test 3 failed!');
      }
    }
    const c = dests.reduce((acc, d) => acc + d.value, 0);

    planner.change(offset, 2000);
    for (let i = 0; i < n - 1; i += 1) {
      if (dests[i].value !== (i + 1) * 5 + 2000) {
        throw new Error('Projection test 4 failed!');
      }
    }
    return 1170 + 5 + c + dests.reduce((acc, d) => acc + d.value, 0);
  }
}

export function run(n: number): number {
  return Planner.chainTest(n) + Planner.projectionTest(n);
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 10000);
