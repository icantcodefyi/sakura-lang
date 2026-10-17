//! Linear memory. On wasm32 this is the module's one memory (ABI §1); everywhere else it is a
//! `Vec<u8>` with the same address space, so the allocator, objects and collections above it are
//! target-independent safe code and the host tests exercise exactly the bytes WASM would see.
//! Addresses are `u32` offsets; 0 is never allocated (ABI §3.1).

/// One WASM page.
pub const PAGE: u32 = 65_536;

pub struct Mem {
    #[cfg(not(target_arch = "wasm32"))]
    bytes: alloc::vec::Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Mem {
    /// Below this address the host model keeps nothing; it stands in for rustc's data and stack.
    const BASE: u32 = 4_096;

    pub fn new() -> Mem {
        Mem {
            bytes: alloc::vec![0; PAGE as usize],
        }
    }

    /// First address the heap may use.
    pub fn base(&self) -> u32 {
        Self::BASE
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

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    /// End of rustc's static data and stack: the heap starts here.
    static __heap_base: u8;
}

#[cfg(target_arch = "wasm32")]
impl Mem {
    pub fn new() -> Mem {
        Mem {}
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

impl Default for Mem {
    fn default() -> Mem {
        Mem::new()
    }
}

/// Little-endian scalar access shared by both backends.
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
