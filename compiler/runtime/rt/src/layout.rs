//! The layout table (ABI §3.4) and the code descriptor table, as `sakura_lir` emits them into
//! the data segment, in the byte format of ABI 1.0.1 §3.4 (plus the effect table, ABI P3); all
//! offsets are relative to the table's start, 0 = absent.
//!
//! Layout table:
//! ```text
//! 0   u32 n_types
//! 4   u32 option_type_id     std::option::Option (runtime builtins build Some/None)
//! 8   u32 result_type_id     std::result::Result
//! 12  u32 pair_type_id       the tuple row (word, word)   (map entries, zip)
//! 16  u32 int_pair_type_id   the tuple row (i64, word)    (enumerate)
//! 20  n_types × u32 entry offset
//! entry: u32 kind (0 record, 1 error record, 2 tuple, 3 variant), u32 name, u32 n_rows,
//!        n_rows × u32 row offset (one row; one per constructor for a variant)
//! row:   u32 size_bytes, u32 n_slots, w × u32 pointer_bitmap (w = max(1, ceil(n_slots/32))),
//!        u32 scalar_kinds, u32 name, u32 field_names (n_slots × u32 name offsets)
//! name:  u32 len, UTF-8 bytes
//! kinds: 2 bits per slot, slot i at bits 2·(i mod 16) of word i/16
//! ```
//! Code descriptor table (one entry per `code`, the funcref table index):
//! ```text
//! 0   u32 n_codes
//! 4   n_codes × u32 entry offset (0: no descriptor)
//! entry: u32 name, u32 n_frame_slots, u32 frame_kinds, u32 n_captures, u32 capture_kinds
//! ```

use crate::{Res, Rt};

/// Scalar kinds of a slot (ABI §3.4). A `Unit` slot is a `WORD` holding the `Unit` immediate.
pub const WORD: u32 = 0;
pub const I64: u32 = 1;
pub const F64: u32 = 2;
pub const BOOL: u32 = 3;

pub const RECORD: u32 = 0;
pub const ERROR: u32 = 1;
pub const TUPLE: u32 = 2;
pub const VARIANT: u32 = 3;

/// "No such type" in the header's well-known ids.
pub const NONE_ID: u32 = u32::MAX;

/// One row of the layout table, with absolute addresses.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub size: u32,
    pub n_slots: u32,
    bitmap: u32,
    kinds: u32,
    /// Address of the row's name (`u32 len`, bytes), 0 when none.
    pub name: u32,
    fields: u32,
}

/// A code descriptor, with absolute addresses.
#[derive(Clone, Copy, Debug)]
pub struct Code {
    pub name: u32,
    pub n_frame_slots: u32,
    frame_kinds: u32,
    pub n_captures: u32,
    capture_kinds: u32,
}

fn kind_at(rt: &Rt, kinds: u32, i: u32) -> u32 {
    if kinds == 0 {
        return WORD;
    }
    (rt.heap.mem.u32(kinds + i / 16 * 4) >> (2 * (i % 16))) & 3
}

impl Rt {
    /// Use the layout table at `addr` (the module's data segment; host tests use `install`).
    pub fn set_layout(&mut self, addr: u32) -> Res<()> {
        self.layout = addr;
        if self.option_id() != NONE_ID {
            let none = self.new_variant(self.option_id(), 0)?;
            self.make_static(none);
            self.none = none;
        }
        Ok(())
    }

    pub fn set_codes(&mut self, addr: u32) {
        self.codes = addr;
    }

    /// Use the effect table at `addr`: `u32 n`, then `n` name offsets (ABI P3).
    pub fn set_effects(&mut self, addr: u32) {
        self.effects = addr;
    }

    /// The qualified name of effect `id` (address of `u32 len`, bytes), 0 when unknown.
    pub fn effect_name(&self, id: u32) -> u32 {
        if self.effects == 0 || id >= self.heap.mem.u32(self.effects) {
            return 0;
        }
        self.table_at(self.effects, self.effects + 4 + 4 * id)
    }

    /// The table offset stored at `addr`, as an absolute address in `table` (0 stays 0).
    fn table_at(&self, table: u32, addr: u32) -> u32 {
        let v = self.heap.mem.u32(addr);
        if v == 0 { 0 } else { table + v }
    }

    fn entry(&self, type_id: u32) -> u32 {
        let n = self.heap.mem.u32(self.layout);
        assert!(type_id < n, "type id {type_id} outside the layout table");
        self.table_at(self.layout, self.layout + 20 + 4 * type_id)
    }

    /// Well-known ids from the table header (`NONE_ID` when the program has no such type).
    pub fn option_id(&self) -> u32 {
        self.heap.mem.u32(self.layout + 4)
    }
    pub fn result_id(&self) -> u32 {
        self.heap.mem.u32(self.layout + 8)
    }
    pub fn pair_id(&self) -> u32 {
        self.heap.mem.u32(self.layout + 12)
    }
    pub fn int_pair_id(&self) -> u32 {
        self.heap.mem.u32(self.layout + 16)
    }

    pub fn type_kind(&self, type_id: u32) -> u32 {
        self.heap.mem.u32(self.entry(type_id))
    }

    /// The type's name (address of `u32 len`, bytes), 0 when none.
    pub fn type_name(&self, type_id: u32) -> u32 {
        self.table_at(self.layout, self.entry(type_id) + 4)
    }

    pub fn n_rows(&self, type_id: u32) -> u32 {
        self.heap.mem.u32(self.entry(type_id) + 8)
    }

    pub fn row(&self, type_id: u32, ctor: u32) -> Row {
        let e = self.entry(type_id);
        assert!(
            ctor < self.heap.mem.u32(e + 8),
            "constructor {ctor} outside type {type_id}"
        );
        let r = self.table_at(self.layout, e + 12 + 4 * ctor);
        let m = &self.heap.mem;
        let n_slots = m.u32(r + 4);
        let w = n_slots.div_ceil(32).max(1);
        Row {
            size: m.u32(r),
            n_slots,
            bitmap: r + 8,
            kinds: self.table_at(self.layout, r + 8 + 4 * w),
            name: self.table_at(self.layout, r + 12 + 4 * w),
            fields: self.table_at(self.layout, r + 16 + 4 * w),
        }
    }

    /// Whether slot `i` of `row` owns a pointer (bit `i` of the pointer bitmap).
    pub fn owns(&self, row: &Row, i: u32) -> bool {
        (self.heap.mem.u32(row.bitmap + i / 32 * 4) >> (i % 32)) & 1 == 1
    }

    pub fn kind(&self, row: &Row, i: u32) -> u32 {
        kind_at(self, row.kinds, i)
    }

    /// Name of field `i` (address of `u32 len`, bytes), 0 when none.
    pub fn field_name(&self, row: &Row, i: u32) -> u32 {
        if row.fields == 0 {
            0
        } else {
            self.table_at(self.layout, row.fields + 4 * i)
        }
    }

    pub fn code(&self, code: u32) -> Option<Code> {
        if self.codes == 0 || code >= self.heap.mem.u32(self.codes) {
            return None;
        }
        let e = self.table_at(self.codes, self.codes + 4 + 4 * code);
        if e == 0 {
            return None;
        }
        let m = &self.heap.mem;
        Some(Code {
            name: self.table_at(self.codes, e),
            n_frame_slots: m.u32(e + 4),
            frame_kinds: self.table_at(self.codes, e + 8),
            n_captures: m.u32(e + 12),
            capture_kinds: self.table_at(self.codes, e + 16),
        })
    }

    pub fn frame_kind(&self, code: &Code, i: u32) -> u32 {
        kind_at(self, code.frame_kinds, i)
    }

    pub fn capture_kind(&self, code: &Code, i: u32) -> u32 {
        kind_at(self, code.capture_kinds, i)
    }
}
