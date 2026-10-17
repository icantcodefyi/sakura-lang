//! Body lowering: expressions, statements and patterns with lexical scopes.

use std::collections::HashMap;

use la_arena::Idx;
use sakura_syntax::{SyntaxKind::*, SyntaxNode, SyntaxToken};
use text_size::TextRange;

use crate::diag::{self, did_you_mean};
use crate::hir::*;
use crate::lower::*;

impl Cx {
    pub(crate) fn lower_fn_body(
        &mut self,
        m: ModuleId,
        owner: Idx<Def>,
        id: BodyId,
        node: &SyntaxNode,
        params: &[Param],
        generics: &[TypeParam],
    ) -> BodyId {
        let mut b = BodyCx::new(self, m, owner, generics);
        b.push_scope();
        for p in params {
            let l = b.bind(&p.name, p.range, false);
            b.body.params.push(l);
        }
        let contracts: Vec<SyntaxNode> = children(node, CONTRACT).collect();
        for c in contracts.iter().filter(|c| token(c, REQUIRES_KW).is_some()) {
            let contract = b.contract(c);
            b.body.requires.push(contract);
        }
        let block = child(node, BLOCK);
        b.body.value = match &block {
            Some(blk) => Some(b.expr(blk)),
            None if node.kind() == OP_SIG => None,
            None if node.kind() == FN_DECL => {
                let def = &b.cx.program.module(m).defs[owner];
                if def
                    .attrs
                    .iter()
                    .any(|a| a.name == "external" || a.name == "llm")
                {
                    None
                } else {
                    let file = b.file();
                    let msg = format!(
                        "function `{}` has no body; only `@external` and `@llm` declarations may omit it",
                        def.name
                    );
                    b.cx.report(diag::error(diag::MISSING_BODY, file, def.range, msg));
                    Some(b.missing(node.text_range()))
                }
            }
            None => Some(b.missing(node.text_range())),
        };
        let ensures: Vec<&SyntaxNode> = contracts
            .iter()
            .filter(|c| token(c, ENSURES_KW).is_some())
            .collect();
        if !ensures.is_empty() {
            b.push_scope();
            let r = b.alloc_local("result", node.text_range(), false);
            b.define("result", r);
            b.body.result = Some(r);
            for c in ensures {
                let contract = b.contract(c);
                b.body.ensures.push(contract);
            }
        }
        b.finish(id)
    }

    pub(crate) fn lower_expr_body(
        &mut self,
        m: ModuleId,
        owner: Idx<Def>,
        id: BodyId,
        expr: Option<SyntaxNode>,
        _params: &[Param],
        generics: &[TypeParam],
    ) -> BodyId {
        let mut b = BodyCx::new(self, m, owner, generics);
        b.push_scope();
        let range = expr.as_ref().map(|e| e.text_range()).unwrap_or_default();
        b.body.value = Some(match expr {
            Some(e) => b.expr(&e),
            None => b.missing(range),
        });
        b.finish(id)
    }

    pub(crate) fn lower_refinement(
        &mut self,
        m: ModuleId,
        owner: Idx<Def>,
        id: BodyId,
        where_clause: Option<SyntaxNode>,
        generics: &[TypeParam],
    ) -> BodyId {
        let mut b = BodyCx::new(self, m, owner, generics);
        b.push_scope();
        let range = where_clause
            .as_ref()
            .map(|w| w.text_range())
            .unwrap_or_default();
        let it = b.bind("it", range, false);
        b.body.params.push(it);
        let pred = where_clause.as_ref().and_then(|w| exprs(w).next());
        b.body.value = Some(match pred {
            Some(e) => b.expr(&e),
            None => b.missing(range),
        });
        b.finish(id)
    }
}

struct BodyCx<'a> {
    cx: &'a mut Cx,
    m: ModuleId,
    generics: &'a [TypeParam],
    body: Body,
    scopes: Vec<Vec<(String, LocalId)>>,
    loop_depth: u32,
    /// Names bound by `let f = fn(…) { … }` while lowering the lambda (for `V1020`).
    lambda_lets: Vec<String>,
    /// `let` keyword ranges of immutable locals, for the "use `var`" fix.
    let_kw: HashMap<LocalId, TextRange>,
    /// Locals bound by the first alternative of the enclosing or-pattern.
    shared_or: Vec<(String, LocalId)>,
}

impl<'a> BodyCx<'a> {
    fn new(cx: &'a mut Cx, m: ModuleId, owner: Idx<Def>, generics: &'a [TypeParam]) -> Self {
        BodyCx {
            cx,
            m,
            generics,
            body: Body::empty(owner),
            scopes: Vec::new(),
            loop_depth: 0,
            lambda_lets: Vec::new(),
            let_kw: HashMap::new(),
            shared_or: Vec::new(),
        }
    }

    fn finish(self, id: BodyId) -> BodyId {
        self.cx.program.modules[self.m.0 as usize].bodies[id] = self.body;
        id
    }

    fn file(&self) -> sakura_diagnostics::FileId {
        self.cx.file(self.m)
    }

    // ---- Scopes ----

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn alloc_local(&mut self, name: &str, range: TextRange, mutable: bool) -> LocalId {
        self.body.locals.alloc(Local {
            name: name.to_string(),
            mutable,
            range,
        })
    }

    fn define(&mut self, name: &str, l: LocalId) {
        if let Some(s) = self.scopes.last_mut() {
            s.push((name.to_string(), l));
        }
    }

    /// Allocates and brings into scope a local, warning when it shadows an import (`W1002`).
    fn bind(&mut self, name: &str, range: TextRange, mutable: bool) -> LocalId {
        let l = self.alloc_local(name, range, mutable);
        if let Some(&(_, import_range)) = self.cx.mods[self.m.0 as usize].imported.get(name) {
            let file = self.file();
            let d = diag::warning(
                diag::SHADOWED_IMPORT,
                file,
                range,
                format!("`{name}` shadows an imported name"),
            )
            .with_span(
                diag::span(file, import_range)
                    .with_role(sakura_diagnostics::SpanRole::Definition)
                    .with_label("imported here"),
            );
            self.cx.report(d);
        }
        self.define(name, l);
        l
    }

    /// The local of a lambda/clause parameter; `_` gets an unnamed local that is not in scope.
    fn param_local(&mut self, p: &SyntaxNode) -> LocalId {
        match token(p, IDENT) {
            Some(t) => self.bind(t.text(), t.text_range(), false),
            None => self.alloc_local("_", p.text_range(), false),
        }
    }

    fn lookup_local(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|(n, _)| n == name)
            .map(|&(_, l)| l)
    }

    fn local_names(&self) -> Vec<String> {
        self.scopes
            .iter()
            .flat_map(|s| s.iter().map(|(n, _)| n.clone()))
            .collect()
    }

    // ---- Allocation ----

    fn alloc(&mut self, e: Expr, range: TextRange) -> ExprId {
        let id = self.body.exprs.alloc(e);
        self.body.source_map.exprs.insert(id, range);
        id
    }

    fn alloc_pat(&mut self, p: Pat, range: TextRange) -> PatId {
        let id = self.body.pats.alloc(p);
        self.body.source_map.pats.insert(id, range);
        id
    }

    fn missing(&mut self, range: TextRange) -> ExprId {
        self.alloc(Expr::Missing, range)
    }

    fn opt_expr(&mut self, n: Option<SyntaxNode>, fallback: TextRange) -> ExprId {
        match n {
            Some(n) => self.expr(&n),
            None => self.missing(fallback),
        }
    }

    fn ty(&mut self, n: &SyntaxNode) -> TypeRef {
        self.cx.lower_type(self.m, n, self.generics)
    }

    fn contract(&mut self, c: &SyntaxNode) -> Contract {
        let label = token(c, COLON)
            .and_then(|_| token(c, IDENT))
            .map(|t| t.text().to_string());
        let expr = self.opt_expr(exprs(c).next(), c.text_range());
        Contract { label, expr }
    }

    // ---- Expressions ----

    fn expr(&mut self, n: &SyntaxNode) -> ExprId {
        crate::ensure_stack(|| self.expr_inner(n))
    }

    fn expr_inner(&mut self, n: &SyntaxNode) -> ExprId {
        let range = n.text_range();
        let e = match n.kind() {
            LITERAL => self.literal(n),
            STRING_LIT => self.string(n),
            TAGGED_STRING => self.tagged(n),
            PATH_EXPR => Expr::Path(self.path_value(n)),
            HOLE_EXPR => {
                let t = token(n, HOLE).map(|t| t.text().trim_start_matches('?').to_string());
                Expr::Hole(t.unwrap_or_default())
            }
            RECORD_EXPR => self.record(n),
            PAREN_EXPR => return self.opt_expr(exprs(n).next(), range),
            TUPLE_EXPR => {
                let es: Vec<SyntaxNode> = exprs(n).collect();
                if es.is_empty() {
                    Expr::Lit(Literal::Unit)
                } else {
                    Expr::Tuple(es.iter().map(|e| self.expr(e)).collect())
                }
            }
            LIST_EXPR => {
                let es: Vec<SyntaxNode> = exprs(n).collect();
                Expr::List(es.iter().map(|e| self.expr(e)).collect())
            }
            BLOCK => self.block(n),
            IF_EXPR => {
                let es: Vec<SyntaxNode> = exprs(n).collect();
                let cond = self.opt_expr(es.first().cloned(), range);
                let then_branch = self.opt_expr(es.get(1).cloned(), range);
                let else_branch = es.get(2).map(|e| self.expr(e));
                Expr::If {
                    cond,
                    then_branch,
                    else_branch,
                }
            }
            MATCH_EXPR => {
                let scrutinee = self.opt_expr(exprs(n).next(), range);
                let arms = self.arms(child(n, MATCH_ARM_LIST).as_ref().unwrap_or(n));
                Expr::Match { scrutinee, arms }
            }
            WITH_EXPR => {
                let es: Vec<SyntaxNode> = exprs(n).collect();
                let (body, handlers) = match es.split_last() {
                    Some((b, hs)) if b.kind() == BLOCK => (Some(b.clone()), hs.to_vec()),
                    _ => (None, es.clone()),
                };
                let handlers = handlers.iter().map(|h| self.expr(h)).collect();
                let body = self.opt_expr(body, range);
                Expr::With { handlers, body }
            }
            TRY_EXPR => {
                let body = self.opt_expr(child(n, BLOCK), range);
                let catch_arms = token(n, CATCH_KW)
                    .map(|_| self.arms(child(n, MATCH_ARM_LIST).as_ref().unwrap_or(n)));
                Expr::Try { body, catch_arms }
            }
            LAMBDA_EXPR => self.lambda(n),
            HANDLER_EXPR => self.handler(n),
            FAIL_EXPR => Expr::Fail(self.opt_expr(exprs(n).next(), range)),
            BIN_EXPR => self.binary(n),
            PREFIX_EXPR => {
                let op = if token(n, BANG).is_some() {
                    UnaryOp::Not
                } else {
                    UnaryOp::Neg
                };
                let expr = self.opt_expr(exprs(n).next(), range);
                Expr::Unary { op, expr }
            }
            CALL_EXPR => self.call(n),
            FIELD_EXPR => self.field(n),
            INDEX_EXPR => {
                let es: Vec<SyntaxNode> = exprs(n).collect();
                let base = self.opt_expr(es.first().cloned(), range);
                let index = self.opt_expr(es.get(1).cloned(), range);
                Expr::Index { base, index }
            }
            RAISE_EXPR => Expr::Raise(self.opt_expr(exprs(n).next(), range)),
            ATTR_EXPR => {
                let name = token(n, IDENT);
                // §2.5: `@trust(…)` is the only attribute that is an expression.
                if let Some(t) = &name
                    && t.text() != "trust"
                {
                    let file = self.file();
                    let msg = format!(
                        "`@{}` cannot be used as an expression; only `@trust(\"reason\")` can",
                        t.text()
                    );
                    self.cx.report(diag::error(
                        diag::UNKNOWN_ATTRIBUTE,
                        file,
                        t.text_range(),
                        msg,
                    ));
                }
                let name = name.map(|t| t.text().to_string()).unwrap_or_default();
                let (args, _) = self.args(n);
                Expr::Attr { name, args }
            }
            _ => Expr::Missing,
        };
        self.alloc(e, range)
    }

    fn literal(&mut self, n: &SyntaxNode) -> Expr {
        match lit_of(n) {
            Some(l) => Expr::Lit(l),
            None => Expr::Missing,
        }
    }

    fn string_parts(&mut self, n: &SyntaxNode) -> (Vec<String>, Vec<ExprId>) {
        let mut parts = vec![String::new()];
        let mut args = Vec::new();
        for el in n.children_with_tokens() {
            match el {
                rowan::NodeOrToken::Token(t) if t.kind() == STRING_TEXT => {
                    parts.last_mut().unwrap().push_str(&unescape(t.text()));
                }
                rowan::NodeOrToken::Node(c) if c.kind() == INTERPOLATION => {
                    let e = self.opt_expr(exprs(&c).next(), c.text_range());
                    args.push(e);
                    parts.push(String::new());
                }
                _ => {}
            }
        }
        (parts, args)
    }

    fn string(&mut self, n: &SyntaxNode) -> Expr {
        let (mut parts, args) = self.string_parts(n);
        if args.is_empty() {
            Expr::Lit(Literal::String(parts.pop().unwrap_or_default()))
        } else {
            Expr::Interpolate { parts, args }
        }
    }

    fn tagged(&mut self, n: &SyntaxNode) -> Expr {
        let (parts, args) = match child(n, STRING_LIT) {
            Some(s) => self.string_parts(&s),
            None => (vec![String::new()], Vec::new()),
        };
        let Some(tag) = token(n, IDENT) else {
            return Expr::Template {
                template: Res::Err,
                parts,
                args,
            };
        };
        let file = self.file();
        let template = match self.cx.lookup_module_name(self.m, tag.text()) {
            Some(Binding::Module(t)) => match self.cx.program.module(t).exports.get("template") {
                Some(&index) => Res::Def(DefId { module: t, index }),
                None => {
                    let msg = format!(
                        "module `{}` does not export `template`, so `{}\"…\"` is not a template tag",
                        self.cx.module_name(t),
                        tag.text()
                    );
                    self.cx.report(diag::error(
                        diag::UNRESOLVED_NAME,
                        file,
                        tag.text_range(),
                        msg,
                    ));
                    Res::Err
                }
            },
            _ if self.cx.is_poisoned(self.m, tag.text()) => Res::Err,
            _ => {
                let msg = format!(
                    "unknown template tag `{0}`; a tag is a module exporting `template`, e.g. `import std/{0}`",
                    tag.text()
                );
                self.cx.report(diag::error(
                    diag::UNRESOLVED_NAME,
                    file,
                    tag.text_range(),
                    msg,
                ));
                Res::Err
            }
        };
        Expr::Template {
            template,
            parts,
            args,
        }
    }

    fn binary(&mut self, n: &SyntaxNode) -> Expr {
        let op = n
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find_map(|t| binop(t.kind()));
        let es: Vec<SyntaxNode> = exprs(n).collect();
        let range = n.text_range();
        let lhs = self.opt_expr(es.first().cloned(), range);
        let rhs = self.opt_expr(es.get(1).cloned(), range);
        match op {
            Some(op) => Expr::Binary { op, lhs, rhs },
            None => Expr::Missing,
        }
    }

    /// Arguments of a call or attribute, with names when any argument is `name: value`.
    fn args(&mut self, n: &SyntaxNode) -> (Vec<ExprId>, Vec<Option<String>>) {
        let Some(list) = child(n, ARG_LIST) else {
            return (Vec::new(), Vec::new());
        };
        let mut args = Vec::new();
        let mut names = Vec::new();
        for c in list.children() {
            if is_expr(c.kind()) {
                args.push(self.expr(&c));
                names.push(None);
            } else if c.kind() == NAMED_ARG {
                let v = self.opt_expr(exprs(&c).next(), c.text_range());
                args.push(v);
                names.push(token(&c, IDENT).map(|t| t.text().to_string()));
            }
        }
        // `f(` cut short by a parse error: a trailing `Missing` tells the type checker the
        // argument count is unknown, so it does not add an arity error (V2002).
        if token(&list, R_PAREN).is_none() {
            let end = TextRange::empty(list.text_range().end());
            args.push(self.missing(end));
            names.push(None);
        }
        if names.iter().all(Option::is_none) {
            names.clear();
        }
        (args, names)
    }

    fn type_args(&mut self, n: &SyntaxNode) -> Vec<TypeRef> {
        match child(n, GENERIC_ARG_LIST) {
            Some(l) => self.cx.lower_generic_args(self.m, &l, self.generics, false),
            None => Vec::new(),
        }
    }

    fn call(&mut self, n: &SyntaxNode) -> Expr {
        let range = n.text_range();
        let callee = exprs(n).next();
        if let Some(c) = &callee
            && c.kind() == FIELD_EXPR
            && let Some((base, name)) = field_parts(c)
        {
            if let Some(res) = self.static_member(&base, &name) {
                let callee = self.alloc(Expr::Path(res), c.text_range());
                let type_args = self.type_args(n);
                let (args, arg_names) = self.args(n);
                return Expr::Call {
                    callee,
                    args,
                    type_args,
                    arg_names,
                };
            }
            if name.kind() != INT {
                let receiver = self.expr(&base);
                let type_args = self.type_args(n);
                let (args, arg_names) = self.args(n);
                return Expr::MethodCall {
                    receiver,
                    method: name.text().to_string(),
                    method_range: name.text_range(),
                    args,
                    type_args,
                    arg_names,
                };
            }
        }
        let callee = self.opt_expr(callee, range);
        let type_args = self.type_args(n);
        let (args, arg_names) = self.args(n);
        Expr::Call {
            callee,
            args,
            type_args,
            arg_names,
        }
    }

    fn field(&mut self, n: &SyntaxNode) -> Expr {
        let Some((base, name)) = field_parts(n) else {
            let e = self.opt_expr(exprs(n).next(), n.text_range());
            return Expr::Field {
                base: e,
                name: String::new(),
            };
        };
        if let Some(res) = self.static_member(&base, &name) {
            return Expr::Path(res);
        }
        let base = self.expr(&base);
        if name.kind() == INT {
            match name.text().replace('_', "").parse() {
                Ok(index) => Expr::TupleField { base, index },
                Err(_) => Expr::Missing,
            }
        } else {
            Expr::Field {
                base,
                name: name.text().to_string(),
            }
        }
    }

    /// §4.9 rules 2–4: `Eff.op`, `Type.f` / `Type.Variant`, `module.f`. `None` when `base` is a
    /// value (rules 1 and 5 need types); `Some(Res::Err)` after reporting a failed lookup.
    fn static_member(&mut self, base: &SyntaxNode, name: &SyntaxToken) -> Option<Res> {
        if base.kind() != PATH_EXPR || name.kind() == INT {
            return None;
        }
        let segs = path_segments(base);
        let last = segs.last()?;
        let file = self.file();
        let member = name.text();
        if segs.len() == 1 && self.lookup_local(last.text()).is_some() {
            return None;
        }
        let binding = match self.cx.resolve_path(self.m, &segs, Ns::Type) {
            Ok(b) => b,
            Err(true) => return Some(Res::Err),
            Err(false) => return None,
        };
        let ty = match binding {
            Binding::Def(d) => match &self.cx.program.def(d).kind {
                DefKind::Effect(e) => {
                    let module = &self.cx.program.module(d.module);
                    if let Some(&op) = e.ops.iter().find(|&&o| module.defs[o].name == member) {
                        return Some(Res::Def(DefId {
                            module: d.module,
                            index: op,
                        }));
                    }
                    TypeRes::Def(d)
                }
                DefKind::Type(_) | DefKind::Error(_) => TypeRes::Def(d),
                _ => return None,
            },
            Binding::Builtin(b) => TypeRes::Builtin(b),
            Binding::Module(t) => {
                return Some(
                    match self.cx.module_member(self.m, t, member, name.text_range()) {
                        Some(Binding::Def(d)) => Res::Def(d),
                        _ => Res::Err,
                    },
                );
            }
        };
        if let Some(def) = self.cx.program.resolve_home(ty, member, self.m) {
            return Some(Res::Def(def));
        }
        let type_name = last.text();
        let Some(home) = self.cx.program.home_module(ty) else {
            let hint = match ty {
                TypeRes::Builtin(b) => b
                    .home_module()
                    .map(|p| format!("; its functions live in std module `std/{p}`, which is not part of this build"))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let msg = format!("`{type_name}` has no function `{member}`{hint}");
            self.cx.report(diag::error(
                diag::UNRESOLVED_NAME,
                file,
                name.text_range(),
                msg,
            ));
            return Some(Res::Err);
        };
        // A variant of an opaque type, or a private function: say so instead of "not found".
        if let Some(def) = self.cx.program.lookup(home, member)
            && let Some(parent) = self.cx.opaque_parent(def)
            && ty == TypeRes::Def(parent)
        {
            self.cx.report_opaque(self.m, parent, name.text_range());
            return Some(Res::Err);
        }
        if let Some(def) = self.cx.program.lookup(home, member)
            && matches!(
                self.cx.program.def(def).kind,
                DefKind::Fn(_) | DefKind::Const(_)
            )
        {
            let def_range = self.cx.program.def(def).range;
            let def_file = self.cx.program.module(home).file;
            let msg = format!(
                "`{type_name}.{member}` is private to module `{}`",
                self.cx.module_name(home)
            );
            let d = diag::error(diag::PRIVATE_ITEM, file, name.text_range(), msg).with_span(
                diag::span(def_file, def_range)
                    .with_role(sakura_diagnostics::SpanRole::Definition)
                    .with_label("declared here without `pub`"),
            );
            self.cx.report(d);
            return Some(Res::Err);
        }
        let mut cands: Vec<String> = Vec::new();
        if let TypeRes::Def(d) = ty
            && let DefKind::Effect(e) = &self.cx.program.def(d).kind
        {
            let module = self.cx.program.module(d.module);
            cands.extend(e.ops.iter().map(|&o| module.defs[o].name.clone()));
        }
        let hm = self.cx.program.module(home);
        cands.extend(
            hm.exports
                .iter()
                .filter(|&(_, &i)| matches!(hm.defs[i].kind, DefKind::Fn(_) | DefKind::Const(_)))
                .map(|(n, _)| n.clone()),
        );
        let what = match ty {
            TypeRes::Def(d) if matches!(self.cx.program.def(d).kind, DefKind::Effect(_)) => {
                format!("effect `{type_name}` has no operation or function `{member}`")
            }
            _ => format!(
                "`{type_name}` has no function or variant `{member}` in its home module `{}`",
                self.cx.module_name(home)
            ),
        };
        let mut d = diag::error(diag::UNRESOLVED_NAME, file, name.text_range(), what);
        if let Some(s) = did_you_mean(member, cands.iter().map(String::as_str)) {
            d.message.push_str(&format!("; did you mean `{s}`?"));
            d = d.with_fix(diag::replace_fix(
                file,
                name.text_range(),
                format!("Use `{type_name}.{s}`"),
                s,
            ));
        }
        self.cx.report(d);
        Some(Res::Err)
    }

    /// Resolves a value-position path, reporting failures. `None` when unresolved.
    fn value_binding(&mut self, segs: &[SyntaxToken]) -> Option<Binding> {
        match self.cx.resolve_path(self.m, segs, Ns::Value) {
            Ok(b) => Some(b),
            Err(true) => None,
            Err(false) => {
                self.unresolved(segs.last()?);
                None
            }
        }
    }

    /// A name in value position: a local, function, constant, constructor or effect op.
    fn path_value(&mut self, n: &SyntaxNode) -> Res {
        let segs = path_segments(n);
        let Some(last) = segs.last().cloned() else {
            return Res::Err;
        };
        if segs.len() == 1
            && let Some(l) = self.lookup_local(last.text())
        {
            return Res::Local(l);
        }
        let Some(binding) = self.value_binding(&segs) else {
            return Res::Err;
        };
        match binding {
            Binding::Def(d)
                if matches!(
                    self.cx.program.def(d).kind,
                    DefKind::Fn(_) | DefKind::Const(_) | DefKind::Op(_)
                ) =>
            {
                Res::Def(d)
            }
            Binding::Module(t) => {
                let file = self.file();
                let msg = format!(
                    "`{}` is the module `{}`, not a value; use one of its items, e.g. `{}.f(…)`",
                    last.text(),
                    self.cx.module_name(t),
                    last.text()
                );
                self.cx
                    .report(diag::error(diag::WRONG_KIND, file, last.text_range(), msg));
                Res::Err
            }
            _ => self.constructor(binding, &last),
        }
    }

    /// A `TypeName` used as a value or constructor pattern: a variant or error tag.
    fn constructor(&mut self, binding: Binding, name: &SyntaxToken) -> Res {
        if let Binding::Def(d) = binding {
            match &self.cx.program.def(d).kind {
                DefKind::Variant(_) | DefKind::Error(_) => return Res::Def(d),
                DefKind::Type(t) => {
                    if let Some(v) = same_named_variant(&self.cx.program, d, t) {
                        if !self.cx.program.constructors_visible(d, self.m) {
                            self.cx.report_opaque(self.m, d, name.text_range());
                            return Res::Err;
                        }
                        return Res::Def(v);
                    }
                }
                _ => {}
            }
        }
        let file = self.file();
        let what = self.cx.describe_binding(binding);
        let hint = match binding {
            Binding::Def(d) => match &self.cx.program.def(d).kind {
                DefKind::Type(TypeDef {
                    body: TypeDefBody::Record(_),
                    ..
                }) => format!("; construct it with `{} {{ … }}`", name.text()),
                DefKind::Effect(_) => {
                    format!("; perform an operation such as `{}.op(…)`", name.text())
                }
                DefKind::Type(TypeDef {
                    body: TypeDefBody::External,
                    ..
                }) => {
                    "; it is an external type, so its values come only from `@external` functions"
                        .to_string()
                }
                _ => String::new(),
            },
            _ => String::new(),
        };
        let msg = format!("`{}` is {what}, not a value{hint}", name.text());
        self.cx
            .report(diag::error(diag::WRONG_KIND, file, name.text_range(), msg));
        Res::Err
    }

    fn unresolved(&mut self, name: &SyntaxToken) {
        let file = self.file();
        let text = name.text();
        let range = name.text_range();
        if self.cx.is_poisoned(self.m, text) {
            return;
        }
        if self.lambda_lets.iter().any(|l| l == text) {
            let msg = format!(
                "local lambda `{text}` cannot call itself; lift it to a top-level `fn` (design §4.5)"
            );
            self.cx
                .report(diag::error(diag::RECURSIVE_LAMBDA, file, range, msg));
            return;
        }
        let flipped = if name.kind() == IDENT {
            upper_first(text)
        } else {
            lower_first(text)
        };
        let flipped_resolves = self.lookup_local(&flipped).is_some()
            || self.cx.lookup_module_name(self.m, &flipped).is_some();
        if flipped_resolves {
            let msg = format!(
                "cannot find `{text}`; `{flipped}` exists but names differ in case (values are lowercase, types are uppercase)"
            );
            let d = diag::error(diag::CASE_MISMATCH, file, range, msg).with_fix(diag::replace_fix(
                file,
                range,
                format!("Use `{flipped}`"),
                &flipped,
            ));
            self.cx.report(d);
            return;
        }
        let mut msg = format!("cannot find `{text}` in this scope");
        if let Some(p) = crate::prelude::lookup(text) {
            msg = format!(
                "`{text}` is in the prelude but std module `std/{p}` is not part of this build"
            );
        }
        let mut cands = self.local_names();
        cands.extend(self.cx.module_candidates(self.m));
        let mut d = diag::error(diag::UNRESOLVED_NAME, file, range, msg);
        let import = self.cx.std_import_fix(self.m, text, false).or_else(|| {
            (name.kind() == IDENT)
                .then(|| self.cx.std_import_fix(self.m, text, true))
                .flatten()
        });
        if let Some(fix) = import {
            d = d.with_fix(fix);
        } else if let Some(s) = did_you_mean(text, cands.iter().map(String::as_str)) {
            d.message.push_str(&format!("; did you mean `{s}`?"));
            d = d.with_fix(diag::replace_fix(file, range, format!("Use `{s}`"), s));
        }
        self.cx.report(d);
    }

    fn record_type(&mut self, n: &SyntaxNode) -> TypeRes {
        let Some(path) = child(n, PATH).or_else(|| child(n, PATH_EXPR)) else {
            return TypeRes::Err;
        };
        let res = self
            .cx
            .resolve_type_path(self.m, &path, self.generics, TypeNs::Type);
        if let TypeRes::Def(d) = res {
            let ok = matches!(
                &self.cx.program.def(d).kind,
                DefKind::Error(_)
                    | DefKind::Type(TypeDef {
                        body: TypeDefBody::Record(_),
                        ..
                    })
            );
            if !ok {
                let file = self.file();
                let name = self.cx.program.def(d).name.clone();
                let msg = format!(
                    "`{name}` is not a record type or error tag, so it has no `{{ field: … }}` form"
                );
                self.cx
                    .report(diag::error(diag::WRONG_KIND, file, path.text_range(), msg));
                return TypeRes::Err;
            }
            if !self.cx.program.constructors_visible(d, self.m) {
                self.cx.report_opaque(self.m, d, path.text_range());
                return TypeRes::Err;
            }
        }
        res
    }

    fn record(&mut self, n: &SyntaxNode) -> Expr {
        let ty = self.record_type(n);
        let spread =
            child(n, RECORD_SPREAD).map(|s| self.opt_expr(exprs(&s).next(), s.text_range()));
        let mut fields = Vec::new();
        for f in children(n, FIELD_INIT) {
            let Some(name) = token(&f, IDENT) else {
                continue;
            };
            let value = match exprs(&f).next() {
                Some(e) => self.expr(&e),
                None => {
                    // Punning: `User { id }` means `User { id: id }`.
                    let res = match self.lookup_local(name.text()) {
                        Some(l) => Res::Local(l),
                        None => match self.cx.lookup_module_name(self.m, name.text()) {
                            Some(Binding::Def(d)) => Res::Def(d),
                            _ => {
                                self.unresolved(&name);
                                Res::Err
                            }
                        },
                    };
                    self.alloc(Expr::Path(res), name.text_range())
                }
            };
            fields.push(FieldInit {
                name: name.text().to_string(),
                range: name.text_range(),
                value,
            });
        }
        Expr::Record { ty, spread, fields }
    }

    fn lambda(&mut self, n: &SyntaxNode) -> Expr {
        self.push_scope();
        let mut params = Vec::new();
        if let Some(list) = child(n, PARAM_LIST) {
            for p in children(&list, PARAM) {
                let ty = first_type(&p).map(|t| self.ty(&t));
                let local = self.param_local(&p);
                params.push(LambdaParam { local, ty });
            }
        }
        let ret = child(n, RET_TYPE)
            .and_then(|r| first_type(&r))
            .map(|t| self.ty(&t));
        let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
        let body = self.opt_expr(child(n, BLOCK), n.text_range());
        self.loop_depth = saved_loop;
        self.pop_scope();
        Expr::Lambda { params, ret, body }
    }

    fn handler(&mut self, n: &SyntaxNode) -> Expr {
        let effect = match n.children().find(|c| matches!(c.kind(), PATH | NAMED_TYPE)) {
            Some(p) => self
                .cx
                .resolve_type_path(self.m, &p, self.generics, TypeNs::Handler),
            None => {
                let has_name = token(n, TYPE_NAME).is_some();
                if has_name {
                    self.cx
                        .resolve_type_path(self.m, n, self.generics, TypeNs::Handler)
                } else {
                    TypeRes::Err
                }
            }
        };
        let ops: Vec<(String, DefId)> = match effect {
            TypeRes::Def(d) => match &self.cx.program.def(d).kind {
                DefKind::Effect(e) => {
                    let module = self.cx.program.module(d.module);
                    e.ops
                        .iter()
                        .map(|&o| {
                            (
                                module.defs[o].name.clone(),
                                DefId {
                                    module: d.module,
                                    index: o,
                                },
                            )
                        })
                        .collect()
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        // State is visible in clauses only, not in the initializers of other state.
        self.push_scope();
        let mut state = Vec::new();
        for v in children(n, VAR_STMT) {
            let ty = first_type(&v).map(|t| self.ty(&t));
            let init = self.opt_expr(exprs(&v).last(), v.text_range());
            let (name, range) = token(&v, IDENT)
                .map(|t| (t.text().to_string(), t.text_range()))
                .unwrap_or_else(|| (String::new(), v.text_range()));
            let local = self.alloc_local(&name, range, true);
            state.push(HandlerState { local, ty, init });
        }
        for s in &state {
            let name = self.body.locals[s.local].name.clone();
            self.define(&name, s.local);
        }
        let mut clauses = Vec::new();
        for c in children(n, HANDLER_CLAUSE) {
            let Some(name) = token(&c, IDENT) else {
                continue;
            };
            let op = ops.iter().find(|(o, _)| o == name.text()).map(|&(_, d)| d);
            if op.is_none() && effect != TypeRes::Err {
                let file = self.file();
                let eff = match effect {
                    TypeRes::Def(d) => self.cx.program.def(d).name.clone(),
                    _ => String::new(),
                };
                let mut d = diag::error(
                    diag::UNRESOLVED_NAME,
                    file,
                    name.text_range(),
                    format!("effect `{eff}` has no operation `{}`", name.text()),
                );
                if let Some(s) = did_you_mean(name.text(), ops.iter().map(|(o, _)| o.as_str())) {
                    d.message.push_str(&format!("; did you mean `{s}`?"));
                    d = d.with_fix(diag::replace_fix(
                        file,
                        name.text_range(),
                        format!("Use `{s}`"),
                        s,
                    ));
                }
                self.cx.report(d);
            }
            self.push_scope();
            let mut params = Vec::new();
            if let Some(list) = child(&c, PARAM_LIST) {
                for p in children(&list, PARAM) {
                    params.push(self.param_local(&p));
                }
            }
            let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
            let body = self.opt_expr(child(&c, BLOCK), c.text_range());
            self.loop_depth = saved_loop;
            self.pop_scope();
            clauses.push(HandlerClause {
                op,
                name: name.text().to_string(),
                range: name.text_range(),
                params,
                body,
            });
        }
        self.pop_scope();
        Expr::Handler(HandlerExpr {
            effect,
            state,
            clauses,
        })
    }

    fn arms(&mut self, list: &SyntaxNode) -> Vec<MatchArm> {
        let mut arms = Vec::new();
        for a in children(list, MATCH_ARM) {
            self.push_scope();
            let pat = self.pattern_opt(a.children().find(|c| is_pat(c.kind())), a.text_range());
            let guard =
                child(&a, MATCH_GUARD).map(|g| self.opt_expr(exprs(&g).next(), g.text_range()));
            let body = self.opt_expr(exprs(&a).next(), a.text_range());
            self.pop_scope();
            arms.push(MatchArm { pat, guard, body });
        }
        arms
    }

    // ---- Blocks and statements ----

    fn block(&mut self, n: &SyntaxNode) -> Expr {
        self.push_scope();
        let mut stmts = Vec::new();
        let items: Vec<SyntaxNode> = n
            .children()
            .filter(|c| {
                matches!(
                    c.kind(),
                    LET_STMT
                        | VAR_STMT
                        | ASSIGN_STMT
                        | FOR_STMT
                        | WHILE_STMT
                        | RETURN_STMT
                        | BREAK_STMT
                        | CONTINUE_STMT
                        | EXPR_STMT
                ) || is_expr(c.kind())
            })
            .collect();
        for s in &items {
            if let Some(stmt) = self.stmt(s) {
                stmts.push(stmt);
            }
        }
        let tail = match stmts.last() {
            Some(Stmt::Expr(e))
                if items
                    .last()
                    .is_some_and(|s| s.kind() == EXPR_STMT || is_expr(s.kind())) =>
            {
                let e = *e;
                stmts.pop();
                Some(e)
            }
            _ => None,
        };
        self.pop_scope();
        Expr::Block { stmts, tail }
    }

    fn stmt(&mut self, s: &SyntaxNode) -> Option<Stmt> {
        let range = s.text_range();
        Some(match s.kind() {
            LET_STMT => {
                let ty = first_type(s).map(|t| self.ty(&t));
                let init_node = exprs(s).last();
                let pat_node = s.children().find(|c| is_pat(c.kind()));
                let lambda_name = match (&pat_node, &init_node) {
                    (Some(p), Some(i)) if p.kind() == BIND_PAT && i.kind() == LAMBDA_EXPR => {
                        token(p, IDENT).map(|t| t.text().to_string())
                    }
                    _ => None,
                };
                if let Some(l) = &lambda_name {
                    self.lambda_lets.push(l.clone());
                }
                let init = self.opt_expr(init_node, range);
                if lambda_name.is_some() {
                    self.lambda_lets.pop();
                }
                let before = self.body.locals.len();
                let pat = self.pattern_opt(pat_node, range);
                if let Some(kw) = token(s, LET_KW) {
                    for raw in before..self.body.locals.len() {
                        let l = LocalId::from_raw(la_arena::RawIdx::from(raw as u32));
                        self.let_kw.insert(l, kw.text_range());
                    }
                }
                Stmt::Let { pat, ty, init }
            }
            VAR_STMT => {
                let ty = first_type(s).map(|t| self.ty(&t));
                let init = self.opt_expr(exprs(s).last(), range);
                let (name, nrange) = token(s, IDENT)
                    .map(|t| (t.text().to_string(), t.text_range()))
                    .unwrap_or_else(|| (String::new(), range));
                let local = self.bind(&name, nrange, true);
                Stmt::Var { local, ty, init }
            }
            ASSIGN_STMT => self.assign(s),
            FOR_STMT => {
                let iterable = self.opt_expr(exprs(s).find(|e| e.kind() != BLOCK), range);
                self.push_scope();
                let pat = self.pattern_opt(s.children().find(|c| is_pat(c.kind())), range);
                self.loop_depth += 1;
                let body = self.opt_expr(child(s, BLOCK), range);
                self.loop_depth -= 1;
                self.pop_scope();
                let e = self.alloc(
                    Expr::For {
                        pat,
                        iterable,
                        body,
                    },
                    range,
                );
                Stmt::Expr(e)
            }
            WHILE_STMT => {
                let es: Vec<SyntaxNode> = exprs(s).collect();
                let cond = self.opt_expr(es.first().cloned(), range);
                self.loop_depth += 1;
                let body = self.opt_expr(es.get(1).cloned().filter(|b| b.kind() == BLOCK), range);
                self.loop_depth -= 1;
                let brk = self.alloc(Expr::Break, range);
                let iff = self.alloc(
                    Expr::If {
                        cond,
                        then_branch: body,
                        else_branch: Some(brk),
                    },
                    range,
                );
                Stmt::Expr(self.alloc(Expr::Loop { body: iff }, range))
            }
            RETURN_STMT => {
                let v = exprs(s).next().map(|e| self.expr(&e));
                Stmt::Expr(self.alloc(Expr::Return(v), range))
            }
            BREAK_STMT | CONTINUE_STMT => {
                let is_break = s.kind() == BREAK_STMT;
                if self.loop_depth == 0 {
                    let file = self.file();
                    let kw = if is_break { "break" } else { "continue" };
                    let msg = format!("`{kw}` outside of a `for` or `while` loop");
                    self.cx
                        .report(diag::error(diag::JUMP_OUTSIDE_LOOP, file, range, msg));
                }
                let e = if is_break {
                    Expr::Break
                } else {
                    Expr::Continue
                };
                Stmt::Expr(self.alloc(e, range))
            }
            EXPR_STMT => Stmt::Expr(self.opt_expr(exprs(s).next(), range)),
            k if is_expr(k) => Stmt::Expr(self.expr(s)),
            _ => return None,
        })
    }

    fn assign(&mut self, s: &SyntaxNode) -> Stmt {
        let range = s.text_range();
        // Target names: direct `IDENT` tokens before `=`, or a path/field expression node.
        let mut names: Vec<SyntaxToken> = s
            .children_with_tokens()
            .take_while(|e| e.kind() != EQ)
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == IDENT)
            .collect();
        let all: Vec<SyntaxNode> = exprs(s).collect();
        let value_node = all.last().cloned();
        if names.is_empty()
            && all.len() >= 2
            && let Some(target) = all.first()
        {
            // Only `x` and `x.a.b` are targets; the parser reported anything else (`V0013`),
            // so such an assignment keeps no target and adds no type error.
            if !is_place(target) {
                let value = self.opt_expr(value_node, range);
                return Stmt::Assign {
                    target: None,
                    fields: Vec::new(),
                    value,
                    range,
                };
            }
            names = target
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| t.kind() == IDENT)
                .collect();
        }
        let value = self.opt_expr(value_node, range);
        let file = self.file();
        let target = match names.first() {
            Some(first) => match self.lookup_local(first.text()) {
                Some(l) if self.body.locals[l].mutable => Some(l),
                Some(l) => {
                    let msg = format!(
                        "cannot assign to `{}`: it is bound by `let`, which is immutable",
                        first.text()
                    );
                    let mut d = diag::error(diag::ASSIGN_IMMUTABLE, file, first.text_range(), msg)
                        .with_span(
                            diag::span(file, self.body.locals[l].range)
                                .with_role(sakura_diagnostics::SpanRole::Definition)
                                .with_label("bound here"),
                        );
                    if let Some(&kw) = self.let_kw.get(&l) {
                        d = d.with_fix(diag::replace_fix(
                            file,
                            kw,
                            "Declare it with `var`".into(),
                            "var",
                        ));
                    }
                    self.cx.report(d);
                    None
                }
                None => {
                    if self.cx.lookup_module_name(self.m, first.text()).is_some() {
                        let msg = format!(
                            "cannot assign to `{}`: only local `var` bindings and handler state can be assigned",
                            first.text()
                        );
                        self.cx.report(diag::error(
                            diag::ASSIGN_IMMUTABLE,
                            file,
                            first.text_range(),
                            msg,
                        ));
                    } else {
                        self.unresolved(first);
                    }
                    None
                }
            },
            None => None,
        };
        Stmt::Assign {
            target,
            fields: names.iter().skip(1).map(|t| t.text().to_string()).collect(),
            value,
            range,
        }
    }

    // ---- Patterns ----

    fn pattern_opt(&mut self, n: Option<SyntaxNode>, fallback: TextRange) -> PatId {
        match n {
            Some(n) => {
                let mut bound = Vec::new();
                self.pattern(&n, &mut bound)
            }
            None => self.alloc_pat(Pat::Missing, fallback),
        }
    }

    /// Lowers a pattern; `bound` collects the names bound so far in this pattern so that
    /// or-alternatives share locals and duplicates are reported.
    fn pattern(&mut self, n: &SyntaxNode, bound: &mut Vec<(String, LocalId)>) -> PatId {
        let range = n.text_range();
        let p = match n.kind() {
            WILDCARD_PAT => Pat::Wild,
            BIND_PAT => match token(n, IDENT) {
                Some(t) => Pat::Bind(self.bind_pat(&t, bound)),
                None => Pat::Missing,
            },
            LITERAL_PAT => {
                let neg = token(n, MINUS).is_some();
                let inner = child(n, LITERAL)
                    .or_else(|| child(n, STRING_LIT))
                    .unwrap_or_else(|| n.clone());
                let lit = if inner.kind() == STRING_LIT {
                    Some(Literal::String(string_text(&inner)))
                } else {
                    lit_of(&inner)
                };
                match (lit, neg) {
                    (Some(Literal::Int(i)), true) => Pat::Lit(Literal::Int(i.wrapping_neg())),
                    (Some(Literal::Float(f)), true) => Pat::Lit(Literal::Float(-f)),
                    (Some(l), false) => Pat::Lit(l),
                    _ => Pat::Missing,
                }
            }
            PATH_PAT | VARIANT_PAT => {
                let segs = path_segments(n);
                let res = match segs.last() {
                    Some(last) => match self.value_binding(&segs) {
                        Some(b) => self.constructor(b, &last.clone()),
                        None => Res::Err,
                    },
                    None => Res::Err,
                };
                let args = n
                    .children()
                    .filter(|c| is_pat(c.kind()))
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|c| self.pattern(c, bound))
                    .collect();
                Pat::Ctor { res, args }
            }
            RECORD_PAT => {
                let ty = self.record_type(n);
                let rest = token(n, DOT2).is_some() || child(n, REST_PAT).is_some();
                let mut fields = Vec::new();
                for f in children(n, FIELD_PAT) {
                    let Some(name) = token(&f, IDENT) else {
                        continue;
                    };
                    let pat = match f.children().find(|c| is_pat(c.kind())) {
                        Some(p) => self.pattern(&p, bound),
                        None => {
                            let l = self.bind_pat(&name, bound);
                            self.alloc_pat(Pat::Bind(l), name.text_range())
                        }
                    };
                    fields.push(FieldPat {
                        name: name.text().to_string(),
                        range: name.text_range(),
                        pat,
                    });
                }
                Pat::Record { ty, fields, rest }
            }
            TUPLE_PAT => {
                let ps: Vec<SyntaxNode> = n.children().filter(|c| is_pat(c.kind())).collect();
                Pat::Tuple(ps.iter().map(|p| self.pattern(p, bound)).collect())
            }
            LIST_PAT => {
                let ps: Vec<SyntaxNode> = n.children().filter(|c| is_pat(c.kind())).collect();
                let items = ps.iter().map(|p| self.pattern(p, bound)).collect();
                let rest = child(n, REST_PAT)
                    .map(|r| token(&r, IDENT).map(|t| self.bind_pat(&t, bound)))
                    .or_else(|| token(n, DOT2).map(|_| None));
                Pat::List { items, rest }
            }
            OR_PAT => {
                let ps: Vec<SyntaxNode> = n.children().filter(|c| is_pat(c.kind())).collect();
                let outer = bound.len();
                let saved = std::mem::take(&mut self.shared_or);
                let mut alts = Vec::new();
                let mut first: Option<Vec<(String, LocalId)>> = None;
                for p in &ps {
                    let mut alt_bound = bound[..outer].to_vec();
                    alts.push(self.pattern(p, &mut alt_bound));
                    let names = alt_bound.split_off(outer);
                    match &first {
                        None => {
                            // Later alternatives reuse these locals by name.
                            self.shared_or = names.clone();
                            first = Some(names);
                        }
                        Some(f) => {
                            let set = |v: &[(String, LocalId)]| {
                                let mut s: Vec<String> = v.iter().map(|(n, _)| n.clone()).collect();
                                s.sort();
                                s
                            };
                            let (a, b) = (set(f), set(&names));
                            if a != b {
                                let file = self.file();
                                let msg = format!(
                                    "alternatives of an or-pattern must bind the same names: the first binds {{{}}}, this one binds {{{}}}",
                                    a.join(", "),
                                    b.join(", ")
                                );
                                self.cx.report(
                                    sakura_diagnostics::Diagnostic::error(
                                        diag::OR_PATTERN_BINDINGS,
                                        sakura_diagnostics::Phase::Pattern,
                                        msg,
                                    )
                                    .with_span(diag::span(file, p.text_range())),
                                );
                            }
                        }
                    }
                }
                self.shared_or = saved;
                bound.extend(first.unwrap_or_default());
                Pat::Or(alts)
            }
            _ => Pat::Missing,
        };
        self.alloc_pat(p, range)
    }

    fn bind_pat(&mut self, t: &SyntaxToken, bound: &mut Vec<(String, LocalId)>) -> LocalId {
        let name = t.text();
        if let Some(&(_, l)) = bound.iter().find(|(n, _)| n == name) {
            let file = self.file();
            let msg = format!("`{name}` is bound more than once in this pattern");
            self.cx
                .report(diag::error(diag::DUPLICATE, file, t.text_range(), msg));
            return l;
        }
        let l = match self.shared_or.iter().find(|(n, _)| n == name) {
            Some(&(_, l)) => l,
            None => self.bind(name, t.text_range(), false),
        };
        bound.push((name.to_string(), l));
        l
    }
}

fn same_named_variant(p: &Program, ty: DefId, t: &TypeDef) -> Option<DefId> {
    let TypeDefBody::Variants(vs) = &t.body else {
        return None;
    };
    let module = p.module(ty.module);
    let name = &module.defs[ty.index].name;
    vs.iter()
        .find(|&&v| &module.defs[v].name == name)
        .map(|&index| DefId {
            module: ty.module,
            index,
        })
}

/// `x` or `x.a.b`: the assignable expressions.
fn is_place(n: &SyntaxNode) -> bool {
    let mut n = n.clone();
    loop {
        match n.kind() {
            PATH_EXPR => return path_segments(&n).len() == 1,
            FIELD_EXPR => match field_parts(&n) {
                Some((base, name)) if name.kind() == IDENT => n = base,
                _ => return false,
            },
            _ => return false,
        }
    }
}

/// The base expression and member token of a `FIELD_EXPR` (`IDENT`, `TYPE_NAME` or `INT`).
fn field_parts(n: &SyntaxNode) -> Option<(SyntaxNode, SyntaxToken)> {
    let base = exprs(n).next()?;
    let name = n
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| matches!(t.kind(), IDENT | TYPE_NAME | INT))
        .last()?;
    Some((base, name))
}

fn lit_of(n: &SyntaxNode) -> Option<Literal> {
    let t = n
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| !t.kind().is_trivia())?;
    Some(match t.kind() {
        INT => Literal::Int(parse_int(t.text())?),
        FLOAT => Literal::Float(t.text().replace('_', "").parse().ok()?),
        TRUE_KW => Literal::Bool(true),
        FALSE_KW => Literal::Bool(false),
        _ => return None,
    })
}

fn parse_int(s: &str) -> Option<i64> {
    let s = s.replace('_', "");
    if let Some(h) = s.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()
    } else if let Some(b) = s.strip_prefix("0b") {
        i64::from_str_radix(b, 2).ok()
    } else {
        s.parse().ok()
    }
}

fn binop(k: sakura_syntax::SyntaxKind) -> Option<BinaryOp> {
    Some(match k {
        PIPE2 => BinaryOp::Or,
        AMP2 => BinaryOp::And,
        EQ2 => BinaryOp::Eq,
        NEQ => BinaryOp::Ne,
        LT => BinaryOp::Lt,
        LTEQ => BinaryOp::Le,
        GT => BinaryOp::Gt,
        GTEQ => BinaryOp::Ge,
        DOT2 => BinaryOp::Range,
        PLUS => BinaryOp::Add,
        MINUS => BinaryOp::Sub,
        STAR => BinaryOp::Mul,
        SLASH => BinaryOp::Div,
        PERCENT => BinaryOp::Rem,
        _ => return None,
    })
}
