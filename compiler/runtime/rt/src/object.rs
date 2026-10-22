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
/// A `String` or `Bytes` a `Secret` held (ABI 1.19.0, plan M5 N-S): its bytes are overwritten
/// when its block is freed, wherever it went after a reveal.
pub const WIPE: u8 = 4;

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
        #[cfg(feature = "regions")]
        self.regions.alloc(p, size);
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

    /// Whether `p` is a `Bytes` the host may hand over as owned (`complete`, `invoke`; ABI
    /// §8.5, security review W11): inside the heap and memory, tag `BYTES`, `rc = 1`, not
    /// static, its data inside memory. Anything else is refused untouched.
    pub fn host_bytes(&self, p: u32) -> bool {
        let size = self.heap.mem.size();
        is_ptr(p)
            && p >= self.heap.mem.base()
            && u64::from(p) + 12 <= size
            && self.tag(p) == tag::BYTES
            && self.rc(p) == 1
            && self.flags(p) & STATIC == 0
            && u64::from(p) + 12 + u64::from(self.heap.mem.u32(p + 8)) <= size
    }

    /// No other reference reaches `w`: an update may write it in place (design §17.149).
    pub(crate) fn unique(&self, w: u32) -> bool {
        self.counted(w) && self.rc(w) == 1
    }

    /// An owned update testing whether it may write `w` in place: [`Rt::unique`], counted
    /// (ABI 1.3.0 `reuse_attempts` and `reuses`, design §17.176 (1)). A collection update
    /// tests its wrapper and then each node on its path, so a hit on the wrapper never hides
    /// a copied node below it.
    pub(crate) fn reusable(&mut self, w: u32) -> bool {
        let u = self.unique(w);
        self.heap.stats.reuse_attempts += 1;
        self.heap.stats.reuses += u64::from(u);
        u
    }

    /// `reuse_reset` (plan M5 N3, design §17.176 (3)): the reuse token for the tuple, record or
    /// variant `w`, which the caller owns. When no other reference reaches `w`, its children are
    /// released and its slots zeroed, and `w` itself is the token: an object of its row whose
    /// slots own nothing, so dropping an unused token frees it. Otherwise `w` is dropped and the
    /// token is `Unit`. Each call is a reuse attempt; a returned object is a reuse.
    pub fn reuse_reset(&mut self, w: u32) -> u32 {
        if !self.reusable(w) {
            self.drop(w);
            return UNIT;
        }
        debug_assert!(matches!(
            self.tag(w),
            tag::TUPLE | tag::RECORD | tag::VARIANT
        ));
        let row = self.obj_row(w);
        let mut head = 0;
        for i in 0..row.n_slots {
            if self.owns(&row, i) {
                self.dec_slot(w + SLOTS + 8 * i, &mut head);
            }
        }
        self.heap.mem.fill(w + SLOTS, 0, 8 * row.n_slots);
        self.release(head);
        w
    }

    /// The block an `Alloc` reusing `token` writes ([`Rt::reuse_reset`]): the token's own
    /// storage, or a fresh object when it is `Unit`. The compiler pairs a token only with an
    /// allocation of its row, so the sizes agree.
    pub fn reuse_or_alloc(&mut self, token: u32, tag: u16, size: u32) -> Res<u32> {
        if is_ptr(token) {
            return Ok(token);
        }
        self.new_object(tag, size)
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

    /// Drops argument `i` of a builtin unless bit `i` of `keep` lends it to the call: a borrowed
    /// builtin (`rt_bb_<name>`, ABI 1.18.0, plan M5 N4a) leaves those to the caller, which holds
    /// them for the whole call.
    pub(crate) fn drop_arg(&mut self, keep: u32, i: u32, w: u32) {
        if keep & 1 << i == 0 {
            self.drop(w);
        }
    }

    /// Release the objects on the worklist starting at `head`, and everything they alone own.
    pub(crate) fn release(&mut self, mut head: u32) {
        while head != 0 {
            let obj = head;
            head = self.heap.mem.u32(obj + 4);
            self.release_children(obj, &mut head);
            #[cfg(feature = "regions")]
            self.regions.free(obj);
            self.heap.free(obj);
        }
    }

    /// Decrement `w`; push it on the worklist `head` when it reaches zero.
    pub(crate) fn dec(&mut self, w: u32, head: &mut u32) {
        if !self.counted(w) {
            return;
        }
        debug_assert_ne!(
            self.tag(w),
            crate::heap::FREE,
            "drop of a freed object at {w}"
        );
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
                // ABI 1.0.1 §3.2: state slots are described by the handler's layout row; a
                // host-built handler has no state and no row (errata L11).
                let (n_state, n_clauses) = (m(self, 16), m(self, 20));
                if n_state > 0 {
                    let row = self.row(m(self, 12), 0);
                    for i in 0..n_state {
                        if self.owns(&row, i) {
                            self.dec_slot(obj + HANDLER_STATE + 8 * i, head);
                        }
                    }
                }
                for i in 0..n_clauses {
                    self.dec(m(self, HANDLER_STATE + 8 * n_state + 4 * i), head);
                }
            }
            tag::SECRET => {
                let inner = m(self, 8);
                if self.counted(inner) {
                    let f = self.flags(inner);
                    self.set_flags(inner, f | WIPE);
                }
                self.dec(inner, head);
            }
            tag::STRING | tag::BYTES if self.flags(obj) & WIPE != 0 => self.zero_payload(obj),
            tag::UNTRUSTED | tag::SOME_BOX => self.dec(m(self, 8), head),
            tag::HANDLE => {
                let (kind, slot, generation) = (m(self, 8), m(self, 12), m(self, 16));
                self.retire_handle(kind, slot, generation);
            }
            tag::CHANNEL => self.channel_release(obj, head),
            _ => {}
        }
    }

    /// A `WIPE` `String`'s or `Bytes`' destructor: its bytes are overwritten.
    fn zero_payload(&mut self, p: u32) {
        let (len, at) = self.data(p);
        self.heap.mem.fill(at, 0, len);
    }
}
