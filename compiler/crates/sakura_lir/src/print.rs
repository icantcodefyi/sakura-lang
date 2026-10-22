//! A stable text form of LIR for snapshots and debugging.
//!
//! ```text
//! fn apply_twice#3 top -> i64 ; main::closures, frame 2 slots [%0 word*, %3 i64]
//!   b0(%0 f: ptr, %1 x: i64):
//!     %2 box: word = box %1
//!     dup %0
//!     call %0(%2) -> b1(%0)
//! ```
//!
//! A `*` after a frame slot marks its pointer-bitmap bit; `[handle, try]` after a block header
//! lists the regions open at entry.

use std::fmt::Write as _;

use crate::layout::{EntryKind, FrameDesc, SlotKind, closure_slots};
use crate::lir::*;

pub fn module(m: &Module) -> String {
    let mut out = String::new();
    layouts(m, &mut out);
    for (i, e) in m.effects.iter().enumerate() {
        let ops: Vec<String> = e
            .ops
            .iter()
            .map(|o| {
                let ps: Vec<&str> = o.sig.params.iter().map(|r| r.name()).collect();
                format!("{}({}) -> {}", o.name, ps.join(", "), o.sig.ret)
            })
            .collect();
        let _ = writeln!(out, "effect {i} {} [{}]", e.name, ops.join(", "));
    }
    for (i, c) in m.contracts.iter().enumerate() {
        let _ = writeln!(out, "contract {i} {} {:?}", c.code, c.name);
    }
    for (i, s) in m.strings.iter().enumerate() {
        let _ = writeln!(out, "string {i} {s:?}");
    }
    for (name, sig) in &m.builtins {
        let ps: Vec<&str> = sig.params.iter().map(|r| r.name()).collect();
        let _ = writeln!(out, "builtin {name}({}) -> {}", ps.join(", "), sig.ret);
    }
    if let Some(e) = m.entry {
        let _ = writeln!(out, "entry {}#{}", m.func(e).name, e.0);
    }
    for (name, f) in &m.tests {
        let _ = writeln!(out, "test {name:?} {}#{}", m.func(*f).name, f.0);
    }
    for i in 0..m.funcs.len() {
        out.push('\n');
        out.push_str(&func(m, FuncId(i as u32)));
    }
    out
}

pub fn layouts(m: &Module, out: &mut String) {
    for (i, e) in m.layouts.entries.iter().enumerate() {
        let kind = match e.kind {
            EntryKind::Record => "record",
            EntryKind::Error => "error",
            EntryKind::Variant => "variant",
            EntryKind::Tuple => "shape",
        };
        let _ = write!(out, "layout {i} {kind}");
        if !e.qualified.is_empty() {
            let _ = write!(out, " {}", e.qualified);
        }
        for r in &e.rows {
            let slots: Vec<String> = r
                .slots
                .iter()
                .enumerate()
                .map(|(k, s)| {
                    let name = r.fields.get(k).map_or(String::new(), |f| format!("{f}: "));
                    format!(
                        "{name}{}{}",
                        kind_name(s.kind),
                        if s.owns { "*" } else { "" }
                    )
                })
                .collect();
            let _ = write!(
                out,
                " | {} {}b ({})",
                r.name,
                r.size_bytes(),
                slots.join(", ")
            );
        }
        out.push('\n');
    }
}

fn kind_name(k: SlotKind) -> &'static str {
    match k {
        SlotKind::Word => "word",
        SlotKind::I64 => "i64",
        SlotKind::F64 => "f64",
        SlotKind::Bool => "bool",
    }
}

pub fn func(m: &Module, id: FuncId) -> String {
    let f = m.func(id);
    let mut out = String::new();
    let kind = match &f.kind {
        FuncKind::Top { .. } if let Some((_, g)) = m.borrowed.iter().find(|(v, _)| *v == id) => {
            format!("borrowed variant of #{}", g.0)
        }
        FuncKind::Top { export: true } => "pub top".to_owned(),
        FuncKind::Top { export: false } => "top".to_owned(),
        FuncKind::Test => "test".to_owned(),
        FuncKind::Const => "const".to_owned(),
        FuncKind::Lambda => "lambda".to_owned(),
        FuncKind::Clause { effect, op } => {
            let e = &m.effects[*effect as usize];
            format!("clause {}.{}", e.name, e.ops[*op as usize].name)
        }
        FuncKind::Adapter { target } => format!("adapter of {}#{}", m.func(*target).name, target.0),
        FuncKind::BuiltinAdapter { name } => format!("adapter of ${name}"),
    };
    let frame = FrameDesc::of(f);
    let slots: Vec<String> = frame
        .slots
        .iter()
        .map(|(v, s)| {
            format!(
                "%{} {}{}",
                v.0,
                kind_name(s.kind),
                if s.owns { "*" } else { "" }
            )
        })
        .collect();
    let caps = if f.kind.is_closure() {
        let cs: Vec<String> = closure_slots(f)
            .iter()
            .map(|s| format!("{}{}", kind_name(s.kind), if s.owns { "*" } else { "" }))
            .collect();
        format!(", captures [{}]", cs.join(", "))
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "fn {}#{} {kind} -> {} ; {}{caps}, frame {} slots [{}]",
        f.name,
        id.0,
        f.ret,
        f.module,
        frame.slots.len(),
        slots.join(", ")
    );
    for (b, block) in f.blocks.iter().enumerate() {
        let params: Vec<String> = block.params.iter().map(|&p| decl(f, p)).collect();
        let regions = if block.regions.is_empty() {
            String::new()
        } else {
            let rs: Vec<&str> = block
                .regions
                .iter()
                .map(|r| match r {
                    RegionKind::Handle => "handle",
                    RegionKind::Try => "try",
                })
                .collect();
            format!(" [{}]", rs.join(", "))
        };
        let _ = writeln!(out, "  b{b}({}){regions}:", params.join(", "));
        for i in &block.insts {
            let _ = writeln!(out, "    {}", inst(m, f, i));
        }
        let _ = writeln!(out, "    {}", term(m, &block.term));
    }
    out
}

fn decl(f: &Func, v: Var) -> String {
    let d = f.var(v);
    format!("%{} {}: {}", v.0, d.name, d.repr)
}

fn vars(vs: &[Var]) -> String {
    vs.iter()
        .map(|v| format!("%{}", v.0))
        .collect::<Vec<_>>()
        .join(", ")
}

fn edge(e: &Edge) -> String {
    format!("b{}({})", e.block.0, vars(&e.args))
}

fn layout_name(m: &Module, l: LayoutId) -> String {
    m.layouts
        .entries
        .get(l.index())
        .map_or(format!("?{}", l.0), |e| e.qualified.clone())
}

fn shape(m: &Module, s: &Shape) -> String {
    match s {
        Shape::Tuple(l) => {
            let slots: Vec<&str> = m
                .layouts
                .entries
                .get(l.index())
                .map(|e| e.rows[0].slots.iter().map(|s| kind_name(s.kind)).collect())
                .unwrap_or_default();
            format!("tuple#{}({})", l.0, slots.join(", "))
        }
        Shape::Record(l) => layout_name(m, *l),
        Shape::Variant(l, c) => {
            let ctor = m
                .layouts
                .entries
                .get(l.index())
                .and_then(|e| e.rows.get(*c as usize))
                .map_or("?", |r| r.name.as_str());
            format!("{}.{ctor}", layout_name(m, *l))
        }
        Shape::List => "list".into(),
    }
}

fn inst(m: &Module, f: &Func, i: &Inst) -> String {
    use Inst::*;
    let def = |d: &Var| decl(f, *d);
    match i {
        Const { dst, value } => {
            let v = match value {
                crate::lir::Const::I64(n) => n.to_string(),
                crate::lir::Const::F64(x) => format!("{x:?}"),
                crate::lir::Const::Bool(b) => b.to_string(),
                crate::lir::Const::Unit => "()".into(),
                crate::lir::Const::Str(s) => format!("str {s}"),
                crate::lir::Const::Nullary(l, c) => {
                    format!("static {}", shape(m, &Shape::Variant(*l, *c)))
                }
                crate::lir::Const::Func(g) => {
                    format!("static closure {}#{}", m.func(*g).name, g.0)
                }
            };
            format!("{} = const {v}", def(dst))
        }
        Prim { dst, op, args } => {
            let name = match op {
                crate::lir::PrimOp::Cmp(c, k) => format!("{c:?}.{k:?}").to_lowercase(),
                other => format!("{other:?}").to_lowercase(),
            };
            format!("{} = {name} {}", def(dst), vars(args))
        }
        Box { dst, src } => format!("{} = box %{}", def(dst), src.0),
        Unbox { dst, src } => format!("{} = unbox %{}", def(dst), src.0),
        Tag { dst, src, of } => {
            let of = match of {
                TagOf::Ctor(l) => format!("ctor {}", layout_name(m, *l)),
                TagOf::Member(ls) => {
                    let ns: Vec<String> = ls.iter().map(|l| layout_name(m, *l)).collect();
                    format!("member [{}]", ns.join(", "))
                }
            };
            format!("{} = tag %{} {of}", def(dst), src.0)
        }
        Alloc {
            dst,
            shape: s,
            args,
            reuse,
        } => {
            let into = reuse
                .map(|t| format!(" reuse %{}", t.0))
                .unwrap_or_default();
            format!("{} = alloc {}({}){into}", def(dst), shape(m, s), vars(args))
        }
        Proj {
            dst,
            src,
            shape: s,
            index,
        } => format!("{} = proj %{}.{index} {}", def(dst), src.0, shape(m, s)),
        TupleGet { dst, src, index } => format!("{} = tuple_get %{}.{index}", def(dst), src.0),
        MakeClosure {
            dst,
            func,
            captures,
        } => format!(
            "{} = closure {}#{}[{}]",
            def(dst),
            m.func(*func).name,
            func.0,
            vars(captures)
        ),
        LoadCapture { dst, env, index } => {
            format!("{} = capture %{}.{index}", def(dst), env.0)
        }
        MakeHandler {
            dst,
            effect,
            row,
            state,
            clauses,
        } => format!(
            "{} = handler {} row {} state({}) clauses({})",
            def(dst),
            m.effects[*effect as usize].name,
            row.0,
            vars(state),
            vars(clauses)
        ),
        MakeCallback { dst, f: g } => format!("{} = callback %{}", def(dst), g.0),
        StateGet {
            dst,
            inst,
            row,
            index,
        } => format!(
            "{} = state_get %{}[{index}] row {}",
            def(dst),
            inst.0,
            row.0
        ),
        StateSet {
            inst,
            row,
            index,
            value,
        } => format!(
            "state_set %{}[{index}] row {} = %{}",
            inst.0, row.0, value.0
        ),
        Check {
            contract,
            cond,
            blame,
        } => format!(
            "check {} %{}{} ; {}",
            m.contracts[*contract as usize].code,
            cond.0,
            blame.map_or(String::new(), |(v, j)| format!(" blame %{}+{j}", v.0)),
            m.contracts[*contract as usize].name
        ),
        Builtin { dst, name, args } => format!("{} = ${name}({})", def(dst), vars(args)),
        HandlePush { handler, act } => match act {
            Some(a) => format!("{} = handle_push %{}", def(a), handler.0),
            None => format!("handle_push %{}", handler.0),
        },
        Bound {
            dst,
            act,
            effect,
            op,
        } => {
            let e = &m.effects[*effect as usize];
            format!(
                "{} = bound %{} {}.{}",
                def(dst),
                act.0,
                e.name,
                e.ops[*op as usize].name
            )
        }
        HandlePop => "handle_pop".into(),
        TryPush {
            catch,
            carry,
            exits,
        } => {
            let ex = if exits.handles + exits.trys > 0 {
                format!(" exits {}h {}t", exits.handles, exits.trys)
            } else {
                String::new()
            };
            format!("try_push -> b{}(err, {}){ex}", catch.0, vars(carry))
        }
        TryPop => "try_pop".into(),
        Dup(v) => format!("dup %{}", v.0),
        DupCandidate(v) => format!("dup.candidate %{}", v.0),
        Lend { dst, src } => format!("{} = lend %{}", def(dst), src.0),
        Drop(v) => format!("drop %{}", v.0),
        Hole { id } => format!("hole ?{id}"),
        Unsupported { what } => format!("unsupported {what:?}"),
    }
}

fn term(m: &Module, t: &Term) -> String {
    match t {
        Term::Return(v) => format!("ret %{}", v.0),
        Term::Jump(e) => format!("jump {}", edge(e)),
        Term::If { cond, then, els } => {
            format!("if %{} then {} else {}", cond.0, edge(then), edge(els))
        }
        Term::Switch { tag, arms, default } => {
            let mut s = format!("switch %{}", tag.0);
            for (k, e) in arms {
                let _ = write!(s, " {k} => {}", edge(e));
            }
            if let Some(d) = default {
                let _ = write!(s, " _ => {}", edge(d));
            }
            s
        }
        Term::Call { callee, args, cont } => {
            let c = match callee {
                Callee::Direct(f) => format!("{}#{}", m.func(*f).name, f.0),
                Callee::Closure(v) => format!("%{}", v.0),
                Callee::Builtin(n) => format!("${n}"),
            };
            format!("call {c}({}) -> {}", vars(args), edge(cont))
        }
        Term::Perform {
            effect,
            op,
            args,
            cont,
        } => {
            let e = &m.effects[*effect as usize];
            format!(
                "perform {}.{}({}) -> {}",
                e.name,
                e.ops[*op as usize].name,
                vars(args),
                edge(cont)
            )
        }
        Term::Fail(v) => format!("fail %{}", v.0),
    }
}
