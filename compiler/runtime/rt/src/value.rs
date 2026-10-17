//! Scalar boxes, strings and bytes (ABI §2, §3.2, §4).

use crate::object::SLOTS;
use crate::{Defect, Res, Rt, tag};

impl Rt {
    /// `IntBox` (tag 1): a 64-bit `Int` in a generic position.
    pub fn new_int(&mut self, v: i64) -> Res<u32> {
        let p = self.new_object(tag::INT_BOX, 16)?;
        self.heap.mem.set_u64(p + 8, v as u64);
        Ok(p)
    }

    /// `FloatBox` (tag 2); NaN payloads are kept as they are.
    pub fn new_float(&mut self, v: f64) -> Res<u32> {
        let p = self.new_object(tag::FLOAT_BOX, 16)?;
        self.heap.mem.set_u64(p + 8, v.to_bits());
        Ok(p)
    }

    pub fn int_of(&self, p: u32) -> i64 {
        self.heap.mem.u64(p + 8) as i64
    }

    pub fn float_of(&self, p: u32) -> f64 {
        f64::from_bits(self.heap.mem.u64(p + 8))
    }

    /// A zeroed `Bytes` (tag 4) of `len` bytes: the `alloc_bytes` export (ABI §8.1).
    pub fn new_bytes(&mut self, len: u32) -> Res<u32> {
        let size = len.checked_add(12).ok_or(Defect::HEAP_EXHAUSTED)?;
        let p = self.new_object(tag::BYTES, size)?;
        self.heap.mem.set_u32(p + 8, len);
        Ok(p)
    }

    /// A `Record` (tag 6) of layout `type_id`, slots zeroed (the caller fills them).
    pub fn new_record(&mut self, type_id: u32) -> Res<u32> {
        let row = self.row(type_id, 0);
        let p = self.new_object(tag::RECORD, row.size)?;
        self.heap.mem.set_u32(p + 8, type_id);
        Ok(p)
    }

    /// A `Variant` (tag 7) constructor `ctor` of `type_id`, slots zeroed.
    pub fn new_variant(&mut self, type_id: u32, ctor: u32) -> Res<u32> {
        let row = self.row(type_id, ctor);
        let p = self.new_object(tag::VARIANT, row.size)?;
        self.heap.mem.set_u32(p + 8, type_id);
        self.heap.mem.set_u32(p + 12, ctor);
        Ok(p)
    }

    /// A `Tuple` (tag 5) of the tuple row `type_id` (ABI.md errata E2), slots zeroed.
    pub fn new_tuple(&mut self, type_id: u32) -> Res<u32> {
        let row = self.row(type_id, 0);
        let p = self.new_object(tag::TUPLE, row.size)?;
        self.heap.mem.set_u32(p + 8, row.n_slots);
        self.heap.mem.set_u32(p + 12, type_id);
        Ok(p)
    }

    /// Slot `i` of a tuple, record, variant or closure (8 bytes, ABI §3.3).
    pub fn slot(&self, p: u32, i: u32) -> u64 {
        self.heap.mem.u64(p + SLOTS + 8 * i)
    }

    pub fn set_slot(&mut self, p: u32, i: u32, v: u64) {
        self.heap.mem.set_u64(p + SLOTS + 8 * i, v);
    }

    /// Copy `bytes` into a static `Bytes` object and return the address of its first byte:
    /// how host tests stand in for the data segment (layout and code tables, literals).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install(&mut self, bytes: &[u8]) -> Res<u32> {
        let p = self.new_bytes(u32::try_from(bytes.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?)?;
        self.make_static(p);
        self.heap
            .mem
            .bytes_mut(p + 12, bytes.len() as u32)
            .copy_from_slice(bytes);
        Ok(p + 12)
    }
}
