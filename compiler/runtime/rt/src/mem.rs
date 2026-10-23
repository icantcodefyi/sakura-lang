//! Linear memory. On wasm32 this is the module's one memory (ABI §1); in a `native` build it is
//! the `offset32-v1` heap (ABI §1b): one 8-aligned mapping reserved up to `SAKURA_MAX_HEAP` and
//! committed in 64 KiB steps, so its base never moves; in a plain host build it is a `Vec<u8>`
//! with the same address space, so the allocator, objects and collections above it are
//! target-independent safe code and the host tests exercise exactly the bytes WASM would see.
//! Addresses are `u32` offsets; 0 is never allocated (ABI §3.1).

/// One WASM page.
pub const PAGE: u32 = 65_536;

/// Below this address the host heaps keep nothing; it stands in for rustc's data and stack.
#[cfg(not(target_arch = "wasm32"))]
const HOST_BASE: u32 = 4_096;

#[cfg(not(any(target_arch = "wasm32", feature = "native")))]
pub struct Mem {
    bytes: alloc::vec::Vec<u8>,
}

#[cfg(not(any(target_arch = "wasm32", feature = "native")))]
impl Mem {
    /// `limit` is the heap's; the vector grows on demand.
    pub fn new(_limit: u64) -> Option<Mem> {
        Some(Mem {
            bytes: alloc::vec![0; PAGE as usize],
        })
    }

    /// First address the heap may use.
    pub fn base(&self) -> u32 {
        HOST_BASE
    }

    /// Current memory size in bytes.
    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Grow by `pages`; false when the host refuses (here: never, the heap enforces the limit).
    pub fn grow(&mut self, pages: u32) -> bool {
        let len = self.bytes.len() + pages as usize * PAGE as usize;
        self.bytes.resize(len, 0);
        true
    }

    pub fn bytes(&self, addr: u32, len: u32) -> &[u8] {
        &self.bytes[addr as usize..addr as usize + len as usize]
    }

    pub fn bytes_mut(&mut self, addr: u32, len: u32) -> &mut [u8] {
        &mut self.bytes[addr as usize..addr as usize + len as usize]
    }

    pub fn copy(&mut self, dst: u32, src: u32, len: u32) {
        self.bytes
            .copy_within(src as usize..src as usize + len as usize, dst as usize);
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub use native::Mem;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod native {
    use core::ptr::NonNull;

    use super::{HOST_BASE, PAGE};

    /// The `offset32-v1` heap (ABI §1b, design §17.169 (1)): `reserved` bytes of address space
    /// mapped inaccessible at `base`, of which the first `size` are committed read-write. The
    /// base never moves, so generated code may cache it between allocating calls; an access past
    /// `size` faults in hardware as well as failing the checks below.
    pub struct Mem {
        base: NonNull<u8>,
        reserved: usize,
        size: usize,
    }

    // SAFETY: the mapping is owned by this value alone (no other pointer into it escapes except
    // the base a native instance publishes to code running on the thread that owns it), so moving
    // the owner to another thread moves every access with it.
    unsafe impl Send for Mem {}

    impl Mem {
        /// Reserves `limit` bytes (rounded up to 64 KiB) and commits the first 64 KiB; `None`
        /// when the system refuses the reservation.
        pub fn new(limit: u64) -> Option<Mem> {
            let reserved = usize::try_from(
                limit
                    .max(PAGE as u64)
                    .checked_next_multiple_of(PAGE as u64)?,
            )
            .ok()?;
            let base = reserve(reserved)?;
            let mut mem = Mem {
                base,
                reserved,
                size: 0,
            };
            mem.grow(1).then_some(mem)
        }

        pub fn base(&self) -> u32 {
            HOST_BASE
        }

        pub fn size(&self) -> u64 {
            self.size as u64
        }

        /// The address of offset 0 (ABI §1b: the context's heap base): fixed for the mapping's life.
        pub fn host_base(&self) -> *mut u8 {
            self.base.as_ptr()
        }

        /// Commit `pages` more 64 KiB pages; false past the reservation or when the system
        /// refuses.
        pub fn grow(&mut self, pages: u32) -> bool {
            let Some(len) = (pages as usize).checked_mul(PAGE as usize) else {
                return false;
            };
            let Some(end) = self.size.checked_add(len).filter(|&e| e <= self.reserved) else {
                return false;
            };
            // SAFETY: `size..end` lies inside the reservation (checked above).
            if !commit(unsafe { self.base.add(self.size) }, len) {
                return false;
            }
            self.size = end;
            true
        }

        fn check(&self, addr: u32, len: u32) -> usize {
            assert!(
                addr as u64 + len as u64 <= self.size as u64,
                "heap access out of bounds"
            );
            addr as usize
        }

        pub fn bytes(&self, addr: u32, len: u32) -> &[u8] {
            let at = self.check(addr, len);
            // SAFETY: the range is committed (checked above); the mapping lives as long as
            // `self`, and `&self` excludes every `&mut` view the methods below hand out.
            unsafe { core::slice::from_raw_parts(self.base.as_ptr().add(at), len as usize) }
        }

        pub fn bytes_mut(&mut self, addr: u32, len: u32) -> &mut [u8] {
            let at = self.check(addr, len);
            // SAFETY: as in `bytes`; `&mut self` makes this the only live view.
            unsafe { core::slice::from_raw_parts_mut(self.base.as_ptr().add(at), len as usize) }
        }

        pub fn copy(&mut self, dst: u32, src: u32, len: u32) {
            self.check(dst, len);
            self.check(src, len);
            // SAFETY: both ranges are committed (checked above); `copy` allows overlap.
            unsafe {
                core::ptr::copy(
                    self.base.as_ptr().add(src as usize),
                    self.base.as_ptr().add(dst as usize),
                    len as usize,
                )
            }
        }
    }

    impl Drop for Mem {
        fn drop(&mut self) {
            release(self.base, self.reserved);
        }
    }

    #[cfg(all(unix, not(miri)))]
    fn reserve(len: usize) -> Option<NonNull<u8>> {
        #[cfg(target_os = "linux")]
        const FLAGS: libc::c_int = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE;
        #[cfg(not(target_os = "linux"))]
        const FLAGS: libc::c_int = libc::MAP_PRIVATE | libc::MAP_ANON;
        // SAFETY: an anonymous mapping at an address of the system's choosing touches no
        // existing memory; the result is checked before use.
        let p = unsafe { libc::mmap(core::ptr::null_mut(), len, libc::PROT_NONE, FLAGS, -1, 0) };
        if p == libc::MAP_FAILED {
            return None;
        }
        NonNull::new(p.cast())
    }

    #[cfg(all(unix, not(miri)))]
    fn commit(at: NonNull<u8>, len: usize) -> bool {
        // SAFETY: `at..at + len` is page-aligned and inside a reservation `reserve` made.
        unsafe { libc::mprotect(at.as_ptr().cast(), len, libc::PROT_READ | libc::PROT_WRITE) == 0 }
    }

    #[cfg(all(unix, not(miri)))]
    fn release(base: NonNull<u8>, len: usize) {
        // SAFETY: exactly the mapping `reserve` made; nothing refers into it after `drop`.
        unsafe { libc::munmap(base.as_ptr().cast(), len) };
    }

    // Windows (platform C): reserve with `VirtualAlloc`, commit with it again, release once.
    #[cfg(all(windows, not(miri)))]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualAlloc(at: *mut u8, len: usize, kind: u32, protect: u32) -> *mut u8;
        fn VirtualFree(at: *mut u8, len: usize, kind: u32) -> i32;
    }
    #[cfg(all(windows, not(miri)))]
    const MEM_COMMIT: u32 = 0x1000;
    #[cfg(all(windows, not(miri)))]
    const MEM_RESERVE: u32 = 0x2000;
    #[cfg(all(windows, not(miri)))]
    const MEM_RELEASE: u32 = 0x8000;
    #[cfg(all(windows, not(miri)))]
    const PAGE_NOACCESS: u32 = 0x01;
    #[cfg(all(windows, not(miri)))]
    const PAGE_READWRITE: u32 = 0x04;

    #[cfg(all(windows, not(miri)))]
    fn reserve(len: usize) -> Option<NonNull<u8>> {
        // SAFETY: a reservation at an address of the system's choosing touches no existing
        // memory; the result is checked before use.
        NonNull::new(unsafe {
            VirtualAlloc(core::ptr::null_mut(), len, MEM_RESERVE, PAGE_NOACCESS)
        })
    }

    #[cfg(all(windows, not(miri)))]
    fn commit(at: NonNull<u8>, len: usize) -> bool {
        // SAFETY: `at..at + len` is page-aligned and inside a reservation `reserve` made.
        !unsafe { VirtualAlloc(at.as_ptr(), len, MEM_COMMIT, PAGE_READWRITE) }.is_null()
    }

    #[cfg(all(windows, not(miri)))]
    fn release(base: NonNull<u8>, _len: usize) {
        // SAFETY: exactly the reservation `reserve` made; nothing refers into it after `drop`.
        unsafe { VirtualFree(base.as_ptr(), 0, MEM_RELEASE) };
    }

    // Miri interprets no `mprotect`: the reservation is one zeroed allocation and committing is
    // bookkeeping only, so the checks above are what Miri exercises.
    #[cfg(miri)]
    fn layout(len: usize) -> std::alloc::Layout {
        std::alloc::Layout::from_size_align(len, PAGE as usize).expect("a page-aligned size")
    }

    #[cfg(miri)]
    fn reserve(len: usize) -> Option<NonNull<u8>> {
        // SAFETY: `layout` has a non-zero size (at least one page).
        NonNull::new(unsafe { std::alloc::alloc_zeroed(layout(len)) })
    }

    #[cfg(miri)]
    fn commit(_: NonNull<u8>, _: usize) -> bool {
        true
    }

    #[cfg(miri)]
    fn release(base: NonNull<u8>, len: usize) {
        // SAFETY: allocated by `reserve` with the same layout.
        unsafe { std::alloc::dealloc(base.as_ptr(), layout(len)) }
    }
}

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    /// End of rustc's static data and stack: the heap starts here.
    static __heap_base: u8;
}

#[cfg(target_arch = "wasm32")]
pub struct Mem {}

#[cfg(target_arch = "wasm32")]
impl Mem {
    /// The module's memory; its declared maximum is the limit (ABI §10).
    pub fn new(_limit: u64) -> Option<Mem> {
        Some(Mem {})
    }

    pub fn base(&self) -> u32 {
        // Only the address of the linker-provided symbol is taken, never its value.
        let base = core::ptr::addr_of!(__heap_base) as u32;
        (base + 7) & !7
    }

    pub fn size(&self) -> u64 {
        core::arch::wasm32::memory_size(0) as u64 * PAGE as u64
    }

    pub fn grow(&mut self, pages: u32) -> bool {
        core::arch::wasm32::memory_grow(0, pages as usize) != usize::MAX
    }

    pub fn bytes(&self, addr: u32, len: u32) -> &[u8] {
        assert!(addr as u64 + len as u64 <= self.size());
        // SAFETY: the range is inside the linear memory (checked above) and at or above the heap
        // base for every address the runtime hands out; nothing in Rust holds a reference into the
        // heap, and the module is single-threaded, so no `&mut` aliases this slice while it lives.
        unsafe { core::slice::from_raw_parts(addr as usize as *const u8, len as usize) }
    }

    pub fn bytes_mut(&mut self, addr: u32, len: u32) -> &mut [u8] {
        assert!(addr as u64 + len as u64 <= self.size());
        // SAFETY: as in `bytes`; `&mut self` makes this the only live view.
        unsafe { core::slice::from_raw_parts_mut(addr as usize as *mut u8, len as usize) }
    }

    pub fn copy(&mut self, dst: u32, src: u32, len: u32) {
        let end = dst.max(src) as u64 + len as u64;
        assert!(end <= self.size());
        // SAFETY: both ranges are inside the linear memory (checked above); `copy` allows overlap.
        unsafe {
            core::ptr::copy(
                src as usize as *const u8,
                dst as usize as *mut u8,
                len as usize,
            )
        }
    }
}

/// Little-endian scalar access on the host heaps.
#[cfg(not(target_arch = "wasm32"))]
impl Mem {
    pub fn u8(&self, addr: u32) -> u8 {
        self.bytes(addr, 1)[0]
    }

    pub fn set_u8(&mut self, addr: u32, v: u8) {
        self.bytes_mut(addr, 1)[0] = v;
    }

    pub fn u16(&self, addr: u32) -> u16 {
        let b = self.bytes(addr, 2);
        u16::from_le_bytes([b[0], b[1]])
    }

    pub fn set_u16(&mut self, addr: u32, v: u16) {
        self.bytes_mut(addr, 2).copy_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&self, addr: u32) -> u32 {
        let b = self.bytes(addr, 4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    pub fn set_u32(&mut self, addr: u32, v: u32) {
        self.bytes_mut(addr, 4).copy_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&self, addr: u32) -> u64 {
        let mut a = [0; 8];
        a.copy_from_slice(self.bytes(addr, 8));
        u64::from_le_bytes(a)
    }

    pub fn set_u64(&mut self, addr: u32, v: u64) {
        self.bytes_mut(addr, 8).copy_from_slice(&v.to_le_bytes());
    }

    pub fn fill(&mut self, addr: u32, byte: u8, len: u32) {
        self.bytes_mut(addr, len).fill(byte);
    }
}

/// Scalar access on wasm32: one load or store each, little-endian as the target is. The engine
/// bounds every access by the linear memory (one past it traps, which the host reports as `D090`,
/// as it does a runtime panic), so `bytes`' check would only repeat it, at a `memory.size` and a
/// compare per access in every runtime call.
#[cfg(target_arch = "wasm32")]
impl Mem {
    pub fn u8(&self, addr: u32) -> u8 {
        // SAFETY: a load of linear memory; the engine traps outside it. Single-threaded, and no
        // Rust reference into the heap is live across it (as in `bytes`).
        unsafe { (addr as usize as *const u8).read() }
    }

    pub fn set_u8(&mut self, addr: u32, v: u8) {
        // SAFETY: as in `u8`; `&mut self` makes this the only access.
        unsafe { (addr as usize as *mut u8).write(v) }
    }

    pub fn u16(&self, addr: u32) -> u16 {
        // SAFETY: as in `u8`.
        unsafe { (addr as usize as *const u16).read_unaligned() }
    }

    pub fn set_u16(&mut self, addr: u32, v: u16) {
        // SAFETY: as in `set_u8`.
        unsafe { (addr as usize as *mut u16).write_unaligned(v) }
    }

    pub fn u32(&self, addr: u32) -> u32 {
        // SAFETY: as in `u8`.
        unsafe { (addr as usize as *const u32).read_unaligned() }
    }

    pub fn set_u32(&mut self, addr: u32, v: u32) {
        // SAFETY: as in `set_u8`.
        unsafe { (addr as usize as *mut u32).write_unaligned(v) }
    }

    pub fn u64(&self, addr: u32) -> u64 {
        // SAFETY: as in `u8`.
        unsafe { (addr as usize as *const u64).read_unaligned() }
    }

    pub fn set_u64(&mut self, addr: u32, v: u64) {
        // SAFETY: as in `set_u8`.
        unsafe { (addr as usize as *mut u64).write_unaligned(v) }
    }

    pub fn fill(&mut self, addr: u32, byte: u8, len: u32) {
        // SAFETY: `memory.fill`, which traps outside the linear memory as a load does.
        unsafe { core::ptr::write_bytes(addr as usize as *mut u8, byte, len as usize) }
    }
}

/// Allocator annotations (design §17.173 (2), (3)). In an `asan` or `valgrind` build the
/// allocator tells the tool which heap bytes belong to a live block, so an access of a freed
/// block is reported although the heap is one mapping; elsewhere they compile to nothing. A
/// free block is closed whole except while the allocator reads its free-list link; a free page
/// run keeps its 8-byte header open, since the run list walks it.
impl Mem {
    /// Open `len` bytes at `addr` and fill them with `byte`: a block being handed out.
    pub fn init(&mut self, addr: u32, byte: u8, len: u32) {
        self.unpoison(addr, len);
        self.fill(addr, byte, len);
    }

    /// The free-list link of the free block at `block`, whose header is opened first.
    pub fn link(&mut self, block: u32) -> u32 {
        self.unpoison(block, 8);
        self.u32(block + 4)
    }

    /// Link the freed block of `size` bytes at `block` to `next`, then close the block.
    pub fn set_link(&mut self, block: u32, next: u32, size: u32) {
        self.set_u32(block + 4, next);
        self.poison(block, size);
    }

    /// Open the run header at `addr`, where a free run is split, and return `addr`.
    pub fn head(&mut self, addr: u32) -> u32 {
        self.unpoison(addr, 8);
        addr
    }

    /// Make `len` bytes at `addr` unaddressable to the tool.
    #[allow(unused_variables)]
    pub fn poison(&mut self, addr: u32, len: u32) {
        #[cfg(any(feature = "asan", feature = "valgrind"))]
        annotate::poison(self.bytes(addr, len));
    }

    /// Make `len` bytes at `addr` addressable (and defined) to the tool.
    #[allow(unused_variables)]
    pub fn unpoison(&mut self, addr: u32, len: u32) {
        #[cfg(any(feature = "asan", feature = "valgrind"))]
        annotate::unpoison(self.bytes(addr, len));
    }
}

#[cfg(any(feature = "asan", feature = "valgrind"))]
mod annotate {
    #[cfg(feature = "asan")]
    unsafe extern "C" {
        fn __asan_poison_memory_region(addr: *const u8, size: usize);
        fn __asan_unpoison_memory_region(addr: *const u8, size: usize);
    }

    pub fn poison(range: &[u8]) {
        #[cfg(feature = "asan")]
        // SAFETY: the range is inside the heap's mapping (`Mem::bytes` checked it); poisoning
        // changes ASan's shadow only.
        unsafe {
            __asan_poison_memory_region(range.as_ptr(), range.len())
        };
        #[cfg(feature = "valgrind")]
        let _ = crabgrind::memcheck::mark_memory(
            range.as_ptr().cast(),
            range.len(),
            crabgrind::memcheck::MemState::NoAccess,
        );
    }

    pub fn unpoison(range: &[u8]) {
        #[cfg(feature = "asan")]
        // SAFETY: as in `poison`.
        unsafe {
            __asan_unpoison_memory_region(range.as_ptr(), range.len())
        };
        #[cfg(feature = "valgrind")]
        let _ = crabgrind::memcheck::mark_memory(
            range.as_ptr().cast(),
            range.len(),
            crabgrind::memcheck::MemState::Defined,
        );
    }
}
