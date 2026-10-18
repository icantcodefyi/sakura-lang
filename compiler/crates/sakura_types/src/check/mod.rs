//! The bidirectional checker over HIR.
//!
//! [`Checker`] owns inference state for the whole program. Declarations (signatures, type
//! definitions, effects) are lowered lazily and cached; bodies are checked one at a time and
//! their results zonked into [`TypedBody`]s.
//!
//! Stack safety: expression chains are as deep as they are long (the parser bounds nesting,
//! not `a.m().m()…` or `1 + 1 + …`), so the two walks that recurse per link, `infer` and
//! the `V3001` cause search, run under [`sakura_hir::ensure_stack`]. Pattern, type and
//! exhaustiveness recursion is bounded by syntactic nesting; `shallow` links to roots.

mod body;
mod consteval;
mod decls;
mod effects;
mod escape;
mod expr;
mod holes;
mod pat;
mod refine;

use std::collections::{HashMap, HashSet};

use la_arena::ArenaMap;
use sakura_diagnostics::{Code, Diagnostic, Phase, Rendered, Span, SpanRole};
use sakura_hir::{Body, BodyId, DefId, DefKind, ExprId, LocalId, ModuleId, PatId, Program};
use text_size::TextRange;

use crate::infer::Infer;
use crate::render::Names;
use crate::ty::{
    Constraint, Interner, Label, ParamId, RowId, RowParamId, Scheme, Tail, Type, TypeId,
};
use crate::{CallTarget, Hole, TypedBody, TypedProgram};

pub(crate) use decls::GenericSlot;

/// Display names for HIR definitions.
pub(crate) struct ProgramNames<'p>(pub &'p Program);

impl Names for ProgramNames<'_> {
    fn def_name(&self, def: DefId) -> String {
        self.0.def(def).name.clone()
    }
}

#[derive(Clone, Debug)]
enum LocalTy {
    Mono(TypeId),
    Poly(Scheme),
}

/// A pending constraint check (`A: Eq` instantiated, `==`, interpolation …).
#[derive(Clone, Debug)]
struct Obligation {
    ty: TypeId,
    constraint: Constraint,
    at: ExprId,
}

/// A function-like scope: the body of a top-level fn, a lambda or a handler clause.
#[derive(Clone, Copy, Debug)]
struct FnFrame {
    ret: TypeId,
}

/// Where a row had to fit an upper bound; turned into V3001/V3020 once the body is solved.
#[derive(Clone, Debug)]
struct RowViolation {
    /// Expression whose row exceeded the bound; causes are searched beneath it.
    root: ExprId,
    primary: TextRange,
    labels: Vec<Label>,
    actual: RowId,
    expected: RowId,
    kind: ViolationKind,
    /// The bound was an open row without `Fail`: a Fail-free row variable (§5.2 rule 8).
    fail_free_tail: bool,
}

#[derive(Clone, Debug)]
enum ViolationKind {
    /// A top-level function body exceeds its signature; the fix edits `row_range` / inserts
    /// after `ret_end`.
    Signature {
        def: DefId,
        row_range: Option<TextRange>,
        ret_end: TextRange,
        body_range: TextRange,
    },
    /// A lambda passed or annotated with a narrower function type.
    Lambda,
    /// A handler clause performs an effect its handler type does not allow.
    Clause { op: String, eff: String },
    /// A handler clause fails with a tag its operation does not declare (§5.5).
    ClauseFail { op: String },
    /// A test body uses an effect without a handler (V3020).
    Test,
    /// A constant initializer, refinement predicate or contract must be pure.
    Pure { what: &'static str, code: Code },
}

/// A `match`, `let` pattern or `catch` whose coverage is checked after solving.
#[derive(Clone, Debug)]
struct CoverageCheck {
    at: ExprId,
    range: TextRange,
    arms: Vec<(PatId, bool)>,
    kind: CoverageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoverageKind {
    Match,
    /// `body_row`: the `try` body's row; if its tail hides failures (§5.2 rule 8), a
    /// catch-all arm also catches those and is reachable.
    Catch {
        body_row: RowId,
    },
    Let,
}

/// Per-body state, reset for each body.
#[derive(Default)]
struct BodyState {
    module: Option<ModuleId>,
    body_id: Option<BodyId>,
    generics: Vec<GenericSlot>,
    types: ArenaMap<ExprId, TypeId>,
    rows: ArenaMap<ExprId, RowId>,
    pat_types: ArenaMap<PatId, TypeId>,
    locals: ArenaMap<LocalId, LocalTy>,
    local_depth: ArenaMap<LocalId, u32>,
    /// How many `try` bodies enclose each local's declaration (§17.43, V2041).
    local_try_depth: ArenaMap<LocalId, u32>,
    calls: ArenaMap<ExprId, CallTarget>,
    instantiations: ArenaMap<ExprId, Vec<TypeId>>,
    conversions: ArenaMap<ExprId, (DefId, Vec<TypeId>)>,
    unwrapped: HashSet<ExprId>,
    scope: Vec<LocalId>,
    frames: Vec<FnFrame>,
    lambda_depth: u32,
    try_depth: u32,
    allowed_row: Option<RowId>,
    holes: Vec<(ExprId, String, TypeId, Vec<LocalId>)>,
    unused: Vec<ExprId>,
    obligations: Vec<Obligation>,
    violations: Vec<RowViolation>,
    coverage: Vec<CoverageCheck>,
    /// Expressions already reported, to avoid a second diagnostic on the same node.
    reported: HashSet<ExprId>,
    /// Clause parameters that are callbacks (§17.48): calling one adds only its row's tail.
    callbacks: HashSet<LocalId>,
    /// `@trust(…)` expressions passed directly as call arguments (the only place a `Trust`
    /// may appear, §6.4).
    trust_args: HashSet<ExprId>,
    /// `(call, expression holding its instantiation)` of every `Untrusted.validate` call
    /// (§17.71), resolved once the body is solved.
    validates: Vec<(ExprId, ExprId)>,
    /// `Untrusted.validate` paths in callee position.
    validate_callees: HashSet<ExprId>,
    /// The body contains a syntax error: its type diagnostics would be cascades.
    suppress: bool,
    /// Empty list literals, checked for an inferable element type (V2010).
    empty_lists: Vec<ExprId>,
    /// Initializers of generalized `let`s (their leftover variables are quantified).
    generalized: HashSet<ExprId>,
    /// `(try, body)` of every `try` without `catch` whose body row ended in a flexible tail:
    /// if the tail later turns out to hide failures, the `try` is `V3003` (§5.3).
    try_checks: Vec<(ExprId, ExprId)>,
    /// `(error type, rest of the message)` for a `catch` arm pattern that matches nothing
    /// the body is known to raise (§17.100), set while the arm patterns are bound.
    catch_note: Option<(TypeId, String)>,
}

/// Generic parameters, aliases and fields per definition. A reused module keeps its entries,
/// so every `ParamId` its bodies and schemes mention stays the same.
#[derive(Debug, Default, Clone)]
pub(crate) struct DeclCache {
    generics: HashMap<DefId, Vec<GenericSlot>>,
    aliases: HashMap<DefId, TypeId>,
    fields: HashMap<DefId, Vec<(String, TypeId)>>,
}

pub(crate) struct Checker<'p> {
    program: &'p Program,
    inf: Infer,
    diags: Vec<Diagnostic>,
    // declaration caches (see decls.rs)
    generics: HashMap<DefId, Vec<GenericSlot>>,
    schemes: HashMap<DefId, Scheme>,
    in_progress: HashSet<DefId>,
    aliases: HashMap<DefId, TypeId>,
    fields: HashMap<DefId, Vec<(String, TypeId)>>,
    checked: HashSet<DefId>,
    /// Effects every test may use without a handler (design §5.8).
    test_defaults: Vec<DefId>,
    /// `(file, offset)` of every lex/parse error.
    syntax_errors: Vec<(sakura_diagnostics::FileId, u32)>,
    out: TypedProgram,
    b: BodyState,
    body: Option<&'p Body>,
    /// Which nominal types hold scoped values, resources or closures (`escape.rs`).
    escape_memo: HashMap<(DefId, escape::Holds), bool>,
    /// Literals into refined types, decided after every body (`refine.rs`, §17.87).
    literals: Vec<refine::PendingLiteral>,
}

pub(crate) fn check(
    program: &Program,
    syntax_errors: &[Diagnostic],
    reuse: Option<(&TypedProgram, &dyn Fn(ModuleId) -> bool)>,
) -> (TypedProgram, Vec<Diagnostic>) {
    let syntax_errors = syntax_errors
        .iter()
        .chain(&program.diagnostics)
        .filter(|d| matches!(d.phase, Phase::Lex | Phase::Parse))
        .filter_map(|d| d.spans.first().map(|s| (s.file, s.start)))
        .collect();
    let mut cx = Checker {
        syntax_errors,
        program,
        inf: Infer::new(),
        diags: Vec::new(),
        generics: HashMap::new(),
        schemes: HashMap::new(),
        in_progress: HashSet::new(),
        aliases: HashMap::new(),
        fields: HashMap::new(),
        checked: HashSet::new(),
        test_defaults: Vec::new(),
        out: TypedProgram::default(),
        b: BodyState::default(),
        body: None,
        escape_memo: HashMap::new(),
        literals: Vec::new(),
    };
    let reused = |m: ModuleId| reuse.is_some_and(|(_, keep)| keep(m));
    if let Some((prev, _)) = reuse {
        // Reused bodies hold the previous `TypeId`s and their defs' `ParamId`s, so start from
        // the previous interner and declarations, and never check those defs again. That
        // includes the schemeless ones (tests, types, effects), whose bodies and holes are copied
        // below and whose diagnostics the caller keeps.
        cx.inf = Infer::continuing(prev.interner.clone());
        for (def, s) in prev.defs.iter().filter(|(d, _)| reused(d.module)) {
            debug_assert!(
                ty_var(&prev.interner, s.ty, ANY, &mut HashSet::new()).is_none(),
                "reused scheme of {def:?} mentions an inference variable"
            );
            cx.schemes.insert(*def, s.clone());
        }
        for (mi, module) in program.modules.iter().enumerate() {
            let module_id = ModuleId(mi as u32);
            if reused(module_id) {
                let defs = module.defs.iter().map(|(index, _)| DefId {
                    module: module_id,
                    index,
                });
                cx.checked.extend(defs);
            }
        }
        let keep = |d: &&DefId| reused(d.module);
        for (d, g) in prev.decls.generics.iter().filter(|(d, _)| keep(d)) {
            cx.generics.insert(*d, g.clone());
        }
        for (d, t) in prev.decls.aliases.iter().filter(|(d, _)| keep(d)) {
            cx.aliases.insert(*d, *t);
        }
        for (d, f) in prev.decls.fields.iter().filter(|(d, _)| keep(d)) {
            cx.fields.insert(*d, f.clone());
        }
    }
    let prev_vars = reuse
        .filter(|_| cfg!(debug_assertions))
        .map(|(prev, _)| prev.interner.var_bounds());
    cx.prepare();
    for (mi, module) in program.modules.iter().enumerate() {
        let mid = ModuleId(mi as u32);
        for (idx, _) in module.defs.iter() {
            cx.check_def(DefId {
                module: mid,
                index: idx,
            });
        }
    }
    if let Some((prev, _)) = reuse {
        let reused_file = |f| {
            program
                .modules
                .iter()
                .enumerate()
                .any(|(i, m)| m.file == f && reused(ModuleId(i as u32)))
        };
        cx.diags
            .retain(|d| d.spans.first().is_none_or(|s| !reused_file(s.file)));
        for (key, body) in prev.bodies.iter().filter(|((m, _), _)| reused(*m)) {
            cx.out.bodies.insert(*key, body.clone());
        }
        let checked = cx.out.bodies.iter().filter(|((m, _), _)| !reused(*m));
        for (key, body) in checked.filter(|_| prev_vars.is_some()) {
            let old = prev_vars.and_then(|below| body_var(&cx.inf.interner, body, below));
            assert!(
                old.is_none(),
                "checked body {key:?} mentions {}, numbered like the reused analysis' own",
                old.unwrap_or_default()
            );
        }
        let holes = prev.holes.iter().filter(|h| reused(h.module)).cloned();
        cx.out.holes.extend(holes);
        cx.out.holes.sort_by_key(|h| h.module);
    }
    cx.decide_literals();
    let mut out = std::mem::take(&mut cx.out);
    for (def, s) in &cx.schemes {
        out.defs.insert(*def, s.clone());
    }
    out.interner = std::mem::take(&mut cx.inf.interner);
    out.decls = DeclCache {
        generics: std::mem::take(&mut cx.generics),
        aliases: std::mem::take(&mut cx.aliases),
        fields: std::mem::take(&mut cx.fields),
    };
    (out, cx.diags)
}

impl<'p> Checker<'p> {
    fn names(&self) -> ProgramNames<'p> {
        ProgramNames(self.program)
    }

    fn render(&mut self, ty: TypeId) -> String {
        let names = self.names();
        self.inf.render(ty, &names)
    }

    fn render_row(&mut self, row: RowId) -> String {
        let names = self.names();
        self.inf.render_row(row, &names)
    }

    fn render_label(&mut self, label: &Label) -> String {
        let names = self.names();
        self.inf.render_label(label, &names)
    }

    fn rendered_row(&mut self, row: RowId) -> Rendered {
        let names = self.names();
        let render = self.inf.render_row(row, &names);
        let s = self.inf.structured_row(row, &names);
        let labels = s
            .labels
            .into_iter()
            .map(|(name, args)| (self.effect_path(&name), args))
            .collect();
        Rendered::row(render, labels, s.tail)
    }

    /// `std::Console` style path for an effect name (falls back to the bare name).
    fn effect_path(&self, name: &str) -> String {
        if name == "Fail" {
            return "std::Fail".to_string();
        }
        for m in &self.program.modules {
            if let Some(&idx) = m.scope.get(name)
                && matches!(m.defs[idx].kind, DefKind::Effect(_))
            {
                let pkg = &self.program.packages[m.package.0 as usize].name;
                return format!("{pkg}::{name}");
            }
        }
        name.to_string()
    }

    /// `pkg/path` of a module, e.g. `std/env`.
    fn module_path(&self, module: ModuleId) -> String {
        let m = self.program.module(module);
        format!(
            "{}/{}",
            self.program.packages[m.package.0 as usize].name, m.path
        )
    }

    /// A callee as written at its home: `Env.get` for an operation, `Option.Some` for a
    /// variant, the bare name otherwise.
    fn callee_name(&self, def: DefId) -> String {
        let m = self.program.module(def.module);
        let d = &m.defs[def.index];
        match &d.kind {
            DefKind::Op(op) => format!("{}.{}", m.defs[op.effect].name, d.name),
            DefKind::Variant(v) => format!("{}.{}", m.defs[v.parent].name, d.name),
            _ => d.name.clone(),
        }
    }

    fn module(&self) -> ModuleId {
        self.b.module.expect("no current module")
    }

    fn file_of(&self, module: ModuleId) -> sakura_diagnostics::FileId {
        self.program.module(module).file
    }

    fn span(&self, range: TextRange) -> Span {
        Span::primary(
            self.file_of(self.module()),
            range.start().into(),
            range.end().into(),
        )
    }

    fn span_in(&self, module: ModuleId, range: TextRange) -> Span {
        Span::primary(
            self.file_of(module),
            range.start().into(),
            range.end().into(),
        )
    }

    fn cause(&self, range: TextRange, label: impl Into<String>) -> Span {
        self.span(range)
            .with_role(SpanRole::Cause)
            .with_label(label)
    }

    fn expr_range(&self, e: ExprId) -> TextRange {
        self.body.map(|b| b.source_map.expr(e)).unwrap_or_default()
    }

    fn pat_range(&self, p: PatId) -> TextRange {
        self.body.map(|b| b.source_map.pat(p)).unwrap_or_default()
    }

    fn error(&mut self, code: Code, phase: Phase, range: TextRange, msg: impl Into<String>) {
        let d = Diagnostic::error(code, phase, msg).with_span(self.span(range));
        self.push(d);
    }

    fn push(&mut self, mut d: Diagnostic) {
        if self.body.is_some() && self.b.suppress {
            return;
        }
        let mut texts: Vec<&mut String> = vec![&mut d.message];
        if let Some(e) = d.expected.as_mut() {
            texts.push(&mut e.render);
        }
        if let Some(a) = d.actual.as_mut() {
            texts.push(&mut a.render);
        }
        for s in d.spans.iter_mut() {
            if let Some(l) = s.label.as_mut() {
                texts.push(l);
            }
        }
        crate::render::tidy_vars(&mut texts);
        self.diags.push(d);
    }

    fn hir_body(&self) -> &'p Body {
        self.body.expect("no current body")
    }

    fn fresh_param_like(&mut self, name: &str, bounds: Vec<Constraint>) -> ParamId {
        self.inf.interner.new_param(name, bounds)
    }

    fn fresh_row_param(&mut self, name: &str, carries_fail: bool) -> RowParamId {
        self.inf.interner.new_row_param(name, carries_fail)
    }

    fn holes_out(&mut self) -> &mut Vec<Hole> {
        &mut self.out.holes
    }
}

pub(crate) fn constraint_of(b: sakura_hir::Bound) -> Constraint {
    match b {
        sakura_hir::Bound::Eq => Constraint::Eq,
        sakura_hir::Bound::Ord => Constraint::Ord,
        sakura_hir::Bound::Hash => Constraint::Hash,
        sakura_hir::Bound::Show => Constraint::Show,
        sakura_hir::Bound::Schema => Constraint::Schema,
    }
}

/// Bounds `(ty, row)` below which [`ty_var`] reports a variable: every one.
const ANY: (u32, u32) = (u32::MAX, u32::MAX);

/// An inference variable numbered below `below` that `b`'s types mention, if any. `check` with
/// reuse starts from the previous interner, whose types may mention the previous check's
/// variables (an unsolved one in a body that failed to check), so its own variables must be
/// numbered after them or the two would alias (design §17.194 (4)).
fn body_var(i: &Interner, b: &TypedBody, below: (u32, u32)) -> Option<String> {
    let mut seen = HashSet::new();
    let schemes = b.local_schemes.values().map(|s| s.ty);
    let tys = (b.types.values().copied())
        .chain(b.pat_types.values().copied())
        .chain(b.locals.values().copied())
        .chain(schemes)
        .chain(b.instantiations.values().flatten().copied())
        .chain(
            b.conversions
                .values()
                .flat_map(|(_, ts)| ts.iter().copied()),
        );
    let rows = b.rows.values().copied();
    tys.map(Ok).chain(rows.map(Err)).find_map(|t| match t {
        Ok(t) => ty_var(i, t, below, &mut seen),
        Err(r) => row_var(i, r, below, &mut seen),
    })
}

fn ty_var(
    i: &Interner,
    t: TypeId,
    below: (u32, u32),
    seen: &mut HashSet<TypeId>,
) -> Option<String> {
    if !seen.insert(t) {
        return None;
    }
    let ty = |t: &TypeId, seen: &mut HashSet<TypeId>| ty_var(i, *t, below, seen);
    match i.ty(t) {
        Type::Var(v) => (v.0 < below.0).then(|| format!("{v:?}")),
        Type::Tuple(ts) | Type::Con(_, ts) => ts.iter().find_map(|t| ty(t, seen)),
        Type::Fn { params, ret, row } => (params.iter().chain([ret]))
            .find_map(|t| ty(t, seen))
            .or_else(|| row_var(i, *row, below, seen)),
        Type::Handler { label, row } => (label.args.iter())
            .find_map(|t| ty(t, seen))
            .or_else(|| row_var(i, *row, below, seen)),
        Type::RowArg(row) => row_var(i, *row, below, seen),
        Type::Union(_) | Type::Never | Type::Param(_) | Type::Error => None,
    }
}

fn row_var(
    i: &Interner,
    r: RowId,
    below: (u32, u32),
    seen: &mut HashSet<TypeId>,
) -> Option<String> {
    let row = i.row_data(r);
    if let Tail::Var(v) = row.tail
        && v.0 < below.1
    {
        return Some(format!("{v:?}"));
    }
    (row.labels.iter().flat_map(|l| &l.args)).find_map(|&t| ty_var(i, t, below, seen))
}
