//! A stable textual form of Core for snapshots and `--dump core`.

use std::fmt::{self, Write};

use crate::ir::*;

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut p = Printer {
            m: self,
            def: None,
            out: String::new(),
            depth: 0,
        };
        p.module();
        f.write_str(&p.out)
    }
}

struct Printer<'a> {
    m: &'a Module,
    def: Option<&'a FnDef>,
    out: String,
    depth: usize,
}

impl<'a> Printer<'a> {
    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn module(&mut self) {
        let m = self.m;
        for t in &m.types {
            let params = if t.params == 0 {
                String::new()
            } else {
                let vs: Vec<String> = (0..t.params).map(|i| format!("T{i}")).collect();
                format!("<{}>", vs.join(", "))
            };
            let builtin = t
                .builtin
                .as_ref()
                .map_or(String::new(), |b| format!(" @builtin({b})"));
            let body = match &t.kind {
                TypeKind::Record { fields, error } => {
                    let fs: Vec<String> = fields
                        .iter()
                        .map(|f| format!("{}: {}", f.name, self.ty(&f.ty)))
                        .collect();
                    let kw = if *error { "error" } else { "record" };
                    format!("{kw} {{ {} }}", fs.join(", "))
                }
                TypeKind::Variant { ctors } => {
                    let cs: Vec<String> = ctors
                        .iter()
                        .map(|c| {
                            if c.fields.is_empty() {
                                c.name.clone()
                            } else {
                                let fs: Vec<String> = c.fields.iter().map(|t| self.ty(t)).collect();
                                format!("{}({})", c.name, fs.join(", "))
                            }
                        })
                        .collect();
                    cs.join(" | ")
                }
            };
            let line = format!("type {}{params}{builtin} = {body}", t.qualified_name());
            self.line(&line);
        }
        for e in &m.effects {
            self.line(&format!("effect {} {{", e.qualified_name()));
            self.depth += 1;
            for o in &e.ops {
                let ps: Vec<String> = o.params.iter().map(|t| self.ty(t)).collect();
                let fail = o
                    .fail
                    .as_ref()
                    .map_or(String::new(), |t| format!(" ! {{Fail<{}>}}", self.ty(t)));
                let line = format!(
                    "fn {}({}) -> {}{fail}",
                    o.name,
                    ps.join(", "),
                    self.ty(&o.ret)
                );
                self.line(&line);
            }
            self.depth -= 1;
            self.line("}");
        }
        for (i, c) in m.contracts.iter().enumerate() {
            let line = format!("contract #{i} {:?} {:?}", c.kind, c.name);
            self.line(&line);
        }
        for (i, d) in m.defs.iter().enumerate() {
            self.out.push('\n');
            self.def(DefId(i as u32), d);
        }
        for t in &m.tests {
            let line = format!("test {:?} = @{}", t.name, m.def(t.def).name);
            self.line(&line);
        }
        if let Some(e) = m.entry {
            let line = format!("entry @{}", m.def(e).name);
            self.line(&line);
        }
    }

    fn def(&mut self, id: DefId, d: &'a FnDef) {
        self.def = Some(d);
        let kind = match &d.kind {
            FnKind::Top { export: true } => "pub fn".to_owned(),
            FnKind::Top { export: false } => "fn".to_owned(),
            FnKind::Lambda => "lambda".to_owned(),
            FnKind::Clause { effect, op } => {
                let e = self.m.effect(*effect);
                format!("clause[{}.{}]", e.name, e.ops[*op as usize].name)
            }
            FnKind::Test => "test fn".to_owned(),
        };
        let caps = if d.captures.is_empty() {
            String::new()
        } else {
            let cs: Vec<String> = d.captures.iter().map(|l| self.decl(*l)).collect();
            format!("[{}]", cs.join(", "))
        };
        let ps: Vec<String> = d.params.iter().map(|l| self.decl(*l)).collect();
        let line = format!(
            "{kind} {}#{}{caps}({}) -> {}{} {{",
            d.name,
            id.0,
            ps.join(", "),
            self.ty(&d.ret),
            self.row_suffix(&d.row)
        );
        self.line(&line);
        self.depth += 1;
        self.expr(&d.body);
        self.depth -= 1;
        self.line("}");
    }

    fn decl(&self, l: Local) -> String {
        let d = self.def.expect("in def").local(l);
        format!("{}: {}", self.local(l), self.ty(&d.ty))
    }

    fn local(&self, l: Local) -> String {
        let name = self
            .def
            .and_then(|d| d.locals.get(l.index()))
            .map_or("?", |d| d.name.as_str());
        format!("{name}%{}", l.0)
    }

    fn atom(&self, a: &Atom) -> String {
        match a {
            Atom::Local(l) => self.local(*l),
            Atom::Lit(lit) => match lit {
                Lit::Unit => "()".into(),
                Lit::Bool(b) => b.to_string(),
                Lit::Int(i) => i.to_string(),
                Lit::Float(f) => format!("{f:?}"),
                Lit::String(s) => format!("{s:?}"),
            },
            Atom::Global(d) => format!("@{}", self.m.defs.get(d.index()).map_or("?", |d| &d.name)),
            Atom::Builtin(b) => format!("${b}"),
        }
    }

    fn atoms(&self, atoms: &[Atom]) -> String {
        atoms
            .iter()
            .map(|a| self.atom(a))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn type_name(&self, t: TypeId) -> &str {
        self.m.types.get(t.index()).map_or("?", |t| &t.name)
    }

    fn effect_name(&self, e: EffectId) -> &str {
        self.m.effects.get(e.index()).map_or("?", |e| &e.name)
    }

    fn row_suffix(&self, r: &Row) -> String {
        if r.is_pure() {
            String::new()
        } else {
            format!(" ! {}", self.row(r))
        }
    }

    fn row(&self, r: &Row) -> String {
        let mut labels: Vec<String> = r
            .effects
            .iter()
            .map(|e| self.effect_name(*e).to_owned())
            .collect();
        if let Some(t) = &r.fail {
            labels.push(format!("Fail<{}>", self.ty(t)));
        }
        let mut s = format!("{{{}", labels.join(", "));
        if let Some(v) = r.tail {
            let _ = write!(s, " | r{v}");
        }
        s.push('}');
        s
    }

    fn ty(&self, t: &Ty) -> String {
        let list = |ts: &[Ty]| ts.iter().map(|t| self.ty(t)).collect::<Vec<_>>().join(", ");
        match t {
            Ty::Unit => "Unit".into(),
            Ty::Bool => "Bool".into(),
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::String => "String".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Never => "Never".into(),
            Ty::Tuple(ts) => format!("({})", list(ts)),
            Ty::Fn(f) => format!(
                "fn({}) -> {}{}",
                list(&f.params),
                self.ty(&f.ret),
                self.row_suffix(&f.row)
            ),
            Ty::Adt(id, args) if args.is_empty() => self.type_name(*id).to_owned(),
            Ty::Adt(id, args) => format!("{}<{}>", self.type_name(*id), list(args)),
            Ty::Union(ts) => ts
                .iter()
                .map(|t| self.type_name(*t))
                .collect::<Vec<_>>()
                .join(" | "),
            Ty::List(t) => format!("List<{}>", self.ty(t)),
            Ty::Map(k, v) => format!("Map<{}, {}>", self.ty(k), self.ty(v)),
            Ty::Set(t) => format!("Set<{}>", self.ty(t)),
            Ty::Handler(e) => format!("Handler<{}>", self.effect_name(*e)),
            Ty::Instance(e) => format!("Instance<{}>", self.effect_name(*e)),
            Ty::Opaque(n) => n.clone(),
            Ty::Var(v) => format!("T{v}"),
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Let {
                local, bind, body, ..
            } => {
                let line = format!("let {} = {}", self.decl(*local), self.bind(bind));
                self.line(&line);
                self.expr(body);
            }
            Expr::Ret(a) => {
                let line = format!("ret {}", self.atom(a));
                self.line(&line);
            }
            Expr::Fail(a) => {
                let line = format!("fail {}", self.atom(a));
                self.line(&line);
            }
            Expr::Jump(l, args) => {
                let line = format!("jump j{}({})", l.0, self.atoms(args));
                self.line(&line);
            }
            Expr::LetJoin {
                label,
                params,
                join,
                body,
            } => {
                let ps: Vec<String> = params.iter().map(|l| self.decl(*l)).collect();
                self.line(&format!("join j{}({}) {{", label.0, ps.join(", ")));
                self.depth += 1;
                self.expr(join);
                self.depth -= 1;
                self.line("}");
                self.expr(body);
            }
            Expr::Switch {
                scrut,
                on,
                arms,
                default,
            } => {
                let line = format!("switch {} {{", self.atom(scrut));
                self.line(&line);
                self.depth += 1;
                for (key, arm) in arms {
                    let name = match on {
                        SwitchOn::Variant(t) => self.m.ty(*t).ctors()[*key as usize].name.clone(),
                        SwitchOn::Errors(ts) => self.type_name(ts[*key as usize]).to_owned(),
                    };
                    self.line(&format!("{name} =>"));
                    self.depth += 1;
                    self.expr(arm);
                    self.depth -= 1;
                }
                if let Some(d) = default {
                    self.line("_ =>");
                    self.depth += 1;
                    self.expr(d);
                    self.depth -= 1;
                }
                self.depth -= 1;
                self.line("}");
            }
            Expr::If { cond, then, els } => {
                let line = format!("if {} {{", self.atom(cond));
                self.line(&line);
                self.depth += 1;
                self.expr(then);
                self.depth -= 1;
                self.line("} else {");
                self.depth += 1;
                self.expr(els);
                self.depth -= 1;
                self.line("}");
            }
            Expr::Handle { handler, body } => {
                let line = format!("handle {} {{", self.atom(handler));
                self.line(&line);
                self.depth += 1;
                self.expr(body);
                self.depth -= 1;
                self.line("}");
            }
            Expr::Try { on_fail, body } => {
                self.line(&format!("try -> j{} {{", on_fail.0));
                self.depth += 1;
                self.expr(body);
                self.depth -= 1;
                self.line("}");
            }
        }
    }

    fn bind(&self, b: &Bind) -> String {
        match b {
            Bind::Atom(a) => self.atom(a),
            Bind::Prim(op, args) => {
                let name = match op {
                    PrimOp::Show(t) => format!("show<{}>", self.ty(t)),
                    PrimOp::Arbitrary(t) => format!("arbitrary<{}>", self.ty(t)),
                    other => format!("{other:?}").to_lowercase(),
                };
                format!("{name}({})", self.atoms(args))
            }
            Bind::Call(f, args) => format!("call {}({})", self.atom(f), self.atoms(args)),
            Bind::Closure(d, caps) => {
                format!("closure @{}[{}]", self.m.def(*d).name, self.atoms(caps))
            }
            Bind::Alloc(a, args) => {
                let what = match a {
                    Alloc::Tuple => "tuple".to_owned(),
                    Alloc::List => "list".to_owned(),
                    Alloc::Record(t) => self.type_name(*t).to_owned(),
                    Alloc::Variant(t, c) => self.m.ty(*t).ctors()[*c as usize].name.clone(),
                };
                format!("alloc {what}({})", self.atoms(args))
            }
            Bind::Proj(a, p) => match p {
                Proj::Tuple(i) => format!("{}.{i}", self.atom(a)),
                Proj::Field(t, i) => format!(
                    "{}.{}",
                    self.atom(a),
                    self.m.ty(*t).fields()[*i as usize].name
                ),
                Proj::Payload(t, c, i) => {
                    format!(
                        "{}.{}#{i}",
                        self.atom(a),
                        self.m.ty(*t).ctors()[*c as usize].name
                    )
                }
            },
            Bind::Perform(e, op, args) => {
                let eff = self.m.effect(*e);
                format!(
                    "perform {}.{}({})",
                    eff.name,
                    eff.ops[*op as usize].name,
                    self.atoms(args)
                )
            }
            Bind::Check(c, a) => format!("check #{}({})", c.0, self.atom(a)),
            Bind::MakeHandler {
                effect,
                state,
                clauses,
            } => format!(
                "handler {} state[{}] clauses[{}]",
                self.effect_name(*effect),
                self.atoms(state),
                self.atoms(clauses)
            ),
            Bind::StateGet(i, s) => format!("state_get {}[{s}]", self.atom(i)),
            Bind::StateSet(i, s, v) => {
                format!("state_set {}[{s}] = {}", self.atom(i), self.atom(v))
            }
            Bind::Hole(h) => format!("hole ?{h}"),
        }
    }
}
