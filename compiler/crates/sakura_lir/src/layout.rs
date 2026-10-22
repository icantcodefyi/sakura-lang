//! The layout table (ABI §3.4), frame descriptors (§6.1) and closure descriptors.
//!
//! Every object kind whose payload is a sequence of 8-byte slots (records, variants, tuples) has
//! an entry; `type_id` is the entry's index. Declared types come first, sorted by qualified name
//! (ABI §3.2), then the two tuple shapes runtime builtins build (`(word, word)` and
//! `(i64, word)`), then every other tuple or handler state shape (§17.108 E6) in first-use
//! order: deterministic, so two
//! compilations of the same program agree byte for byte. `Show`, `Eq`, `Ord`, `Hash`,
//! `Dup`/`Drop` and the LIR validator read this table and nothing else.
//!
//! ABI 1.0.1 (the endorsed errata folded in) as `sakura_rt` implements it:
//! a field of known `Int`, `Float` or `Bool` type is a scalar slot and a `Unit` field holds the
//! `Unit` immediate in a word slot (E5); a tuple has one row per *shape* (E2); the byte format is
//! E3's ([`LayoutTable::to_bytes`]) and frames and closures are described by E4's code
//! descriptor table ([`code_table`]).

use crate::lir::{Func, LayoutId, Module, Repr, Term, Var};
use crate::snapshot as core;

/// The 2-bit `scalar_kinds` code of a slot (ABI §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotKind {
    /// 0: a pointer or generic word.
    Word = 0,
    I64 = 1,
    F64 = 2,
    /// 3: `Bool` (and `Unit` in frames and closures, where nothing renders it).
    Bool = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    pub kind: SlotKind,
    /// The slot's pointer-bitmap bit: it may own a pointer and is visited by `Drop`.
    pub owns: bool,
}

impl Slot {
    pub const WORD: Slot = Slot {
        kind: SlotKind::Word,
        owns: true,
    };
    /// A `Unit` field: the `Unit` immediate in a word slot that owns nothing (errata E5).
    pub const UNIT: Slot = Slot {
        kind: SlotKind::Word,
        owns: false,
    };

    fn scalar(kind: SlotKind) -> Slot {
        Slot { kind, owns: false }
    }

    /// The slot holding a field of declared Core type `ty`.
    pub fn of_ty(ty: &core::Ty) -> Slot {
        match ty {
            core::Ty::Int => Slot::scalar(SlotKind::I64),
            core::Ty::Float => Slot::scalar(SlotKind::F64),
            core::Ty::Bool => Slot::scalar(SlotKind::Bool),
            core::Ty::Unit | core::Ty::Never => Slot::UNIT,
            _ => Slot::WORD,
        }
    }

    /// The object slot holding a value of representation `r` (a tuple element).
    pub fn field_of_repr(r: Repr) -> Slot {
        match r {
            // A borrowed word is never stored in an object (the validator); a frame slot holds
            // it unowned, as an id.
            Repr::I64 | Repr::Id | Repr::Ref => Slot::scalar(SlotKind::I64),
            Repr::F64 => Slot::scalar(SlotKind::F64),
            Repr::Bool => Slot::scalar(SlotKind::Bool),
            Repr::Unit => Slot::UNIT,
            Repr::Ptr | Repr::Word => Slot::WORD,
        }
    }

    /// The slot holding a value of representation `r` in a frame or closure, where a kind-0 slot
    /// always owns its word (errata E4).
    pub fn of_repr(r: Repr) -> Slot {
        match r {
            // A borrowed word's slot does not own it (plan M5 N4a): its owner is the caller's.
            Repr::I64 | Repr::Id | Repr::Ref => Slot::scalar(SlotKind::I64),
            Repr::F64 => Slot::scalar(SlotKind::F64),
            Repr::Bool | Repr::Unit => Slot::scalar(SlotKind::Bool),
            Repr::Ptr | Repr::Word => Slot::WORD,
        }
    }

    /// The variable representation a slot loads as and stores from.
    pub fn repr(self) -> Repr {
        match self.kind {
            SlotKind::Word => Repr::Word,
            SlotKind::I64 => Repr::I64,
            SlotKind::F64 => Repr::F64,
            SlotKind::Bool => Repr::Bool,
        }
    }
}

/// The entry kinds of errata E3, with their codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Record = 0,
    /// An `error` tag: a record that may appear in error unions; shows without `{}` when empty.
    Error = 1,
    Tuple = 2,
    Variant = 3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// `Show` name: the type name of a record, the constructor name of a variant row, empty for
    /// a tuple.
    pub name: String,
    pub slots: Vec<Slot>,
    /// Record field names in declaration order; empty otherwise.
    pub fields: Vec<String>,
}

impl Row {
    /// Object size: the 8-byte header, two `u32` words, then the slots at offset 16 (E6).
    pub fn size_bytes(&self) -> u32 {
        16 + 8 * self.slots.len() as u32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Qualified name (`std::option::Option`); empty for tuples.
    pub qualified: String,
    pub kind: EntryKind,
    /// `Some("Option")`/`Some("Result")` for the runtime-known variants.
    pub builtin: Option<String>,
    /// One row for a record or tuple, one per constructor for a variant.
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutTable {
    pub entries: Vec<Entry>,
    /// Core `TypeId` index → entry.
    pub of_type: Vec<LayoutId>,
}

/// The two tuple shapes runtime builtins construct (errata E3 header).
pub const PAIR: [Slot; 2] = [Slot::WORD, Slot::WORD];
pub const INT_PAIR: [Slot; 2] = [
    Slot {
        kind: SlotKind::I64,
        owns: false,
    },
    Slot::WORD,
];

impl LayoutTable {
    /// Entries for every Core type and the builtin tuple shapes; more tuple shapes are added by
    /// [`LayoutTable::tuple`] as lowering meets them.
    pub fn build(m: &core::Module) -> LayoutTable {
        let mut order: Vec<usize> = (0..m.types.len()).collect();
        order.sort_by_key(|&i| format!("{}::{}", m.types[i].module, m.types[i].name));
        let mut table = LayoutTable {
            of_type: vec![LayoutId(0); m.types.len()],
            ..LayoutTable::default()
        };
        for i in order {
            let t = &m.types[i];
            table.of_type[i] = LayoutId(table.entries.len() as u32);
            let (kind, rows) = match &t.kind {
                core::TypeKind::Record { fields, error } => (
                    if *error {
                        EntryKind::Error
                    } else {
                        EntryKind::Record
                    },
                    vec![Row {
                        name: t.name.clone(),
                        slots: fields.iter().map(|f| Slot::of_ty(&f.ty)).collect(),
                        fields: fields.iter().map(|f| f.name.clone()).collect(),
                    }],
                ),
                core::TypeKind::Variant { ctors } => (
                    EntryKind::Variant,
                    ctors
                        .iter()
                        .map(|c| Row {
                            name: c.name.clone(),
                            slots: c.fields.iter().map(Slot::of_ty).collect(),
                            fields: vec![],
                        })
                        .collect(),
                ),
            };
            table.entries.push(Entry {
                qualified: format!("{}::{}", t.module, t.name),
                kind,
                builtin: t.builtin.clone(),
                rows,
            });
        }
        table.tuple(&PAIR);
        table.tuple(&INT_PAIR);
        table
    }

    pub fn entry(&self, id: LayoutId) -> &Entry {
        &self.entries[id.index()]
    }

    /// The tuple row with these slots, added on first use.
    pub fn tuple(&mut self, slots: &[Slot]) -> LayoutId {
        if let Some(id) = self.find_tuple(slots) {
            return id;
        }
        self.entries.push(Entry {
            qualified: String::new(),
            kind: EntryKind::Tuple,
            builtin: None,
            rows: vec![Row {
                name: String::new(),
                slots: slots.to_vec(),
                fields: vec![],
            }],
        });
        LayoutId(self.entries.len() as u32 - 1)
    }

    /// The row describing a handler's state (ABI 1.0.1 §3.2 tag 14, §3.4 "one entry per handler
    /// state shape"): a shape row like a tuple's, shared with tuples of the same shape.
    pub fn handler_state(&mut self, slots: &[Slot]) -> LayoutId {
        self.tuple(slots)
    }

    pub fn find_tuple(&self, slots: &[Slot]) -> Option<LayoutId> {
        self.entries
            .iter()
            .position(|e| e.kind == EntryKind::Tuple && e.rows[0].slots == slots)
            .map(|i| LayoutId(i as u32))
    }

    /// The entry of the runtime-known variant `name` (`"Option"`).
    pub fn builtin(&self, name: &str) -> Option<LayoutId> {
        self.entries
            .iter()
            .position(|e| e.builtin.as_deref() == Some(name))
            .map(|i| LayoutId(i as u32))
    }

    /// The data-segment bytes, little-endian, in `sakura_rt`'s reading of ABI §3.4 (errata E3,
    /// `compiler/runtime/rt/src/layout.rs`):
    ///
    /// ```text
    /// 0   u32 n_types, u32 option_type_id, u32 result_type_id, u32 pair_type_id,
    ///     u32 int_pair_type_id (0xFFFF_FFFF when absent)
    /// 20  n_types × u32 entry offset
    /// entry: u32 kind (0 record, 1 error, 2 tuple or handler state shape, 3 variant), u32 name,
    ///        u32 n_rows, n_rows × u32 row offset
    /// row:   u32 size_bytes, u32 n_slots, max(1, ⌈n/32⌉) × u32 pointer_bitmap,
    ///        u32 scalar_kinds, u32 name, u32 field_names
    /// kinds: ⌈n/16⌉ u32 words, slot i at bits 2(i mod 16) of word i/16
    /// field names: n_slots × u32 name offset
    /// name:  u32 len, UTF-8 bytes, zero padding to 4
    /// ```
    ///
    /// Offsets are bytes from the start of the table; 0 is "absent". Entries come first, then the
    /// rows, kinds and field-name arrays in entry order, then the names.
    pub fn to_bytes(&self) -> Vec<u8> {
        let id = |l: Option<LayoutId>| l.map_or(u32::MAX, |l| l.0);
        let mut w = Writer::default();
        let header = [
            self.entries.len() as u32,
            id(self.builtin("Option")),
            id(self.builtin("Result")),
            id(self.find_tuple(&PAIR)),
            id(self.find_tuple(&INT_PAIR)),
        ];
        let dir = w.reserve(header.len() + self.entries.len());
        w.set(dir, &header);
        let entries: Vec<usize> = self
            .entries
            .iter()
            .map(|e| w.reserve(3 + e.rows.len()))
            .collect();
        for (i, (e, &at)) in self.entries.iter().zip(&entries).enumerate() {
            w.set(dir + header.len() + i, &[w.offset(at)]);
            let name = if e.qualified.is_empty() {
                None
            } else {
                Some(w.name(&e.qualified))
            };
            let kind = e.kind as u32;
            w.set(at, &[kind, 0, e.rows.len() as u32]);
            if let Some(n) = name {
                w.fix_name(at + 1, n);
            }
            for (k, r) in e.rows.iter().enumerate() {
                let n = r.slots.len();
                let words = n.div_ceil(32).max(1);
                let row = w.reserve(2 + words + 3);
                w.set(at + 3 + k, &[w.offset(row)]);
                let mut bitmap = vec![0u32; words];
                for (i, s) in r.slots.iter().enumerate() {
                    if s.owns {
                        bitmap[i / 32] |= 1 << (i % 32);
                    }
                }
                w.set(row, &[r.size_bytes(), n as u32]);
                w.set(row + 2, &bitmap);
                if n > 0 {
                    let mut kinds = vec![0u32; n.div_ceil(16)];
                    for (i, s) in r.slots.iter().enumerate() {
                        kinds[i / 16] |= (s.kind as u32) << (2 * (i % 16));
                    }
                    let k_at = w.reserve(kinds.len());
                    w.set(k_at, &kinds);
                    w.set(row + 2 + words, &[w.offset(k_at)]);
                }
                if !r.name.is_empty() {
                    let nm = w.name(&r.name);
                    w.fix_name(row + 3 + words, nm);
                }
                if !r.fields.is_empty() {
                    let f_at = w.reserve(r.fields.len());
                    w.set(row + 4 + words, &[w.offset(f_at)]);
                    for (i, f) in r.fields.iter().enumerate() {
                        let nm = w.name(f);
                        w.fix_name(f_at + i, nm);
                    }
                }
            }
        }
        w.finish()
    }
}

/// Builds a table of `u32` words followed by a pool of names; name offsets are fixed up when
/// the word area's size is known.
#[derive(Default)]
struct Writer {
    words: Vec<u32>,
    pool: Vec<u8>,
    names: Vec<(String, u32)>,
    /// (word index, pool offset) to rewrite as table offsets.
    fixups: Vec<(usize, u32)>,
}

impl Writer {
    fn reserve(&mut self, n: usize) -> usize {
        let at = self.words.len();
        self.words.resize(at + n, 0);
        at
    }

    fn set(&mut self, at: usize, xs: &[u32]) {
        self.words[at..at + xs.len()].copy_from_slice(xs);
    }

    fn offset(&self, word: usize) -> u32 {
        (word * 4) as u32
    }

    /// The pool offset of `s`, added once.
    fn name(&mut self, s: &str) -> u32 {
        if let Some((_, o)) = self.names.iter().find(|(t, _)| t == s) {
            return *o;
        }
        let o = self.pool.len() as u32;
        self.pool.extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.pool.extend_from_slice(s.as_bytes());
        while !self.pool.len().is_multiple_of(4) {
            self.pool.push(0);
        }
        self.names.push((s.to_owned(), o));
        o
    }

    fn fix_name(&mut self, at: usize, pool_offset: u32) {
        self.fixups.push((at, pool_offset));
    }

    fn finish(mut self) -> Vec<u8> {
        let base = (self.words.len() * 4) as u32;
        for (at, o) in std::mem::take(&mut self.fixups) {
            self.words[at] = base + o;
        }
        let mut out: Vec<u8> = self.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        out.extend(self.pool);
        out
    }
}

/// Which variables of a function live in its continuation frame (ABI §6.1): the parameters,
/// then every parameter of a block the trampoline resumes (a call or perform continuation, or a
/// catch target), in block order. Other values are registers within one block.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameDesc {
    pub slots: Vec<(Var, Slot)>,
}

impl FrameDesc {
    pub fn of(f: &Func) -> FrameDesc {
        let mut resumed = vec![false; f.blocks.len()];
        if let Some(r) = resumed.first_mut() {
            *r = true;
        }
        for b in &f.blocks {
            if let Term::Call { cont, .. } | Term::Perform { cont, .. } = &b.term
                && let Some(r) = resumed.get_mut(cont.block.index())
            {
                *r = true;
            }
            for i in &b.insts {
                if let crate::lir::Inst::TryPush { catch, .. } = i
                    && let Some(r) = resumed.get_mut(catch.index())
                {
                    *r = true;
                }
            }
        }
        let mut slots = Vec::new();
        for (block, _) in f.blocks.iter().zip(resumed).filter(|(_, r)| *r) {
            for &p in &block.params {
                slots.push((p, Slot::of_repr(f.repr(p))));
            }
        }
        FrameDesc { slots }
    }

    /// Frame object size: header, the six `u32` fields of ABI §6.1, then the slots.
    pub fn size_bytes(&self) -> u32 {
        32 + 8 * self.slots.len() as u32
    }

    pub fn slot_of(&self, v: Var) -> Option<usize> {
        self.slots.iter().position(|(w, _)| *w == v)
    }
}

/// The capture slots of a closure of `f`: what `Drop` of the closure visits.
pub fn closure_slots(f: &Func) -> Vec<Slot> {
    f.captures.iter().map(|&r| Slot::of_repr(r)).collect()
}

/// The code descriptor table of errata E4, one entry per `code` (function table index):
///
/// ```text
/// 0   u32 n_codes, n_codes × u32 entry offset
/// entry: u32 name, u32 n_frame_slots, u32 frame_kinds, u32 n_captures, u32 capture_kinds
/// ```
///
/// with kinds and names as in [`LayoutTable::to_bytes`] (0 = absent: no kinds words when there
/// are no slots). A kind-0 frame or capture slot owns its word.
pub fn code_table(m: &Module) -> Vec<u8> {
    code_table_with(m, &[])
}

/// A code the runtime provides rather than the LIR (the callback stub, host clauses): its
/// name and the kinds of its frame slots and captures, as a [`code_table`] entry.
pub type NativeCode<'a> = (&'a str, &'a [Slot], &'a [Slot]);

/// [`code_table`] with `natives` as codes `n..` after the module's `n` functions (the WASM
/// backend's funcref table puts the runtime's codes there).
pub fn code_table_with(m: &Module, natives: &[NativeCode<'_>]) -> Vec<u8> {
    let mut codes: Vec<(&str, Vec<Slot>, Vec<Slot>)> = m
        .funcs
        .iter()
        .map(|f| {
            let frame = FrameDesc::of(f).slots.into_iter().map(|(_, s)| s).collect();
            (f.name.as_str(), frame, closure_slots(f))
        })
        .collect();
    codes.extend(natives.iter().map(|&(n, f, c)| (n, f.to_vec(), c.to_vec())));
    let mut w = Writer::default();
    let dir = w.reserve(1 + codes.len());
    w.set(dir, &[codes.len() as u32]);
    let entries: Vec<usize> = codes.iter().map(|_| w.reserve(5)).collect();
    for (i, ((name, frame, caps), &at)) in codes.iter().zip(&entries).enumerate() {
        w.set(dir + 1 + i, &[w.offset(at)]);
        let nm = w.name(name);
        w.fix_name(at, nm);
        w.set(at + 1, &[frame.len() as u32]);
        w.set(at + 3, &[caps.len() as u32]);
        for (field, slots) in [(at + 2, frame), (at + 4, caps)] {
            if slots.is_empty() {
                continue;
            }
            let mut kinds = vec![0u32; slots.len().div_ceil(16)];
            for (k, s) in slots.iter().enumerate() {
                kinds[k / 16] |= (s.kind as u32) << (2 * (k % 16));
            }
            let k_at = w.reserve(kinds.len());
            w.set(k_at, &kinds);
            w.set(field, &[w.offset(k_at)]);
        }
    }
    w.finish()
}

/// The effect table (ABI 1.0.1 open question P3, `sakura_rt`'s reading): `u32 n`, then `n` name
/// offsets of the effects' qualified names (as JS effect ids), names as in
/// [`LayoutTable::to_bytes`]; `effect_id` indexes it.
pub fn effect_table(m: &Module) -> Vec<u8> {
    let mut w = Writer::default();
    let dir = w.reserve(1 + m.effects.len());
    w.set(dir, &[m.effects.len() as u32]);
    for (i, e) in m.effects.iter().enumerate() {
        let nm = w.name(&e.name);
        w.fix_name(dir + 1 + i, nm);
    }
    w.finish()
}
