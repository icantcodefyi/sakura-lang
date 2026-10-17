//! Typed views over the CST. Every accessor returns `Option`/an iterator because the tree may be
//! incomplete; a view never panics on malformed input.
//!
//! Views are zero-cost wrappers around a [`SyntaxNode`] of one kind (or, for the enums
//! [`Item`], [`Type`], [`Stmt`], [`Expr`], [`Pat`], one of several kinds). Node shapes are given
//! in each view's doc comment using the grammar of design §3; trivia, `NEWLINE`, and `;`
//! terminators are not mentioned but may appear between children.

use crate::SyntaxKind::{self, *};
use crate::{SyntaxNode, SyntaxToken};
use rowan::NodeOrToken;
use std::marker::PhantomData;

pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

/// Iterator over the children of a node that cast to `N`.
#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: rowan::SyntaxNodeChildren<crate::VowLanguage>,
    _n: PhantomData<N>,
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;
    fn next(&mut self) -> Option<N> {
        self.inner.by_ref().find_map(N::cast)
    }
}

fn children<N: AstNode>(node: &SyntaxNode) -> AstChildren<N> {
    AstChildren {
        inner: node.children(),
        _n: PhantomData,
    }
}

fn child<N: AstNode>(node: &SyntaxNode) -> Option<N> {
    children(node).next()
}

fn token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find(|t| t.kind() == kind)
}

fn token_in(node: &SyntaxNode, kinds: &[SyntaxKind]) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .find(|t| kinds.contains(&t.kind()))
}

/// The first child node of kind `N` appearing after the first token of kind `after`.
fn child_after<N: AstNode>(node: &SyntaxNode, after: SyntaxKind) -> Option<N> {
    node.children_with_tokens()
        .skip_while(|e| e.kind() != after)
        .filter_map(NodeOrToken::into_node)
        .find_map(N::cast)
}

macro_rules! nodes {
    ($($(#[$m:meta])* $name:ident = $kind:ident;)*) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool { kind == $kind }
            fn cast(node: SyntaxNode) -> Option<Self> {
                Self::can_cast(node.kind()).then(|| $name(node))
            }
            fn syntax(&self) -> &SyntaxNode { &self.0 }
        }
    )*};
}

macro_rules! enums {
    ($($(#[$m:meta])* $name:ident { $($variant:ident),* $(,)? })*) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name { $($variant($variant)),* }
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                $( $variant::can_cast(kind) )||*
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                $( if $variant::can_cast(node.kind()) { return $variant::cast(node).map($name::$variant); } )*
                None
            }
            fn syntax(&self) -> &SyntaxNode {
                match self { $( $name::$variant(n) => n.syntax() ),* }
            }
        }
    )*};
}

nodes! {
    /// `{ Item }`
    SourceFile = SOURCE_FILE;
    /// `@ IDENT [ ArgList ]`, inside an item before `pub`.
    Attribute = ATTRIBUTE;
    /// `{ Attribute } [pub] import ModPath [ ImportList ] [ as IDENT ]`
    Import = IMPORT;
    /// `IDENT { / IDENT }`
    ModPath = MOD_PATH;
    /// `. { Name, … }` — names are `IDENT` or `TYPE_NAME` tokens.
    ImportList = IMPORT_LIST;
    /// `{ Attribute } [pub] type TYPE_NAME [ TypeParamList ] = ( RecordType | VariantList | Type ) [ WhereClause ]`
    TypeDecl = TYPE_DECL;
    /// `< TypeParam, … >`
    TypeParamList = TYPE_PARAM_LIST;
    /// `TYPE_NAME [ : TYPE_NAME { + TYPE_NAME } ]` or a row variable `IDENT`.
    TypeParam = TYPE_PARAM;
    /// `{ FieldDecl, … }`
    RecordType = RECORD_TYPE;
    /// `IDENT : Type [ WhereClause ]`
    FieldDecl = FIELD_DECL;
    /// `Variant { | Variant }` — only when the body has `|` or a payload (design §3.2 item 5).
    VariantList = VARIANT_LIST;
    /// `Path [ TypeList ]`
    Variant = VARIANT;
    /// `where Expr`
    WhereClause = WHERE_CLAUSE;
    /// `{ Attribute } [pub] error TYPE_NAME [ FieldList ]`
    ErrorDecl = ERROR_DECL;
    /// `( FieldDecl, … )`
    FieldList = FIELD_LIST;
    /// `{ Attribute } [pub] effect TYPE_NAME [ TypeParamList ] { OpSig … }`
    EffectDecl = EFFECT_DECL;
    /// `fn IDENT ParamList [ RetType ] [ ! EffectRow ] { Contract }`
    OpSig = OP_SIG;
    /// `{ Attribute } [pub] fn IDENT [ TypeParamList ] ParamList [ RetType ] [ ! EffectRow ]
    /// { Contract } [ Block ]` — the body is absent for `@llm`/`@external` declarations.
    FnDecl = FN_DECL;
    /// `( Param, … )`
    ParamList = PARAM_LIST;
    /// `( IDENT | _ ) [ : Type ]` — the type is optional only in lambdas and handler clauses.
    Param = PARAM;
    /// `-> Type`
    RetType = RET_TYPE;
    /// `{ NamedType, … [ | IDENT ] }` or a bare row variable `IDENT`.
    EffectRow = EFFECT_ROW;
    /// `( requires | ensures ) [ IDENT : ] Expr`
    Contract = CONTRACT;
    /// `{ Attribute } [pub] let IDENT [ : Type ] = Expr`
    ConstDecl = CONST_DECL;
    /// `{ Attribute } test StringLit Block`
    TestDecl = TEST_DECL;
    /// `{ Attribute } property StringLit ParamList { Contract } Block`
    PropertyDecl = PROPERTY_DECL;

    /// `( IDENT | TYPE_NAME ) { . TYPE_NAME }` — e.g. `x`, `Some`, `http.Request`, `Retry.Policy`.
    Path = PATH;
    /// `Path [ GenericArgList ]`
    NamedType = NAMED_TYPE;
    /// `< Type, … >`
    GenericArgList = GENERIC_ARG_LIST;
    /// `fn TypeList [ RetType ] [ ! EffectRow ]`
    FnType = FN_TYPE;
    /// `( Type, … )`
    TypeList = TYPE_LIST;
    /// `( Type, … )` — `()` is `Unit`; `(T)` is a one-element list the checker treats as `T`.
    TupleType = TUPLE_TYPE;
    /// `NamedType | NamedType { | NamedType }`
    UnionType = UNION_TYPE;

    /// `{ Stmt … }`
    Block = BLOCK;
    /// `let Pat [ : Type ] = Expr`
    LetStmt = LET_STMT;
    /// `var IDENT [ : Type ] = Expr` (also handler state)
    VarStmt = VAR_STMT;
    /// `Expr = Expr` — the target is a `PathExpr`/`FieldExpr` chain of identifiers.
    AssignStmt = ASSIGN_STMT;
    /// `for Pat in Expr Block`
    ForStmt = FOR_STMT;
    /// `while Expr Block`
    WhileStmt = WHILE_STMT;
    /// `return [ Expr ]`
    ReturnStmt = RETURN_STMT;
    /// `break`
    BreakStmt = BREAK_STMT;
    /// `continue`
    ContinueStmt = CONTINUE_STMT;
    /// `Expr`
    ExprStmt = EXPR_STMT;

    /// One of `INT`, `FLOAT`, `true`, `false`.
    Literal = LITERAL;
    /// `STRING_START { STRING_TEXT | Interpolation } STRING_END`
    StringLit = STRING_LIT;
    /// `INTERP_START Expr INTERP_END`
    Interpolation = INTERPOLATION;
    /// `IDENT StringLit` with no space in between (`sql"…"`, design §4.11).
    TaggedString = TAGGED_STRING;
    /// `Path`
    PathExpr = PATH_EXPR;
    /// `HOLE`
    HoleExpr = HOLE_EXPR;
    /// `Path { [ RecordSpread , ] FieldInit, … }`
    RecordExpr = RECORD_EXPR;
    /// `.. Expr`
    RecordSpread = RECORD_SPREAD;
    /// `IDENT [ : Expr ]`
    FieldInit = FIELD_INIT;
    /// `( Expr )`
    ParenExpr = PAREN_EXPR;
    /// `()` or `( Expr , [ Expr, … ] )`
    TupleExpr = TUPLE_EXPR;
    /// `[ Expr, … ]`
    ListExpr = LIST_EXPR;
    /// `if Expr Block [ else ( IfExpr | Block ) ]`
    IfExpr = IF_EXPR;
    /// `match Expr MatchArmList`
    MatchExpr = MATCH_EXPR;
    /// `{ MatchArm … }` — arms end with `,` or `NEWLINE`.
    MatchArmList = MATCH_ARM_LIST;
    /// `Pat [ MatchGuard ] => Expr`
    MatchArm = MATCH_ARM;
    /// `if Expr`
    MatchGuard = MATCH_GUARD;
    /// `with Expr { , Expr } Block`
    WithExpr = WITH_EXPR;
    /// `try Block [ catch MatchArmList ]`
    TryExpr = TRY_EXPR;
    /// `fn ParamList [ RetType ] Block`
    LambdaExpr = LAMBDA_EXPR;
    /// `handler Path { ( VarStmt | HandlerClause ) … }`
    HandlerExpr = HANDLER_EXPR;
    /// `fn IDENT ParamList Block`
    HandlerClause = HANDLER_CLAUSE;
    /// `fail Expr`
    FailExpr = FAIL_EXPR;
    /// `Expr op Expr`
    BinExpr = BIN_EXPR;
    /// `( - | ! ) Expr`
    PrefixExpr = PREFIX_EXPR;
    /// `Expr [ GenericArgList ] ArgList`
    CallExpr = CALL_EXPR;
    /// `( Arg, … )` where an argument is an `Expr` or a `NamedArg`.
    ArgList = ARG_LIST;
    /// `IDENT : Expr`
    NamedArg = NAMED_ARG;
    /// `Expr . ( IDENT | TYPE_NAME | INT )`
    FieldExpr = FIELD_EXPR;
    /// `Expr [ Expr ]`
    IndexExpr = INDEX_EXPR;
    /// `Expr ?`
    RaiseExpr = RAISE_EXPR;
    /// `@ IDENT [ ArgList ]` in expression position, e.g. `.trust(@trust("reason"))`.
    AttrExpr = ATTR_EXPR;

    /// `_`
    WildcardPat = WILDCARD_PAT;
    /// `IDENT`
    BindPat = BIND_PAT;
    /// `[ - ] Literal` or `StringLit`
    LiteralPat = LITERAL_PAT;
    /// `Path` naming a nullary variant or error tag.
    PathPat = PATH_PAT;
    /// `Path ( Pat, … )`
    VariantPat = VARIANT_PAT;
    /// `Path { FieldPat, … [ , .. ] }`
    RecordPat = RECORD_PAT;
    /// `IDENT [ : Pat ]`
    FieldPat = FIELD_PAT;
    /// `( Pat, … )`
    TuplePat = TUPLE_PAT;
    /// `[ Pat, … [ RestPat ] ]`
    ListPat = LIST_PAT;
    /// `.. [ IDENT ]`
    RestPat = REST_PAT;
    /// `Pat | Pat { | Pat }`
    OrPat = OR_PAT;
}

enums! {
    Item { Import, TypeDecl, ErrorDecl, EffectDecl, FnDecl, ConstDecl, TestDecl, PropertyDecl }
    Type { NamedType, FnType, TupleType, UnionType }
    Stmt {
        LetStmt, VarStmt, AssignStmt, ForStmt, WhileStmt, ReturnStmt, BreakStmt, ContinueStmt,
        ExprStmt,
    }
    Expr {
        Literal, StringLit, TaggedString, PathExpr, HoleExpr, RecordExpr, ParenExpr, TupleExpr,
        ListExpr, Block, IfExpr, MatchExpr, WithExpr, TryExpr, LambdaExpr, HandlerExpr, FailExpr,
        BinExpr, PrefixExpr, CallExpr, FieldExpr, IndexExpr, RaiseExpr, AttrExpr,
    }
    Pat {
        WildcardPat, BindPat, LiteralPat, PathPat, VariantPat, RecordPat, TuplePat, ListPat, OrPat,
    }
    /// A call argument.
    Arg { NamedArg, Expr }
    /// The `else` part of an `if`.
    ElseBranch { IfExpr, Block }
    /// An entry of a handler expression body.
    HandlerMember { VarStmt, HandlerClause }
}

/// Attributes, visibility and doc comments shared by every item.
pub trait HasAttrs: AstNode {
    fn attributes(&self) -> AstChildren<Attribute> {
        children(self.syntax())
    }
    fn pub_token(&self) -> Option<SyntaxToken> {
        token(self.syntax(), PUB_KW)
    }
    fn is_pub(&self) -> bool {
        self.pub_token().is_some()
    }
    /// `///` comments directly above the item, in source order.
    fn doc_comments(&self) -> Vec<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .filter(|t| t.kind() == DOC_COMMENT)
            .collect()
    }
}

impl HasAttrs for Item {}
impl HasAttrs for Import {}
impl HasAttrs for TypeDecl {}
impl HasAttrs for ErrorDecl {}
impl HasAttrs for EffectDecl {}
impl HasAttrs for FnDecl {}
impl HasAttrs for ConstDecl {}
impl HasAttrs for TestDecl {}
impl HasAttrs for PropertyDecl {}

impl SourceFile {
    pub fn items(&self) -> AstChildren<Item> {
        children(&self.0)
    }
}

impl Attribute {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn args(&self) -> Option<ArgList> {
        child(&self.0)
    }
}

impl AttrExpr {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn args(&self) -> Option<ArgList> {
        child(&self.0)
    }
}

impl Import {
    pub fn path(&self) -> Option<ModPath> {
        child(&self.0)
    }
    pub fn names(&self) -> Option<ImportList> {
        child(&self.0)
    }
    /// The `IDENT` after `as`.
    pub fn alias(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .skip_while(|e| e.kind() != AS_KW)
            .filter_map(NodeOrToken::into_token)
            .find(|t| t.kind() == IDENT)
    }
}

impl ModPath {
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens_of(&self.0, &[IDENT])
    }
}

impl ImportList {
    pub fn names(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens_of(&self.0, &[IDENT, TYPE_NAME])
    }
}

fn tokens_of(
    node: &SyntaxNode,
    kinds: &'static [SyntaxKind],
) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(move |t| kinds.contains(&t.kind()))
}

impl TypeDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, TYPE_NAME)
    }
    pub fn type_params(&self) -> Option<TypeParamList> {
        child(&self.0)
    }
    pub fn record(&self) -> Option<RecordType> {
        child(&self.0)
    }
    pub fn variants(&self) -> Option<VariantList> {
        child(&self.0)
    }
    /// The aliased (or refined) type when the body is neither a record nor a variant list.
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
    pub fn where_clause(&self) -> Option<WhereClause> {
        child(&self.0)
    }
}

impl TypeParamList {
    pub fn params(&self) -> AstChildren<TypeParam> {
        children(&self.0)
    }
}

impl TypeParam {
    /// `TYPE_NAME` for a type parameter, `IDENT` for a row variable.
    pub fn name(&self) -> Option<SyntaxToken> {
        token_in(&self.0, &[TYPE_NAME, IDENT])
    }
    pub fn is_row_var(&self) -> bool {
        self.name().is_some_and(|t| t.kind() == IDENT)
    }
    /// Constraint names after `:`.
    pub fn bounds(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        self.0
            .children_with_tokens()
            .skip_while(|e| e.kind() != COLON)
            .filter_map(NodeOrToken::into_token)
            .filter(|t| t.kind() == TYPE_NAME)
    }
}

impl RecordType {
    pub fn fields(&self) -> AstChildren<FieldDecl> {
        children(&self.0)
    }
}

impl FieldDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
    pub fn where_clause(&self) -> Option<WhereClause> {
        child(&self.0)
    }
}

impl VariantList {
    pub fn variants(&self) -> AstChildren<Variant> {
        children(&self.0)
    }
}

impl Variant {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn fields(&self) -> Option<TypeList> {
        child(&self.0)
    }
}

impl WhereClause {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ErrorDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, TYPE_NAME)
    }
    pub fn fields(&self) -> Option<FieldList> {
        child(&self.0)
    }
}

impl FieldList {
    pub fn fields(&self) -> AstChildren<FieldDecl> {
        children(&self.0)
    }
}

impl EffectDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, TYPE_NAME)
    }
    pub fn type_params(&self) -> Option<TypeParamList> {
        child(&self.0)
    }
    pub fn ops(&self) -> AstChildren<OpSig> {
        children(&self.0)
    }
}

macro_rules! signature_accessors {
    ($($ty:ident),*) => {$(
        impl $ty {
            pub fn name(&self) -> Option<SyntaxToken> {
                token(&self.0, IDENT)
            }
            pub fn params(&self) -> Option<ParamList> {
                child(&self.0)
            }
            pub fn ret(&self) -> Option<RetType> {
                child(&self.0)
            }
            /// The effect row after `!`.
            pub fn row(&self) -> Option<EffectRow> {
                child(&self.0)
            }
            pub fn contracts(&self) -> AstChildren<Contract> {
                children(&self.0)
            }
        }
    )*};
}
signature_accessors!(FnDecl, OpSig);

impl FnDecl {
    pub fn type_params(&self) -> Option<TypeParamList> {
        child(&self.0)
    }
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
}

impl ParamList {
    pub fn params(&self) -> AstChildren<Param> {
        children(&self.0)
    }
}

impl Param {
    /// `IDENT` or `UNDERSCORE`.
    pub fn name(&self) -> Option<SyntaxToken> {
        token_in(&self.0, &[IDENT, UNDERSCORE])
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl RetType {
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
}

impl EffectRow {
    /// Effect labels, e.g. `Console`, `Fail<E>`.
    pub fn effects(&self) -> AstChildren<NamedType> {
        children(&self.0)
    }
    /// The row variable: the tail after `|`, or the whole row when written as a bare `IDENT`.
    pub fn tail(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    /// `true` for `e` (no braces); `false` for `{…}`.
    pub fn is_bare_var(&self) -> bool {
        token(&self.0, L_BRACE).is_none()
    }
}

impl Contract {
    /// `REQUIRES_KW` or `ENSURES_KW`.
    pub fn keyword(&self) -> Option<SyntaxToken> {
        token_in(&self.0, &[REQUIRES_KW, ENSURES_KW])
    }
    pub fn is_requires(&self) -> bool {
        token(&self.0, REQUIRES_KW).is_some()
    }
    /// The clause name in `requires name: expr`.
    pub fn label(&self) -> Option<SyntaxToken> {
        token(&self.0, COLON)?;
        token(&self.0, IDENT)
    }
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ConstDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
    pub fn value(&self) -> Option<Expr> {
        child_after(&self.0, EQ)
    }
}

impl TestDecl {
    pub fn name(&self) -> Option<StringLit> {
        child(&self.0)
    }
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
}

impl PropertyDecl {
    pub fn name(&self) -> Option<StringLit> {
        child(&self.0)
    }
    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }
    pub fn contracts(&self) -> AstChildren<Contract> {
        children(&self.0)
    }
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
}

// ---- Types ----

impl Path {
    /// `IDENT`/`TYPE_NAME` segments in order.
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken> + use<> {
        tokens_of(&self.0, &[IDENT, TYPE_NAME])
    }
    /// The last segment, which names the referenced item.
    pub fn name(&self) -> Option<SyntaxToken> {
        self.segments().last()
    }
}

impl NamedType {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn generic_args(&self) -> Option<GenericArgList> {
        child(&self.0)
    }
}

impl GenericArgList {
    pub fn types(&self) -> AstChildren<Type> {
        children(&self.0)
    }
}

impl FnType {
    pub fn params(&self) -> Option<TypeList> {
        child(&self.0)
    }
    pub fn ret(&self) -> Option<RetType> {
        child(&self.0)
    }
    pub fn row(&self) -> Option<EffectRow> {
        child(&self.0)
    }
}

impl TypeList {
    pub fn types(&self) -> AstChildren<Type> {
        children(&self.0)
    }
}

impl TupleType {
    pub fn types(&self) -> AstChildren<Type> {
        children(&self.0)
    }
}

impl UnionType {
    pub fn types(&self) -> AstChildren<Type> {
        children(&self.0)
    }
}

// ---- Statements ----

impl Block {
    pub fn stmts(&self) -> AstChildren<Stmt> {
        children(&self.0)
    }
    /// The final statement's expression, which is the block's value (design §3.2 item 9).
    pub fn tail_expr(&self) -> Option<Expr> {
        match self.stmts().last()? {
            Stmt::ExprStmt(s) => s.expr(),
            _ => None,
        }
    }
}

impl LetStmt {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
    pub fn value(&self) -> Option<Expr> {
        child_after(&self.0, EQ)
    }
}

impl VarStmt {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn ty(&self) -> Option<Type> {
        child(&self.0)
    }
    pub fn value(&self) -> Option<Expr> {
        child_after(&self.0, EQ)
    }
}

impl AssignStmt {
    pub fn target(&self) -> Option<Expr> {
        self.0
            .children_with_tokens()
            .take_while(|e| e.kind() != EQ)
            .filter_map(NodeOrToken::into_node)
            .find_map(Expr::cast)
    }
    pub fn value(&self) -> Option<Expr> {
        child_after(&self.0, EQ)
    }
}

impl ForStmt {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
    pub fn iterable(&self) -> Option<Expr> {
        let body = self.body();
        child_after::<Expr>(&self.0, IN_KW)
            .filter(|e| Some(e.syntax()) != body.as_ref().map(|b| b.syntax()))
    }
    pub fn body(&self) -> Option<Block> {
        last_block(&self.0)
    }
}

impl WhileStmt {
    pub fn cond(&self) -> Option<Expr> {
        let body = self.body();
        child::<Expr>(&self.0).filter(|e| Some(e.syntax()) != body.as_ref().map(|b| b.syntax()))
    }
    pub fn body(&self) -> Option<Block> {
        last_block(&self.0)
    }
}

fn last_block(node: &SyntaxNode) -> Option<Block> {
    node.children().filter_map(Block::cast).last()
}

impl ReturnStmt {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

// ---- Expressions ----

/// The value-carrying token of a [`Literal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    Int,
    Float,
    Bool(bool),
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        token_in(&self.0, &[INT, FLOAT, TRUE_KW, FALSE_KW])
    }
    pub fn kind(&self) -> Option<LiteralKind> {
        Some(match self.token()?.kind() {
            INT => LiteralKind::Int,
            FLOAT => LiteralKind::Float,
            TRUE_KW => LiteralKind::Bool(true),
            _ => LiteralKind::Bool(false),
        })
    }
    /// The value of an `INT` literal (underscores removed, hex/binary decoded). `None` if the
    /// literal is not an `Int` or is out of range (already reported as `V0003`).
    pub fn int_value(&self) -> Option<i64> {
        let t = self.token().filter(|t| t.kind() == INT)?;
        crate::lexer::int_value(t.text())
    }
}

/// A piece of a string literal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StringPart {
    /// Raw literal text with escapes still encoded; decode with [`crate::unescape`].
    Text(SyntaxToken),
    Interp(Interpolation),
}

impl StringLit {
    pub fn parts(&self) -> Vec<StringPart> {
        self.0
            .children_with_tokens()
            .filter_map(|e| match e {
                NodeOrToken::Token(t) if t.kind() == STRING_TEXT => Some(StringPart::Text(t)),
                NodeOrToken::Node(n) => Interpolation::cast(n).map(StringPart::Interp),
                _ => None,
            })
            .collect()
    }
    pub fn is_terminated(&self) -> bool {
        token(&self.0, STRING_END).is_some()
    }
}

impl Interpolation {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TaggedString {
    pub fn tag(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn string(&self) -> Option<StringLit> {
        child(&self.0)
    }
}

impl PathExpr {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

impl HoleExpr {
    pub fn token(&self) -> Option<SyntaxToken> {
        token(&self.0, HOLE)
    }
    /// The hole's name without the leading `?`.
    pub fn name(&self) -> Option<String> {
        self.token().map(|t| t.text()[1..].to_string())
    }
}

impl RecordExpr {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn spread(&self) -> Option<RecordSpread> {
        child(&self.0)
    }
    pub fn fields(&self) -> AstChildren<FieldInit> {
        children(&self.0)
    }
}

impl RecordSpread {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl FieldInit {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    /// `None` for shorthand `{ id }`.
    pub fn value(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ParenExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TupleExpr {
    pub fn exprs(&self) -> AstChildren<Expr> {
        children(&self.0)
    }
}

impl ListExpr {
    pub fn exprs(&self) -> AstChildren<Expr> {
        children(&self.0)
    }
}

impl IfExpr {
    pub fn cond(&self) -> Option<Expr> {
        let then = self.then_branch();
        child::<Expr>(&self.0).filter(|e| Some(e.syntax()) != then.as_ref().map(|b| b.syntax()))
    }
    pub fn then_branch(&self) -> Option<Block> {
        self.0
            .children_with_tokens()
            .take_while(|e| e.kind() != ELSE_KW)
            .filter_map(NodeOrToken::into_node)
            .filter_map(Block::cast)
            .last()
    }
    pub fn else_branch(&self) -> Option<ElseBranch> {
        child_after(&self.0, ELSE_KW)
    }
}

impl MatchExpr {
    pub fn scrutinee(&self) -> Option<Expr> {
        child(&self.0)
    }
    pub fn arm_list(&self) -> Option<MatchArmList> {
        child(&self.0)
    }
    pub fn arms(&self) -> impl Iterator<Item = MatchArm> + use<> {
        self.arm_list().into_iter().flat_map(|l| l.arms())
    }
}

impl MatchArmList {
    pub fn arms(&self) -> AstChildren<MatchArm> {
        children(&self.0)
    }
}

impl MatchArm {
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
    pub fn guard(&self) -> Option<MatchGuard> {
        child(&self.0)
    }
    pub fn expr(&self) -> Option<Expr> {
        child_after(&self.0, FAT_ARROW)
    }
}

impl MatchGuard {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl WithExpr {
    /// Handler expressions, outermost first.
    pub fn handlers(&self) -> impl Iterator<Item = Expr> + use<> {
        let body = self.body().map(|b| b.syntax().clone());
        children::<Expr>(&self.0).filter(move |e| Some(e.syntax()) != body.as_ref())
    }
    pub fn body(&self) -> Option<Block> {
        last_block(&self.0)
    }
}

impl TryExpr {
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
    pub fn catch_arms(&self) -> Option<MatchArmList> {
        child(&self.0)
    }
}

impl LambdaExpr {
    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }
    pub fn ret(&self) -> Option<RetType> {
        child(&self.0)
    }
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
}

impl HandlerExpr {
    /// The handled effect.
    pub fn effect(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn members(&self) -> AstChildren<HandlerMember> {
        children(&self.0)
    }
    pub fn state(&self) -> AstChildren<VarStmt> {
        children(&self.0)
    }
    pub fn clauses(&self) -> AstChildren<HandlerClause> {
        children(&self.0)
    }
}

impl HandlerClause {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn params(&self) -> Option<ParamList> {
        child(&self.0)
    }
    pub fn body(&self) -> Option<Block> {
        child(&self.0)
    }
}

impl FailExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

/// Binary operator tokens.
pub const BINARY_OPS: &[SyntaxKind] = &[
    PIPE2, AMP2, EQ2, NEQ, LT, LTEQ, GT, GTEQ, DOT2, PLUS, MINUS, STAR, SLASH, PERCENT,
];

impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        child(&self.0)
    }
    pub fn rhs(&self) -> Option<Expr> {
        children::<Expr>(&self.0).nth(1)
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        token_in(&self.0, BINARY_OPS)
    }
}

impl PrefixExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        token_in(&self.0, &[MINUS, BANG])
    }
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        child(&self.0)
    }
    /// Explicit type arguments `f<T>(…)` (see crate docs, deviation 2).
    pub fn generic_args(&self) -> Option<GenericArgList> {
        child(&self.0)
    }
    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }
    pub fn args(&self) -> impl Iterator<Item = Arg> + use<> {
        self.arg_list().into_iter().flat_map(|l| l.args())
    }
}

impl ArgList {
    pub fn args(&self) -> AstChildren<Arg> {
        children(&self.0)
    }
}

impl NamedArg {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    pub fn value(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl FieldExpr {
    pub fn receiver(&self) -> Option<Expr> {
        child(&self.0)
    }
    /// The field name (`IDENT`, or `TYPE_NAME` for a qualified name on a non-path receiver).
    pub fn name(&self) -> Option<SyntaxToken> {
        self.0
            .children_with_tokens()
            .skip_while(|e| e.kind() != DOT)
            .filter_map(NodeOrToken::into_token)
            .find(|t| matches!(t.kind(), IDENT | TYPE_NAME))
    }
    /// The tuple index in `e.0`.
    pub fn index(&self) -> Option<SyntaxToken> {
        token(&self.0, INT)
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        child(&self.0)
    }
    pub fn index(&self) -> Option<Expr> {
        children::<Expr>(&self.0).nth(1)
    }
}

impl RaiseExpr {
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

// ---- Patterns ----

impl BindPat {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
}

impl LiteralPat {
    pub fn is_negative(&self) -> bool {
        token(&self.0, MINUS).is_some()
    }
    pub fn literal(&self) -> Option<Literal> {
        child(&self.0)
    }
    pub fn string(&self) -> Option<StringLit> {
        child(&self.0)
    }
}

impl PathPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
}

impl VariantPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn pats(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}

impl RecordPat {
    pub fn path(&self) -> Option<Path> {
        child(&self.0)
    }
    pub fn fields(&self) -> AstChildren<FieldPat> {
        children(&self.0)
    }
    /// `true` when the pattern ends with `..`.
    pub fn has_rest(&self) -> bool {
        token(&self.0, DOT2).is_some()
    }
}

impl FieldPat {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
    /// `None` for shorthand `{ id }`.
    pub fn pat(&self) -> Option<Pat> {
        child(&self.0)
    }
}

impl TuplePat {
    pub fn pats(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}

impl ListPat {
    pub fn pats(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
    pub fn rest(&self) -> Option<RestPat> {
        child(&self.0)
    }
}

impl RestPat {
    pub fn name(&self) -> Option<SyntaxToken> {
        token(&self.0, IDENT)
    }
}

impl OrPat {
    pub fn pats(&self) -> AstChildren<Pat> {
        children(&self.0)
    }
}
