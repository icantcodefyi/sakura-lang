//! Object headers, generic words, `Dup`/`Drop` and destruction (ABI §3.1, §3.5, §4).
//!
//! Destruction never recurses and never allocates: an object whose count reaches zero has no
//! further use for its `rc` field, so the worklist is a stack threaded through those fields
//! (ABI §3.5). Popping an object decrements its owned children, pushes the ones that
//! reach zero, runs the tag's destructor and frees the block.

use crate::layout::WORD;
use crate::{Defect, Res, Rt, tag};

/// Header flags (ABI §3.1).
pub const STATIC: u8 = 1;
pub const SCOPED: u8 = 2;

/// Generic-word immediates (ABI §4): low bits `01`, bit 2 set for `Bool`, bit 3 its value.
pub const UNIT: u32 = 0b0001;
pub const FALSE: u32 = 0b0101;
pub const TRUE: u32 = 0b1101;

/// A generic word that points to an object.
pub fn is_ptr(w: u32) -> bool {
    w != 0 && w & 3 == 0
}

pub fn bool_word(b: bool) -> u32 {
    if b { TRUE } else { FALSE }
}

/// Payload offsets shared by constructors, collections and destruction.
pub const SLOTS: u32 = 16;
/// First state slot of a `Handler` (after `effect_id`, `type_id`, `n_state`, `n_clauses`).
pub const HANDLER_STATE: u32 = 24;

impl Rt {
    /// A fresh object: zeroed payload, `rc = 1`, no flags.
    pub fn new_object(&mut self, tag: u16, size: u32) -> Res<u32> {
        let p = self.heap.alloc(size)?;
        self.heap.mem.set_u16(p, tag);
        self.heap.mem.set_u32(p + 4, 1);
        Ok(p)
    }

    pub fn tag(&self, p: u32) -> u16 {
        self.heap.mem.u16(p)
    }

    pub fn rc(&self, p: u32) -> u32 {
        self.heap.mem.u32(p + 4)
    }

    pub fn flags(&self, p: u32) -> u8 {
        self.heap.mem.u8(p + 3)
    }

    pub fn set_flags(&mut self, p: u32, flags: u8) {
        self.heap.mem.set_u8(p + 3, flags);
    }

    /// Mark `p` static: its count never changes and it is never freed (literals, singletons).
    pub fn make_static(&mut self, p: u32) {
        let f = self.flags(p);
        self.set_flags(p, f | STATIC);
    }

    fn counted(&self, w: u32) -> bool {
        is_ptr(w) && self.flags(w) & STATIC == 0
    }

    /// `Dup`: one more owner. `D092` instead of wrapping at 2^32 − 1.
    pub fn dup(&mut self, w: u32) -> Res<()> {
        if !self.counted(w) {
            return Ok(());
        }
        let rc = self.rc(w);
        debug_assert!(rc > 0, "dup of a dead object at {w}");
        let rc = rc.checked_add(1).ok_or(Defect::RC_OVERFLOW)?;
        self.heap.mem.set_u32(w + 4, rc);
        Ok(())
    }

    /// `Drop`: one owner fewer; at zero the object and everything it alone owns is released.
    pub fn drop(&mut self, w: u32) {
        let mut head = 0;
        self.dec(w, &mut head);
        self.release(head);
    }

    /// Release the objects on the worklist starting at `head`, and everything they alone own.
    pub(crate) fn release(&mut self, mut head: u32) {
        while head != 0 {
            let obj = head;
            head = self.heap.mem.u32(obj + 4);
            self.release_children(obj, &mut head);
            self.heap.free(obj);
        }
    }

    /// Decrement `w`; push it on the worklist `head` when it reaches zero.
    pub(crate) fn dec(&mut self, w: u32, head: &mut u32) {
        if !self.counted(w) {
            return;
        }
        let rc = self.rc(w);
        assert!(rc > 0, "drop of a dead object at {w}");
        if rc > 1 {
            self.heap.mem.set_u32(w + 4, rc - 1);
        } else {
            self.heap.mem.set_u32(w + 4, *head);
            *head = w;
        }
    }

    /// Decrement the word held in an 8-byte slot (pointer in the low half).
    fn dec_slot(&mut self, slot: u32, head: &mut u32) {
        let w = self.heap.mem.u32(slot);
        self.dec(w, head);
    }

    fn release_children(&mut self, obj: u32, head: &mut u32) {
        let m = |rt: &Rt, off: u32| rt.heap.mem.u32(obj + off);
        match self.tag(obj) {
            tag::TUPLE | tag::RECORD | tag::VARIANT => {
                let row = self.obj_row(obj);
                for i in 0..row.n_slots {
                    if self.owns(&row, i) {
                        self.dec_slot(obj + SLOTS + 8 * i, head);
                    }
                }
            }
            tag::CLOSURE => {
                // Captures are described by the code descriptor of `code` (ABI §3.4); one
                // beyond it is never dropped, as for frames. Without a descriptor (runtime-built
                // closures in tests), every capture is a word.
                let n = m(self, 12);
                match self.code(m(self, 8)) {
                    Some(c) => {
                        for i in 0..n.min(c.n_captures) {
                            if self.capture_kind(&c, i) == WORD {
                                self.dec_slot(obj + SLOTS + 8 * i, head);
                            }
                        }
                    }
                    None => {
                        for i in 0..n {
                            self.dec_slot(obj + SLOTS + 8 * i, head);
                        }
                    }
                }
            }
            tag::LIST_NODE => {
                for i in 0..m(self, 12) {
                    self.dec(m(self, 16 + 4 * i), head);
                }
            }
            tag::LIST_ROOT => {
                self.dec(m(self, 16), head);
                self.dec(m(self, 20), head);
            }
            tag::MAP_ROOT | tag::SET_ROOT => {
                self.dec(m(self, 12), head);
                self.dec(m(self, 16), head);
            }
            tag::HAMT_NODE => {
                for i in 0..m(self, 12) {
                    let a = m(self, 16 + 8 * i);
                    if a & 1 == 0 {
                        self.dec(a, head);
                    }
                }
            }
            tag::HANDLER => {
                // ABI 1.0.1 §3.2: state slots are described by the handler's layout row.
                let row = self.row(m(self, 12), 0);
                let (n_state, n_clauses) = (m(self, 16), m(self, 20));
                for i in 0..n_state {
                    if self.owns(&row, i) {
                        self.dec_slot(obj + HANDLER_STATE + 8 * i, head);
                    }
                }
                for i in 0..n_clauses {
                    self.dec(m(self, HANDLER_STATE + 8 * n_state + 4 * i), head);
                }
            }
            tag::SECRET => {
                let inner = m(self, 8);
                if self.counted(inner) && self.rc(inner) == 1 {
                    self.zero_payload(inner);
                }
                self.dec(inner, head);
            }
            tag::UNTRUSTED | tag::SOME_BOX => self.dec(m(self, 8), head),
            tag::HANDLE => {
                let (kind, slot, generation) = (m(self, 8), m(self, 12), m(self, 16));
                self.retire_handle(kind, slot, generation);
            }
            _ => {}
        }
    }

    /// `Secret` destructor: the bytes of a dying `String`/`Bytes` are overwritten.
    fn zero_payload(&mut self, p: u32) {
        let len = self.heap.mem.u32(p + 8);
        match self.tag(p) {
            tag::STRING => self.heap.mem.fill(p + 16, 0, len),
            tag::BYTES => self.heap.mem.fill(p + 12, 0, len),
            _ => {}
        }
    }
}
