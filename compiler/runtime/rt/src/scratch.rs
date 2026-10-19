//! Scratch structures in linear memory for the runtime's own loops: a growable `u32` stack (the
//! explicit stacks that keep `Show`, `Eq`, `Ord` and `Hash` free of recursion, so deep values
//! cannot exhaust the native stack, which on wasm32 has no guard) and a string builder.
//! Blocks come from the heap like any object; failure is `D096` and the caller frees.

use crate::{Defect, Res, Rt, tag};

/// A growable array of `u32` in one heap block.
#[derive(Clone, Copy, Debug)]
pub struct Stack {
    block: u32,
    pub len: u32,
    cap: u32,
}

impl Stack {
    /// A stack with no storage yet (the first push allocates it).
    pub const NONE: Stack = Stack {
        block: 0,
        len: 0,
        cap: 0,
    };

    /// A stack kept in 12 bytes of linear memory (`block`, `len`, `cap`).
    pub fn load(mem: &crate::mem::Mem, at: u32) -> Stack {
        Stack {
            block: mem.u32(at),
            len: mem.u32(at + 4),
            cap: mem.u32(at + 8),
        }
    }

    pub fn store(&self, mem: &mut crate::mem::Mem, at: u32) {
        mem.set_u32(at, self.block);
        mem.set_u32(at + 4, self.len);
        mem.set_u32(at + 8, self.cap);
    }
}

impl Rt {
    pub fn stack(&mut self) -> Res<Stack> {
        let block = self.heap.alloc(8 + 4 * 30)?;
        Ok(Stack {
            block,
            len: 0,
            cap: 30,
        })
    }

    pub fn stack_free(&mut self, s: Stack) {
        if s.block != 0 {
            self.heap.free(s.block);
        }
    }

    pub fn push(&mut self, s: &mut Stack, v: u32) -> Res<()> {
        if s.len == s.cap {
            let cap = s.cap.checked_mul(2).ok_or(Defect::HEAP_EXHAUSTED)?.max(14);
            let size = cap.checked_mul(4).and_then(|b| b.checked_add(8));
            let block = self.heap.alloc(size.ok_or(Defect::HEAP_EXHAUSTED)?)?;
            if s.block != 0 {
                self.heap.mem.copy(block + 8, s.block + 8, 4 * s.len);
                self.heap.free(s.block);
            }
            *s = Stack {
                block,
                len: s.len,
                cap,
            };
        }
        self.heap.mem.set_u32(s.block + 8 + 4 * s.len, v);
        s.len += 1;
        Ok(())
    }

    pub fn pop(&mut self, s: &mut Stack) -> Option<u32> {
        if s.len == 0 {
            return None;
        }
        s.len -= 1;
        Some(self.heap.mem.u32(s.block + 8 + 4 * s.len))
    }

    pub fn stack_set(&mut self, s: &Stack, i: u32, v: u32) {
        self.heap.mem.set_u32(s.block + 8 + 4 * i, v);
    }

    /// Element `i` from the bottom.
    pub fn stack_at(&self, s: &Stack, i: u32) -> u32 {
        self.heap.mem.u32(s.block + 8 + 4 * i)
    }
}

/// A `String` under construction: the object grows by copying until `finish`. A `wipe` buffer
/// holds key material or a `Secret`'s value (ABI §8.1): every block it leaves behind is
/// overwritten before it is freed.
pub struct StrBuf {
    obj: u32,
    len: u32,
    cap: u32,
    wipe: bool,
}

impl StrBuf {
    /// The `String` object being built (its bytes start at offset 16).
    pub fn obj(&self) -> u32 {
        self.obj
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Overwrite every block this buffer frees from now on, and the `String` `finish_bytes`
    /// copies from.
    pub fn wipe(&mut self) {
        self.wipe = true;
    }

    /// Keep only the first `n` bytes.
    pub fn truncate(&mut self, n: u32) {
        self.len = self.len.min(n);
    }
}

impl Rt {
    pub fn strbuf(&mut self) -> Res<StrBuf> {
        let obj = self.new_object(tag::STRING, 64)?;
        Ok(StrBuf {
            obj,
            len: 0,
            cap: 64 - 16,
            wipe: false,
        })
    }

    fn reserve(&mut self, b: &mut StrBuf, more: u32) -> Res<()> {
        let need = b.len.checked_add(more).ok_or(Defect::HEAP_EXHAUSTED)?;
        if need <= b.cap {
            return Ok(());
        }
        let cap = need.max(b.cap.saturating_mul(2));
        let obj = self.new_object(
            tag::STRING,
            cap.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?,
        )?;
        self.heap.mem.copy(obj + 16, b.obj + 16, b.len);
        if b.wipe {
            self.heap.mem.fill(b.obj + 16, 0, b.len);
        }
        self.heap.free(b.obj);
        b.obj = obj;
        b.cap = self.heap.block_size(obj) - 16;
        Ok(())
    }

    pub fn put(&mut self, b: &mut StrBuf, bytes: &[u8]) -> Res<()> {
        self.reserve(b, bytes.len() as u32)?;
        self.heap
            .mem
            .bytes_mut(b.obj + 16 + b.len, bytes.len() as u32)
            .copy_from_slice(bytes);
        b.len += bytes.len() as u32;
        Ok(())
    }

    /// Append `len` bytes of linear memory at `addr`.
    pub fn put_mem(&mut self, b: &mut StrBuf, addr: u32, len: u32) -> Res<()> {
        self.reserve(b, len)?;
        self.heap.mem.copy(b.obj + 16 + b.len, addr, len);
        b.len += len;
        Ok(())
    }

    pub fn finish(&mut self, b: StrBuf) -> u32 {
        self.heap.mem.set_u32(b.obj + 8, b.len);
        b.obj
    }

    pub fn strbuf_free(&mut self, b: StrBuf) {
        if b.wipe {
            self.heap.mem.fill(b.obj + 16, 0, b.len);
        }
        self.heap.free(b.obj);
    }

    /// The buffer's bytes as a `Bytes` (`string_to_bytes` of `finish`), the `String` overwritten
    /// first for a `wipe` buffer.
    pub fn finish_bytes(&mut self, b: StrBuf) -> Res<u32> {
        let wipe = b.wipe;
        let s = self.finish(b);
        if wipe {
            let (len, at) = self.data(s);
            let r = self.new_bytes(len);
            if let Ok(o) = r {
                self.heap.mem.copy(o + 12, at, len);
            }
            self.heap.mem.fill(at, 0, len);
            self.drop(s);
            return r;
        }
        self.string_to_bytes(s)
    }
}
