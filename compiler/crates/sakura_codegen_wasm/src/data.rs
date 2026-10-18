//! The static block: the program's literals and tables, one passive data segment that `start`
//! copies into a block the runtime allocates once (`rt_alloc_static`) and never frees. Every
//! object in it is position-independent (headers, lengths, ids; table offsets are relative to
//! each table's start, ABI §3.4), so the block can live at any address; compiled code adds the
//! block's address (a global) to the offsets below.

use std::collections::HashMap;

/// ABI §3.1 header flag.
const STATIC: u8 = 1;

#[derive(Default)]
pub struct Statics {
    pub bytes: Vec<u8>,
    strings: HashMap<String, u32>,
    nullary: HashMap<(u32, u32), u32>,
    closures: HashMap<u32, u32>,
    raw: HashMap<Vec<u8>, u32>,
}

impl Statics {
    fn align(&mut self, to: usize) -> u32 {
        while !self.bytes.len().is_multiple_of(to) {
            self.bytes.push(0);
        }
        self.bytes.len() as u32
    }

    /// An 8-aligned object header: tag, class 0, flags `static`, `rc = 1` (ABI §3.1).
    fn header(&mut self, tag: u16) -> u32 {
        let at = self.align(8);
        self.bytes.extend_from_slice(&tag.to_le_bytes());
        self.bytes.extend_from_slice(&[0, STATIC]);
        self.bytes.extend_from_slice(&1u32.to_le_bytes());
        at
    }

    fn u32(&mut self, v: u32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    /// A static `String` object (tag 3): `byte_len`, hash 0 (not computed), UTF-8 bytes.
    pub fn string(&mut self, s: &str) -> u32 {
        if let Some(&o) = self.strings.get(s) {
            return o;
        }
        let at = self.header(3);
        self.u32(s.len() as u32);
        self.u32(0);
        self.bytes.extend_from_slice(s.as_bytes());
        self.strings.insert(s.into(), at);
        at
    }

    /// The static singleton of nullary constructor `ctor` of `type_id` (tag 7, no slots).
    pub fn nullary(&mut self, type_id: u32, ctor: u32) -> u32 {
        if let Some(&o) = self.nullary.get(&(type_id, ctor)) {
            return o;
        }
        let at = self.header(7);
        self.u32(type_id);
        self.u32(ctor);
        self.nullary.insert((type_id, ctor), at);
        at
    }

    /// The static closure (tag 8, no captures) of code `code`.
    pub fn closure(&mut self, code: u32) -> u32 {
        if let Some(&o) = self.closures.get(&code) {
            return o;
        }
        let at = self.header(8);
        self.u32(code);
        self.u32(0);
        self.closures.insert(code, at);
        at
    }

    /// Raw bytes (a table or a defect message), 8-aligned, deduplicated.
    pub fn bytes(&mut self, b: &[u8]) -> u32 {
        if let Some(&o) = self.raw.get(b) {
            return o;
        }
        let at = self.align(8);
        self.bytes.extend_from_slice(b);
        self.raw.insert(b.to_vec(), at);
        at
    }

    pub fn len(&mut self) -> u32 {
        self.align(8)
    }
}
