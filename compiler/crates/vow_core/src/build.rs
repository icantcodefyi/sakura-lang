//! Helpers for constructing Core: fresh locals/labels and a block builder that turns a flat
//! sequence of bindings into nested ANF. Used by lowering and by hand-written tests.

use crate::ir::*;

/// Fresh locals and labels for one [`FnDef`].
#[derive(Debug, Default)]
pub struct FnBuilder {
    pub locals: Vec<LocalDecl>,
    labels: u32,
}

impl FnBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn local(&mut self, name: impl Into<String>, ty: Ty) -> Local {
        self.locals.push(LocalDecl {
            name: name.into(),
            ty,
        });
        Local(self.locals.len() as u32 - 1)
    }

    pub fn label(&mut self) -> Label {
        self.labels += 1;
        Label(self.labels - 1)
    }

    pub fn ty(&self, l: Local) -> &Ty {
        &self.locals[l.index()].ty
    }
}

#[derive(Debug)]
enum Frame {
    Let(Local, Bind, Option<Pos>),
    /// Define a join point; code continues in the `LetJoin` body.
    Join(Label, Vec<Local>, Expr),
    /// Run `Expr` (which jumps to `label`); code continues inside the join point.
    Continue(Label, Vec<Local>, Expr),
    Handle(Atom),
    Try(Label),
}

/// An open sequence of bindings; [`Block::finish`] closes it with a terminator.
#[derive(Debug, Default)]
pub struct Block {
    frames: Vec<Frame>,
}

impl Block {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, local: Local, bind: Bind) {
        self.frames.push(Frame::Let(local, bind, None));
    }

    pub fn push_at(&mut self, local: Local, bind: Bind, pos: Option<Pos>) {
        self.frames.push(Frame::Let(local, bind, pos));
    }

    /// `let label(params) = join in …rest of block`.
    pub fn join(&mut self, label: Label, params: Vec<Local>, join: Expr) {
        self.frames.push(Frame::Join(label, params, join));
    }

    /// Emit `body` (which must jump to `label`) and continue building inside `label(params)`.
    pub fn continue_at(&mut self, label: Label, params: Vec<Local>, body: Expr) {
        self.frames.push(Frame::Continue(label, params, body));
    }

    /// The rest of the block runs with `handler` installed.
    pub fn handle(&mut self, handler: Atom) {
        self.frames.push(Frame::Handle(handler));
    }

    /// The rest of the block runs under a `Try` that fails to `on_fail`.
    pub fn try_(&mut self, on_fail: Label) {
        self.frames.push(Frame::Try(on_fail));
    }

    pub fn finish(self, term: Expr) -> Expr {
        self.frames
            .into_iter()
            .rev()
            .fold(term, |rest, frame| match frame {
                Frame::Let(local, bind, pos) => Expr::Let {
                    local,
                    bind,
                    pos,
                    body: Box::new(rest),
                },
                Frame::Join(label, params, join) => Expr::LetJoin {
                    label,
                    params,
                    join: Box::new(join),
                    body: Box::new(rest),
                },
                Frame::Continue(label, params, body) => Expr::LetJoin {
                    label,
                    params,
                    join: Box::new(rest),
                    body: Box::new(body),
                },
                Frame::Handle(handler) => Expr::Handle {
                    handler,
                    body: Box::new(rest),
                },
                Frame::Try(on_fail) => Expr::Try {
                    on_fail,
                    body: Box::new(rest),
                },
            })
    }
}

/// Shorthand constructors for atoms.
pub fn local(l: Local) -> Atom {
    Atom::Local(l)
}

pub fn int(i: i64) -> Atom {
    Atom::Lit(Lit::Int(i))
}

pub fn str_(s: &str) -> Atom {
    Atom::Lit(Lit::String(s.to_owned()))
}

pub fn unit() -> Atom {
    Atom::Lit(Lit::Unit)
}

pub fn builtin(name: &str) -> Atom {
    debug_assert!(
        crate::builtins::lookup(name).is_some(),
        "unknown builtin {name}"
    );
    Atom::Builtin(name.to_owned())
}
