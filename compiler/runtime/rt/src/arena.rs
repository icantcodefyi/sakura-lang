//! The activation arena (ABI §7.2): execution-state objects (frames, activations, finalizer
//! entries, tasks, requests) are addressed by generational ids `slot << 8 | generation`, never by
//! pointers stored in other objects. Slot 0 is never used; reuse increments the generation and a
//! slot whose generation would wrap to 0 is retired for the instance's lifetime.
//!
//! The table is one heap block of 8-byte entries `(object address, generation)`; a free entry
//! holds `FREE | next free slot` instead of an address.

use crate::{Defect, Res, Rt};

const FREE: u32 = 1 << 31;

#[derive(Default)]
pub struct Arena {
    table: u32,
    cap: u32,
    /// Next never-used slot.
    top: u32,
    /// First free slot (0: none).
    free: u32,
    pub live: u32,
}

impl Rt {
    fn arena_entry(&self, slot: u32) -> u32 {
        self.arena.table + 8 + 8 * slot
    }

    /// Register `obj` and return its id.
    pub fn arena_new(&mut self, obj: u32) -> Res<u32> {
        let slot = if self.arena.free != 0 {
            let slot = self.arena.free;
            self.arena.free = self.heap.mem.u32(self.arena_entry(slot)) & !FREE;
            slot
        } else {
            if self.arena.top == 0 {
                self.arena.top = 1;
            }
            if self.arena.top >= self.arena.cap {
                self.arena_grow()?;
            }
            let slot = self.arena.top;
            self.arena.top += 1;
            self.heap.mem.set_u32(self.arena_entry(slot) + 4, 1);
            slot
        };
        if slot >= 1 << 24 {
            return Err(Defect::HEAP_EXHAUSTED);
        }
        let e = self.arena_entry(slot);
        self.heap.mem.set_u32(e, obj);
        self.arena.live += 1;
        Ok(slot << 8 | self.heap.mem.u32(e + 4))
    }

    fn arena_grow(&mut self) -> Res<()> {
        let cap = (self.arena.cap * 2).max(64);
        let size = cap.checked_mul(8).and_then(|b| b.checked_add(8));
        let table = self.heap.alloc(size.ok_or(Defect::HEAP_EXHAUSTED)?)?;
        if self.arena.table != 0 {
            self.heap
                .mem
                .copy(table + 8, self.arena.table + 8, 8 * self.arena.cap);
            self.heap.free(self.arena.table);
        }
        self.arena.table = table;
        self.arena.cap = cap;
        Ok(())
    }

    /// The object behind `id` if the id is current and the object has tag `tag`.
    pub fn arena_obj(&self, id: u32, tag: u16) -> Option<u32> {
        let slot = id >> 8;
        if id == 0 || slot >= self.arena.top {
            return None;
        }
        let e = self.arena_entry(slot);
        let obj = self.heap.mem.u32(e);
        let current = self.heap.mem.u32(e + 4) == id & 0xff && obj & FREE == 0 && obj != 0;
        (current && self.tag(obj) == tag).then_some(obj)
    }

    /// As `arena_obj`, with a stale id being the runtime bug `D090` (ABI §7.2).
    pub fn arena_get(&self, id: u32, tag: u16) -> Res<u32> {
        self.arena_obj(id, tag).ok_or(Defect::INTERNAL)
    }

    /// Retire `id`: the slot is reused with the next generation, or never again once the
    /// generation would wrap. The object itself is the caller's to release.
    pub fn arena_free(&mut self, id: u32) {
        let slot = id >> 8;
        let e = self.arena_entry(slot);
        debug_assert!(
            self.heap.mem.u32(e + 4) == id & 0xff,
            "arena_free of a stale id"
        );
        self.arena.live -= 1;
        let generation = self.heap.mem.u32(e + 4) + 1;
        if generation > 0xff {
            self.heap.mem.set_u32(e, FREE);
            self.heap.mem.set_u32(e + 4, 0);
            return;
        }
        self.heap.mem.set_u32(e + 4, generation);
        self.heap.mem.set_u32(e, FREE | self.arena.free);
        self.arena.free = slot;
    }
}
