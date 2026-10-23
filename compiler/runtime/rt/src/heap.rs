//! The allocator (ABI §10): sixteen geometric size classes carved from 64 KiB slab chunks, one
//! free list each; larger blocks are page runs (`class = 0xFF`, run size in the `u32` at −8);
//! a 64 KiB emergency pool that only unwinding may use; growth in WASM pages up to the limit.
//! Every size computation is checked before alignment and growth; failure is `D096`.

use crate::mem::{Mem, PAGE};
use crate::{Defect, Res};

/// Block sizes of the small classes; the header's `class` byte indexes this table.
pub const CLASSES: [u32; 16] = [
    16, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096,
];
/// `class` of a page-run block.
pub const RUN: u8 = 0xFF;
/// Granularity of page runs (a large block costs at most one of these in slack).
pub const RUN_PAGE: u32 = 4_096;
/// Small classes are bump-allocated from chunks of this size.
const CHUNK: u32 = 65_536;
/// The emergency pool (ABI §10).
pub const EMERGENCY: u32 = 65_536;
/// `SAKURA_MAX_HEAP` default.
pub const DEFAULT_MAX_HEAP: u64 = 1 << 30;
/// The tag a free block carries; real objects never have tag 0 (ABI §3.2).
pub(crate) const FREE: u16 = 0;
/// The tag of an allocated block until its constructor writes the real one (reserved range).
const RAW: u16 = 0xFFFF;

/// `stats()` counters (ABI §10). Byte counts are block sizes, so `live` includes class slack.
/// `repr(C)`: on wasm32 compiled code updates `requested`, `live`, `live_objects`, `peak` and
/// `free_list_bytes` in place when it allocates or frees a small block itself ([`Heap::cells`]).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Stats {
    /// Bytes requested by every allocation so far (cumulative).
    pub requested: u64,
    /// Bytes in live blocks.
    pub live: u64,
    /// Number of live blocks.
    pub live_objects: u64,
    /// Linear memory committed above the heap base.
    pub committed: u64,
    /// The highest `live` so far.
    pub peak: u64,
    /// Bytes in free lists and free page runs.
    pub free_list_bytes: u64,
    /// Continuation frames allocated (design §17.118: direct regions allocate none).
    pub frames: u64,
    /// `Int`/`Float` boxes compiled code allocated (the LIR's `Box`).
    pub boxes: u64,
    /// Reference-count increments compiled code asked for (`rt_dup`: the LIR's `Dup` and the
    /// increments of projections and captures).
    pub dups: u64,
    /// Words given up through the `drop` export (compiled code's `Drop`s and the host's).
    pub drops: u64,
    /// Direct-region calls run again through frames after their twin ran out of gauge
    /// (ABI 1.2.0, design §17.160 (2)): at most one per entry from frame-protocol code.
    pub restarts: u64,
    /// Owned updates that tested whether they may write an object in place (ABI 1.3.0,
    /// design §17.176 (1)): a collection's wrapper and each node on the update's path.
    pub reuse_attempts: u64,
    /// The `reuse_attempts` that wrote the existing object instead of copying it.
    pub reuses: u64,
    /// Executed `Dup`s the LIR marked as a borrow candidate: a reference that stays live
    /// after a call which only reads it, so borrowing would remove the pair.
    pub borrow_candidates: u64,
    /// Calls that passed an argument under the borrowed convention; none before plan N4.
    pub borrows: u64,
}

/// `repr(C)`: on wasm32 compiled code pops and pushes the main pool's free lists ([`Heap::cells`]).
#[derive(Default)]
#[repr(C)]
struct Pool {
    free: [u32; 16],
    bump: u32,
    end: u32,
}

pub struct Heap {
    pub mem: Mem,
    /// Hard limit on the memory size in bytes (`SAKURA_MAX_HEAP`).
    limit: u64,
    /// End of the pages handed out so far.
    brk: u32,
    main: Pool,
    em: Pool,
    em_start: u32,
    /// Free page runs in address order; each starts with `[u32 size][u32 next]`.
    runs: u32,
    /// Set while a task unwinds: a failed allocation may then use the emergency pool.
    pub emergency: bool,
    /// The emergency pool could not serve a request: the task ends without more finalizers.
    pub em_exhausted: bool,
    pub(crate) stats: Stats,
}

/// The small class holding `size` bytes, if any.
pub fn class_of(size: u32) -> Option<usize> {
    CLASSES.iter().position(|&c| c >= size)
}

impl Heap {
    pub fn new(limit: u64) -> Res<Heap> {
        // A 32-bit address space: the break must stay representable.
        let limit = limit.min(u32::MAX as u64 + 1 - PAGE as u64);
        let mem = Mem::new(limit).ok_or(Defect::HEAP_EXHAUSTED)?;
        let base = mem.base().next_multiple_of(RUN_PAGE);
        let mut h = Heap {
            mem,
            limit,
            brk: base,
            main: Pool::default(),
            em: Pool::default(),
            em_start: 0,
            runs: 0,
            emergency: false,
            em_exhausted: false,
            stats: Stats::default(),
        };
        let (start, _) = h.pages(EMERGENCY)?;
        h.em = Pool {
            free: [0; 16],
            bump: start,
            end: start + EMERGENCY,
        };
        h.em_start = start;
        Ok(h)
    }

    /// What compiled code needs to allocate and free a small block inline (wasm32,
    /// `sakura_codegen_wasm`): the address of the main pool's sixteen free-list heads, of
    /// [`Stats`], and the start of the emergency pool, whose blocks it leaves to [`Heap::free`].
    /// Inline it does exactly what [`Heap::alloc`] does for a small class whose free list is not
    /// empty, and [`Heap::free`] for a small block outside the emergency pool.
    #[cfg(target_arch = "wasm32")]
    pub fn cells(&self) -> [u32; 3] {
        [
            self.main.free.as_ptr() as usize as u32,
            &self.stats as *const Stats as usize as u32,
            self.em_start,
        ]
    }

    pub fn stats(&self) -> Stats {
        let mut s = self.stats;
        s.committed = self.mem.size() - self.mem.base() as u64;
        s
    }

    /// A zeroed block of at least `size` bytes, 8-aligned, with its `class` byte (offset 2) set.
    pub fn alloc(&mut self, size: u32) -> Res<u32> {
        self.stats.requested += size as u64;
        let got = match class_of(size.max(8)) {
            Some(c) => {
                let small = match self.small(c) {
                    Err(_) if self.emergency => self.emergency_small(c),
                    r => r,
                };
                small.map(|addr| {
                    self.mem.init(addr, 0, CLASSES[c]);
                    self.mem.set_u16(addr, RAW);
                    self.mem.set_u8(addr + 2, c as u8);
                    (addr, CLASSES[c])
                })
            }
            None => {
                let total = (size as u64 + 8).next_multiple_of(RUN_PAGE as u64);
                let total = u32::try_from(total).map_err(|_| Defect::HEAP_EXHAUSTED);
                total.and_then(|t| self.pages(t)).map(|(start, total)| {
                    self.mem.init(start, 0, total);
                    self.mem.set_u32(start, total);
                    self.mem.set_u16(start + 8, RAW);
                    self.mem.set_u8(start + 8 + 2, RUN);
                    (start + 8, total)
                })
            }
        };
        let (addr, block) = got.inspect_err(|_| self.em_exhausted |= self.emergency)?;
        self.stats.live += block as u64;
        self.stats.live_objects += 1;
        self.stats.peak = self.stats.peak.max(self.stats.live);
        Ok(addr)
    }

    /// Return a block from `alloc`.
    pub fn free(&mut self, addr: u32) {
        assert!(self.mem.u16(addr) != FREE, "double free at {addr}");
        self.stats.live_objects -= 1;
        let class = self.mem.u8(addr + 2);
        self.mem.set_u16(addr, FREE);
        if class == RUN {
            let start = addr - 8;
            let total = self.mem.u32(start);
            self.stats.live -= total as u64;
            self.free_run(start, total);
            return;
        }
        let size = CLASSES[class as usize];
        self.stats.live -= size as u64;
        self.stats.free_list_bytes += size as u64;
        let in_em = addr >= self.em_start && addr < self.em_start + EMERGENCY;
        let pool = if in_em { &mut self.em } else { &mut self.main };
        let next = pool.free[class as usize];
        pool.free[class as usize] = addr;
        self.mem.set_link(addr, next, size);
    }

    /// Size of the block at `addr` (its class size or run size).
    pub fn block_size(&self, addr: u32) -> u32 {
        match self.mem.u8(addr + 2) {
            RUN => self.mem.u32(addr - 8) - 8,
            c => CLASSES[c as usize],
        }
    }

    fn small(&mut self, c: usize) -> Res<u32> {
        let size = CLASSES[c];
        if let Some(a) = Self::pop(&mut self.mem, &mut self.main, c) {
            self.stats.free_list_bytes -= size as u64;
            return Ok(a);
        }
        if self.main.end - self.main.bump < size {
            let (start, _) = self.pages(CHUNK)?;
            self.main.bump = start;
            self.main.end = start + CHUNK;
        }
        let a = self.main.bump;
        self.main.bump += size;
        Ok(a)
    }

    fn emergency_small(&mut self, c: usize) -> Res<u32> {
        let size = CLASSES[c];
        if let Some(a) = Self::pop(&mut self.mem, &mut self.em, c) {
            self.stats.free_list_bytes -= size as u64;
            return Ok(a);
        }
        if self.em.end - self.em.bump < size {
            return Err(Defect::HEAP_EXHAUSTED);
        }
        let a = self.em.bump;
        self.em.bump += size;
        Ok(a)
    }

    fn pop(mem: &mut Mem, pool: &mut Pool, c: usize) -> Option<u32> {
        let a = pool.free[c];
        if a == 0 {
            return None;
        }
        pool.free[c] = mem.link(a);
        Some(a)
    }

    /// `total` bytes (a multiple of `RUN_PAGE`) of pages: first fit among free runs, else from
    /// the break, growing memory. Returns the start and the size actually taken.
    /// ponytail: first fit over an address-ordered list, O(free runs); a size-bucketed index
    /// when large-object churn shows up in `stats()`.
    fn pages(&mut self, total: u32) -> Res<(u32, u32)> {
        let (mut prev, mut r) = (0, self.runs);
        while r != 0 {
            let size = self.mem.u32(r);
            let next = self.mem.u32(r + 4);
            if size >= total {
                let taken = if size - total >= RUN_PAGE {
                    total
                } else {
                    size
                };
                let rest = if taken < size {
                    let rest = self.mem.head(r + taken);
                    self.mem.set_u32(rest, size - taken);
                    self.mem.set_u32(rest + 4, next);
                    rest
                } else {
                    next
                };
                if prev == 0 {
                    self.runs = rest;
                } else {
                    self.mem.set_u32(prev + 4, rest);
                }
                self.stats.free_list_bytes -= taken as u64;
                return Ok((r, taken));
            }
            (prev, r) = (r, next);
        }
        let end = self.brk as u64 + total as u64;
        let size = self.mem.size();
        if end > size {
            let grow = (end - size).div_ceil(PAGE as u64);
            if size + grow * PAGE as u64 > self.limit || !self.mem.grow(grow as u32) {
                return Err(Defect::HEAP_EXHAUSTED);
            }
        }
        let start = self.brk;
        self.brk = end as u32;
        Ok((start, total))
    }

    fn free_run(&mut self, start: u32, total: u32) {
        // The run's header stays open: the free-run list walks it (`mem.rs`, annotations).
        self.mem.poison(start + 8, total - 8);
        self.stats.free_list_bytes += total as u64;
        let (mut prev, mut r) = (0, self.runs);
        while r != 0 && r < start {
            (prev, r) = (r, self.mem.u32(r + 4));
        }
        let (mut start, mut total) = (start, total);
        let mut next = r;
        if next != 0 && start + total == next {
            total += self.mem.u32(next);
            next = self.mem.u32(next + 4);
        }
        if prev != 0 && prev + self.mem.u32(prev) == start {
            start = prev;
            total += self.mem.u32(prev);
        } else if prev != 0 {
            self.mem.set_u32(prev + 4, start);
        } else {
            self.runs = start;
        }
        self.mem.set_u32(start, total);
        self.mem.set_u32(start + 4, next);
    }
}
