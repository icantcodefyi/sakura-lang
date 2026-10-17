//! Deterministic text dump of HIR, for snapshot tests and `--dump-hir` style debugging.
//!
//! One node per line, children indented two spaces. Definitions print as `pkg/module::Name`,
//! locals as `name#index`.

use std::fmt::Write;

use crate::hir::*;

impl Program {
    /// `pkg/module::name`; ops and variants print as `pkg/module::Parent.name`.
    pub fn def_path(&self, def: DefId) -> String {
        let m = self.module(def.module);
        let pkg = &self.packages[m.package.0 as usize].name;
        let d = &m.defs[def.index];
        match &d.kind {
            DefKind::Op(op) => format!("{pkg}/{}::{}.{}", m.path, m.defs[op.effect].name, d.name),
            DefKind::Variant(v) => {
                format!("{pkg}/{}::{}.{}", m.path, m.defs[v.parent].name, d.name)
            }
            DefKind::Fn(_) if RefinementIntro::of(d).is_some() => {
                let ty = m.defs.iter().find(|(_, t)| {
                    matches!(&t.kind, DefKind::Type(TypeDef {
                        body: TypeDefBody::Refined { from, validate, .. },
                        ..
                    }) if *from == def.index || *validate == def.index)
                });
                let ty = ty.map_or("?", |(_, t)| t.name.as_str());
                format!("{pkg}/{}::{ty}.{}", m.path, d.name)
            }
            _ => format!("{pkg}/{}::{}", m.path, d.name),
        }
    }

    /// Dumps one module: imports, then every item with its signature and body.
    pub fn dump_module(&self, module: ModuleId) -> String {
        let mut p = Printer {
            program: self,
            module,
            body: None,
            out: String::new(),
        };
        p.module();
        p.out
    }
}

struct Printer<'a> {
    program: &'a Program,
    module: ModuleId,
    body: Option<&'a Body>,
    out: String,
}

impl<'a> Printer<'a> {
    fn line(&mut self, depth: usize, text: impl AsRef<str>) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn module(&mut self) {
        let m = self.program.module(self.module);
        for import in &m.imports {
            let target = match import.target {
                Some(t) => {
                    let tm = self.program.module(t);
                    format!(
                        "{}/{}",
                        self.program.packages[tm.package.0 as usize].name, tm.path
                    )
                }
                None => "<unresolved>".into(),
            };
            let mut s = format!("import {target}");
            if let Some(b) = &import.binding {
                write!(s, " as {b}").unwrap();
            }
            for n in &import.names {
                write!(s, " {}={}", n.name, self.def_opt(n.res)).unwrap();
            }
            self.line(0, s);
        }
        for &item in &m.items {
            self.item(item);
        }
    }

    fn def_opt(&self, def: Option<DefId>) -> String {
        def.map_or("<err>".into(), |d| self.program.def_path(d))
    }

    fn item(&mut self, index: la_arena::Idx<Def>) {
        let m = self.program.module(self.module);
        let def = &m.defs[index];
        let vis = if def.is_pub { "pub " } else { "" };
        for a in &def.attrs {
            let line = format!("@{}({})", a.name, a.args.join(", "));
            self.line(0, line);
        }
        match &def.kind {
            DefKind::Fn(f) => {
                let sig = self.sig(&f.sig);
                self.line(0, format!("{vis}fn {}{sig}", def.name));
                self.body(1, f.body);
            }
            DefKind::Const(c) => {
                let ty = c.ty.as_ref().map(|t| format!(": {}", self.ty(t)));
                self.line(
                    0,
                    format!("{vis}let {}{}", def.name, ty.unwrap_or_default()),
                );
                self.body(1, c.body);
            }
            DefKind::Type(t) => {
                let g = self.generics(&t.generics);
                match &t.body {
                    TypeDefBody::Record(fields) => {
                        self.line(0, format!("{vis}type {}{g} = record", def.name));
                        for f in fields {
                            let line = format!("{}: {}", f.name, self.ty(&f.ty));
                            self.line(1, line);
                        }
                    }
                    TypeDefBody::Variants(vs) => {
                        self.line(0, format!("{vis}type {}{g} = variants", def.name));
                        for &v in vs {
                            let vd = &m.defs[v];
                            if let DefKind::Variant(var) = &vd.kind {
                                let fields: Vec<_> =
                                    var.fields.iter().map(|t| self.ty(t)).collect();
                                self.line(1, format!("{}({})", vd.name, fields.join(", ")));
                            }
                        }
                    }
                    TypeDefBody::ErrorUnion(ts) => {
                        let ts: Vec<_> = ts.iter().map(|t| self.ty(t)).collect();
                        self.line(
                            0,
                            format!("{vis}type {}{g} = errors {}", def.name, ts.join(" | ")),
                        );
                    }
                    TypeDefBody::Alias(t) => {
                        let t = self.ty(t);
                        self.line(0, format!("{vis}type {}{g} = alias {t}", def.name));
                    }
                    TypeDefBody::Refined { base, pred, .. } => {
                        let t = self.ty(base);
                        self.line(0, format!("{vis}type {}{g} = refined {t}", def.name));
                        self.body(1, *pred);
                    }
                    TypeDefBody::Missing => {
                        self.line(0, format!("{vis}type {}{g} = <missing>", def.name))
                    }
                }
            }
            DefKind::Error(e) => {
                let fields: Vec<_> = e
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.ty(&f.ty)))
                    .collect();
                self.line(0, format!("{vis}error {}({})", def.name, fields.join(", ")));
            }
            DefKind::Effect(e) => {
                let g = self.generics(&e.generics);
                self.line(0, format!("{vis}effect {}{g}", def.name));
                for &op in &e.ops {
                    let od = &m.defs[op];
                    if let DefKind::Op(o) = &od.kind {
                        let sig = self.sig(&o.sig);
                        self.line(1, format!("op {}{sig}", od.name));
                        if let Some(b) = o.body {
                            self.body(2, b);
                        }
                    }
                }
            }
            DefKind::Test(t) => {
                self.line(0, format!("test {:?}", def.name));
                self.body(1, t.body);
            }
            DefKind::Property(p) => {
                let params: Vec<_> = p
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, self.ty(&p.ty)))
                    .collect();
                self.line(0, format!("property {:?}({})", def.name, params.join(", ")));
                self.body(1, p.body);
            }
            DefKind::Variant(_) | DefKind::Op(_) => {}
        }
    }

    fn generics(&self, g: &[TypeParam]) -> String {
        if g.is_empty() {
            return String::new();
        }
        let ps: Vec<_> = g
            .iter()
            .map(|p| match &p.kind {
                TypeParamKind::Row => p.name.clone(),
                TypeParamKind::Type(b) if b.is_empty() => p.name.clone(),
                TypeParamKind::Type(b) => {
                    let b: Vec<_> = b.iter().map(|b| format!("{b:?}")).collect();
                    format!("{}: {}", p.name, b.join(" + "))
                }
            })
            .collect();
        format!("<{}>", ps.join(", "))
    }

    fn sig(&self, sig: &Signature) -> String {
        let params: Vec<_> = sig
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, self.ty(&p.ty)))
            .collect();
        let row = sig.row.as_ref().map(|r| format!(" ! {}", self.row(r)));
        format!(
            "{}({}) -> {}{}",
            self.generics(&sig.generics),
            params.join(", "),
            self.ty(&sig.ret),
            row.unwrap_or_default()
        )
    }

    fn type_res(&self, res: TypeRes) -> String {
        match res {
            TypeRes::Def(d) => self.program.def_path(d),
            TypeRes::Builtin(b) => b.name().to_string(),
            TypeRes::Param(i) => format!("${i}"),
            TypeRes::Err => "<err>".into(),
        }
    }

    fn ty(&self, t: &TypeRef) -> String {
        match &t.kind {
            TypeRefKind::Named { res, args } => {
                let mut s = self.type_res(*res);
                if !args.is_empty() {
                    let a: Vec<_> = args.iter().map(|a| self.ty(a)).collect();
                    write!(s, "<{}>", a.join(", ")).unwrap();
                }
                s
            }
            TypeRefKind::Tuple(ts) => {
                let a: Vec<_> = ts.iter().map(|a| self.ty(a)).collect();
                format!("({})", a.join(", "))
            }
            TypeRefKind::Fn { params, ret, row } => {
                let a: Vec<_> = params.iter().map(|a| self.ty(a)).collect();
                let row = row.as_ref().map(|r| format!(" ! {}", self.row(r)));
                format!(
                    "fn({}) -> {}{}",
                    a.join(", "),
                    self.ty(ret),
                    row.unwrap_or_default()
                )
            }
            TypeRefKind::Union(ts) => {
                let a: Vec<_> = ts.iter().map(|a| self.ty(a)).collect();
                a.join(" | ")
            }
            TypeRefKind::Row(r) => self.row(r),
            TypeRefKind::Missing => "<missing>".into(),
        }
    }

    fn row(&self, r: &RowRef) -> String {
        let mut labels: Vec<_> = r
            .labels
            .iter()
            .map(|l| {
                let mut s = self.type_res(l.effect);
                if !l.args.is_empty() {
                    let a: Vec<_> = l.args.iter().map(|a| self.ty(a)).collect();
                    write!(s, "<{}>", a.join(", ")).unwrap();
                }
                s
            })
            .collect();
        if let Some(t) = r.tail {
            labels.push(format!("| {}", self.type_res(t)));
        }
        format!("{{{}}}", labels.join(", "))
    }

    fn body(&mut self, depth: usize, id: BodyId) {
        let body = &self.program.module(self.module).bodies[id];
        let prev = self.body.replace(body);
        if !body.params.is_empty() {
            let ps: Vec<_> = body.params.iter().map(|&l| self.local(l)).collect();
            self.line(depth, format!("params {}", ps.join(" ")));
        }
        for (kw, cs) in [("requires", &body.requires), ("ensures", &body.ensures)] {
            for c in cs {
                let label = c
                    .label
                    .as_deref()
                    .map(|l| format!(" {l}:"))
                    .unwrap_or_default();
                self.line(depth, format!("{kw}{label}"));
                self.expr(depth + 1, c.expr);
            }
        }
        if let Some(v) = body.value {
            self.expr(depth, v);
        }
        self.body = prev;
    }

    fn b(&self) -> &'a Body {
        self.body.expect("printing inside a body")
    }

    fn local(&self, l: LocalId) -> String {
        let local = &self.b().locals[l];
        let m = if local.mutable { "mut " } else { "" };
        format!("{m}{}#{}", local.name, l.into_raw())
    }

    fn res(&self, r: Res) -> String {
        match r {
            Res::Local(l) => format!("local {}", self.local(l)),
            Res::Def(d) => format!("def {}", self.program.def_path(d)),
            Res::Err => "<err>".into(),
        }
    }

    fn arms(&mut self, depth: usize, arms: &[MatchArm]) {
        for arm in arms {
            self.line(depth, "arm");
            self.pat(depth + 1, arm.pat);
            if let Some(g) = arm.guard {
                self.line(depth + 1, "guard");
                self.expr(depth + 2, g);
            }
            self.expr(depth + 1, arm.body);
        }
    }

    fn expr(&mut self, d: usize, id: ExprId) {
        let body = self.b();
        match &body.exprs[id] {
            Expr::Missing => self.line(d, "<missing>"),
            Expr::Lit(l) => self.line(d, lit(l)),
            Expr::Interpolate { parts, args } => {
                self.line(d, format!("interpolate {parts:?}"));
                for &a in args {
                    self.expr(d + 1, a);
                }
            }
            Expr::Template {
                template,
                parts,
                args,
            } => {
                let t = self.res(*template);
                self.line(d, format!("template {t} {parts:?}"));
                for &a in args {
                    self.expr(d + 1, a);
                }
            }
            Expr::Hole(n) => self.line(d, format!("hole ?{n}")),
            Expr::Path(r) => {
                let r = self.res(*r);
                self.line(d, r);
            }
            Expr::Call {
                callee,
                args,
                type_args,
                arg_names,
            } => {
                self.call_line(d, "call", type_args, arg_names);
                self.expr(d + 1, *callee);
                for &a in args {
                    self.expr(d + 1, a);
                }
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                type_args,
                arg_names,
                ..
            } => {
                self.call_line(d, &format!("method .{method}"), type_args, arg_names);
                self.expr(d + 1, *receiver);
                for &a in args {
                    self.expr(d + 1, a);
                }
            }
            Expr::Field { base, name } => {
                self.line(d, format!("field .{name}"));
                self.expr(d + 1, *base);
            }
            Expr::TupleField { base, index } => {
                self.line(d, format!("field .{index}"));
                self.expr(d + 1, *base);
            }
            Expr::Index { base, index } => {
                self.line(d, "index");
                self.expr(d + 1, *base);
                self.expr(d + 1, *index);
            }
            Expr::Unary { op, expr } => {
                self.line(d, format!("{op:?}"));
                self.expr(d + 1, *expr);
            }
            Expr::Binary { op, lhs, rhs } => {
                self.line(d, format!("{op:?}"));
                self.expr(d + 1, *lhs);
                self.expr(d + 1, *rhs);
            }
            Expr::Raise(e) => {
                self.line(d, "raise");
                self.expr(d + 1, *e);
            }
            Expr::Block { stmts, tail } => {
                self.line(d, "block");
                for s in stmts {
                    self.stmt(d + 1, s);
                }
                if let Some(t) = tail {
                    self.expr(d + 1, *t);
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.line(d, "if");
                self.expr(d + 1, *cond);
                self.expr(d + 1, *then_branch);
                if let Some(e) = else_branch {
                    self.expr(d + 1, *e);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.line(d, "match");
                self.expr(d + 1, *scrutinee);
                self.arms(d + 1, arms);
            }
            Expr::Loop { body } => {
                self.line(d, "loop");
                self.expr(d + 1, *body);
            }
            Expr::For {
                pat,
                iterable,
                body,
            } => {
                self.line(d, "for");
                self.pat(d + 1, *pat);
                self.expr(d + 1, *iterable);
                self.expr(d + 1, *body);
            }
            Expr::Break => self.line(d, "break"),
            Expr::Continue => self.line(d, "continue"),
            Expr::Return(e) => {
                self.line(d, "return");
                if let Some(e) = e {
                    self.expr(d + 1, *e);
                }
            }
            Expr::Fail(e) => {
                self.line(d, "fail");
                self.expr(d + 1, *e);
            }
            Expr::Lambda { params, ret, body } => {
                let ps: Vec<_> = params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(t) => format!("{}: {}", self.local(p.local), self.ty(t)),
                        None => self.local(p.local),
                    })
                    .collect();
                let ret = ret.as_ref().map(|t| format!(" -> {}", self.ty(t)));
                self.line(
                    d,
                    format!("lambda ({}){}", ps.join(", "), ret.unwrap_or_default()),
                );
                self.expr(d + 1, *body);
            }
            Expr::With { handlers, body } => {
                self.line(d, "with");
                for &h in handlers {
                    self.expr(d + 1, h);
                }
                self.expr(d + 1, *body);
            }
            Expr::Try { body, catch_arms } => {
                self.line(d, "try");
                self.expr(d + 1, *body);
                if let Some(arms) = catch_arms {
                    self.line(d, "catch");
                    self.arms(d + 1, arms);
                }
            }
            Expr::Handler(h) => {
                let eff = self.type_res(h.effect);
                self.line(d, format!("handler {eff}"));
                for s in &h.state {
                    let ty = s.ty.as_ref().map(|t| format!(": {}", self.ty(t)));
                    let line = format!("state {}{}", self.local(s.local), ty.unwrap_or_default());
                    self.line(d + 1, line);
                    self.expr(d + 2, s.init);
                }
                for c in &h.clauses {
                    let ps: Vec<_> = c.params.iter().map(|&l| self.local(l)).collect();
                    let line = format!("clause {} -> {}", c.name, self.def_opt(c.op));
                    self.line(d + 1, format!("{line} ({})", ps.join(", ")));
                    self.expr(d + 2, c.body);
                }
            }
            Expr::Record { ty, spread, fields } => {
                let t = self.type_res(*ty);
                self.line(d, format!("record {t}"));
                if let Some(s) = spread {
                    self.line(d + 1, "..");
                    self.expr(d + 2, *s);
                }
                for f in fields {
                    self.line(d + 1, format!("{}:", f.name));
                    self.expr(d + 2, f.value);
                }
            }
            Expr::Tuple(es) => {
                self.line(d, "tuple");
                for &e in es {
                    self.expr(d + 1, e);
                }
            }
            Expr::Attr { name, args } => {
                self.line(d, format!("attr @{name}"));
                for &a in args {
                    self.expr(d + 1, a);
                }
            }
            Expr::List(es) => {
                self.line(d, "list");
                for &e in es {
                    self.expr(d + 1, e);
                }
            }
        }
    }

    fn call_line(&mut self, d: usize, head: &str, type_args: &[TypeRef], names: &[Option<String>]) {
        let mut s = head.to_string();
        if !type_args.is_empty() {
            let t: Vec<_> = type_args.iter().map(|t| self.ty(t)).collect();
            write!(s, " <{}>", t.join(", ")).unwrap();
        }
        if !names.is_empty() {
            let n: Vec<_> = names.iter().map(|n| n.as_deref().unwrap_or("_")).collect();
            write!(s, " names({})", n.join(", ")).unwrap();
        }
        self.line(d, s);
    }

    fn stmt(&mut self, d: usize, s: &Stmt) {
        match s {
            Stmt::Let { pat, ty, init } => {
                let ty = ty.as_ref().map(|t| format!(": {}", self.ty(t)));
                self.line(d, format!("let{}", ty.unwrap_or_default()));
                self.pat(d + 1, *pat);
                self.expr(d + 1, *init);
            }
            Stmt::Var { local, ty, init } => {
                let ty = ty.as_ref().map(|t| format!(": {}", self.ty(t)));
                let line = format!("var {}{}", self.local(*local), ty.unwrap_or_default());
                self.line(d, line);
                self.expr(d + 1, *init);
            }
            Stmt::Assign {
                target,
                fields,
                value,
                ..
            } => {
                let t = target.map_or("<err>".into(), |l| self.local(l));
                let path: String = fields.iter().map(|f| format!(".{f}")).collect();
                self.line(d, format!("assign {t}{path}"));
                self.expr(d + 1, *value);
            }
            Stmt::Expr(e) => self.expr(d, *e),
        }
    }

    fn pat(&mut self, d: usize, id: PatId) {
        let body = self.b();
        match &body.pats[id] {
            Pat::Missing => self.line(d, "pat <missing>"),
            Pat::Wild => self.line(d, "pat _"),
            Pat::Bind(l) => {
                let l = self.local(*l);
                self.line(d, format!("pat bind {l}"));
            }
            Pat::Lit(l) => self.line(d, format!("pat {}", lit(l))),
            Pat::Ctor { res, args } => {
                let r = self.res(*res);
                self.line(d, format!("pat ctor {r}"));
                for &a in args {
                    self.pat(d + 1, a);
                }
            }
            Pat::Record { ty, fields, rest } => {
                let t = self.type_res(*ty);
                let rest = if *rest { " .." } else { "" };
                self.line(d, format!("pat record {t}{rest}"));
                for f in fields {
                    self.line(d + 1, format!("{}:", f.name));
                    self.pat(d + 2, f.pat);
                }
            }
            Pat::Tuple(ps) => {
                self.line(d, "pat tuple");
                for &p in ps {
                    self.pat(d + 1, p);
                }
            }
            Pat::List { items, rest } => {
                let rest = match rest {
                    None => String::new(),
                    Some(None) => " ..".into(),
                    Some(Some(l)) => format!(" ..{}", self.local(*l)),
                };
                self.line(d, format!("pat list{rest}"));
                for &p in items {
                    self.pat(d + 1, p);
                }
            }
            Pat::Or(ps) => {
                self.line(d, "pat or");
                for &p in ps {
                    self.pat(d + 1, p);
                }
            }
        }
    }
}

fn lit(l: &Literal) -> String {
    match l {
        Literal::Int(i) => format!("int {i}"),
        Literal::Float(f) => format!("float {f:?}"),
        Literal::String(s) => format!("string {s:?}"),
        Literal::Bool(b) => format!("bool {b}"),
        Literal::Unit => "unit".into(),
    }
}
