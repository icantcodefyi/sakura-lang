//! `std/compress` (RFC 0002 §2.6): raw DEFLATE over `miniz_oxide`, CRC-32 and Adler-32, the
//! twins of the JS runtime's `compress_*` functions over `node:zlib`. `std/compress` reads and
//! writes the gzip and zlib framing and checks every bound before it calls here; this module
//! only codes DEFLATE. Arguments are taken owned and results returned owned (ABI.md P2).
//!
//! A decoding state (`std/compress`'s `Inflater`) is a `Bytes` object of the guest heap that
//! only these functions read: the input bytes the last push took, whether the stream ended, a
//! 64 KiB window and a `miniz_oxide` `DecompressorOxide`, copied in and out around each step so
//! no reference into linear memory outlives a call. The window holds the last 32 KiB of output
//! (all of it while there is less) before the room the next step writes into, and is decoded
//! into as a non-wrapping buffer, so a distance before the start of the stream is malformed as
//! zlib finds it, rather than read from a zeroed history. The state needs no host import and no
//! release beyond its reference count. Compression allocates (`CompressorOxide` boxes its
//! tables): on wasm32, where the runtime has no allocator of its own, the boxes live in a heap
//! block of the guest that exists for one call (`arena`).

use alloc::vec::Vec;
use core::mem::size_of;

use miniz_oxide::deflate::core::{
    CompressorOxide, TDEFLFlush, TDEFLStatus, compress_to_output, create_comp_flags_from_zip_params,
};
use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::inflate_flags::{
    TINFL_FLAG_HAS_MORE_INPUT, TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
};
use miniz_oxide::inflate::core::{DecompressorOxide, decompress};

use crate::{Defect, Res, Rt};

/// The error codes `std/compress`'s `reason` reads.
pub const MALFORMED: i64 = 0;
pub const OUTPUT_LIMIT: i64 = 1;
pub const TRUNCATED: i64 = 2;

/// Input bytes one step reads at most (copied to the stack first).
const PIECE: u32 = 16_384;
/// DEFLATE's largest distance, the history a step keeps.
const HISTORY: u32 = 32_768;
const WINDOW: u32 = 2 * HISTORY;

/// A decoding state's payload: `used`, `done`, the history's length, the window, then the
/// decompressor.
const USED: u32 = 0;
const DONE: u32 = 4;
const HIST: u32 = 8;
const WIN: u32 = 16;
const DECOMP: u32 = WIN + WINDOW;
const STATE_SIZE: u32 = DECOMP + size_of::<DecompressorOxide>() as u32;

/// What one step did: the input it read, where its output is (an address in the window) and
/// how long, and the decompressor's status.
struct Step {
    used: u32,
    at: u32,
    len: u32,
    status: TINFLStatus,
}

const CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 == 1 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
};

/// CRC-32 (ISO 3309, as gzip and `zlib.crc32`) of `b`, continuing from `crc`.
pub fn crc32(crc: u32, b: &[u8]) -> u32 {
    let mut c = !crc;
    for &x in b {
        c = CRC_TABLE[((c ^ u32::from(x)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// Adler-32 (RFC 1950 §8.2) of `b`, continuing from `adler`.
pub fn adler32(adler: u32, b: &[u8]) -> u32 {
    miniz_oxide::mz_adler32_oxide(adler, b)
}

/// An upper bound of raw DEFLATE's size for `n` input bytes: stored blocks cost 5 bytes per
/// 65,535, plus the final or sync flush.
fn deflate_bound(n: u32) -> usize {
    n as usize + n as usize / 64 + 4096
}

impl Rt {
    /// `compress_crc32(crc, b)`.
    pub fn compress_crc32(&mut self, crc: i64, b: u32, keep: u32) -> i64 {
        let (n, at) = self.data(b);
        let r = crc32(crc as u32, self.heap.mem.bytes(at, n));
        self.drop_arg(keep, 1, b);
        i64::from(r)
    }

    /// `compress_adler32(adler, b)`.
    pub fn compress_adler32(&mut self, adler: i64, b: u32, keep: u32) -> i64 {
        let (n, at) = self.data(b);
        let r = adler32(adler as u32, self.heap.mem.bytes(at, n));
        self.drop_arg(keep, 1, b);
        i64::from(r)
    }

    /// `compress_deflate(b, level, last)`: raw DEFLATE of `b` at `level` (0..=9), ending with
    /// the final block when `last` is 1, else with a sync flush.
    pub fn compress_deflate(&mut self, b: u32, level: i64, last: i64, keep: u32) -> Res<u32> {
        let (n, at) = self.data(b);
        let bound = deflate_bound(n);
        let r = with_arena(self, bound, |rt| {
            let flags = create_comp_flags_from_zip_params(level.clamp(0, 9) as i32, -15, 0);
            let mut d = CompressorOxide::new(flags);
            let mut out: Vec<u8> = Vec::with_capacity(bound);
            let flush = if last == 1 {
                TDEFLFlush::Finish
            } else {
                TDEFLFlush::Sync
            };
            let (status, _) = compress_to_output(&mut d, rt.heap.mem.bytes(at, n), flush, |c| {
                let room = out.len() + c.len() <= out.capacity();
                if room {
                    out.extend_from_slice(c);
                }
                room
            });
            if !matches!(status, TDEFLStatus::Okay | TDEFLStatus::Done) {
                return Err(Defect::INTERNAL);
            }
            rt.bytes_of(&out)
        });
        self.drop_arg(keep, 0, b);
        r
    }

    /// `compress_inflate(b, at, max)`: the raw DEFLATE stream at `b[at..]` as `Ok((output,
    /// rest))`, `rest` the input after the stream's end, or `Err(code)`. Two passes, the first
    /// counting, so the output is allocated once at its size.
    pub fn compress_inflate(&mut self, b: u32, at: i64, max: i64, keep: u32) -> Res<u32> {
        let r = self.inflate_whole(b, at, max.max(0) as u64);
        self.drop_arg(keep, 0, b);
        r
    }

    fn inflate_whole(&mut self, b: u32, at: i64, max: u64) -> Res<u32> {
        let n = self.data(b).0;
        let from = at.clamp(0, i64::from(n)) as u32;
        let s = self.compress_inflater()?;
        let r = match self.inflate_pass(s, b, from, max, None) {
            Err(code) => self.new_int(code).and_then(|e| self.err(e)),
            Ok((len, used)) => (|| {
                let len = u32::try_from(len).map_err(|_| Defect::HEAP_EXHAUSTED)?;
                let out = self.new_bytes(len)?;
                // The second pass decodes what the first did, so it cannot fail.
                self.reset_state(s);
                let _ = self.inflate_pass(s, b, from, max, Some(out + 12));
                let tail = n - from - used;
                let rest = self.new_bytes(tail).inspect_err(|_| self.drop(out))?;
                let p = self.data(b).1;
                self.heap.mem.copy(rest + 12, p + from + used, tail);
                let pair = self.pair(out, rest)?;
                self.ok(pair)
            })(),
        };
        self.drop(s);
        r
    }

    /// One pass over `b[from..]` with the fresh state `s`, writing the output at `out` when
    /// given: the output length and the input bytes up to the end of the stream, or a code.
    fn inflate_pass(
        &mut self,
        s: u32,
        b: u32,
        from: u32,
        max: u64,
        out: Option<u32>,
    ) -> Result<(u64, u32), i64> {
        let (n, p) = self.data(b);
        let (mut pos, mut total) = (from, 0u64);
        loop {
            let room = (max - total.min(max)).saturating_add(1);
            let st = self.inflate_step(s, p + pos, n - pos, room);
            pos += st.used;
            if total + u64::from(st.len) > max {
                return Err(OUTPUT_LIMIT);
            }
            if let Some(o) = out {
                self.heap.mem.copy(o + total as u32, st.at, st.len);
            }
            total += u64::from(st.len);
            self.slide(s, st.len);
            match status_code(st.status) {
                None if st.status == TINFLStatus::Done => return Ok((total, pos - from)),
                None => {}
                Some(code) => return Err(code),
            }
        }
    }

    /// One step of the state `s` over the `len` input bytes at `input` (at most `PIECE` of
    /// them; none finishes the stream), writing at most `room` bytes into the window.
    fn inflate_step(&mut self, s: u32, input: u32, len: u32, room: u64) -> Step {
        let base = s + 12;
        let hist = self.heap.mem.u32(base + HIST);
        let k = len.min(PIECE);
        let mut piece = [0u8; PIECE as usize];
        piece[..k as usize].copy_from_slice(self.heap.mem.bytes(input, k));
        let mut d = self.load_decompressor(base);
        let space = room.min(u64::from(WINDOW - hist)) as u32;
        let more = if len > 0 {
            TINFL_FLAG_HAS_MORE_INPUT
        } else {
            0
        };
        let (status, used, written) = decompress(
            &mut d,
            &piece[..k as usize],
            self.heap.mem.bytes_mut(base + WIN, hist + space),
            hist as usize,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF | more,
        );
        self.store_decompressor(base, d);
        Step {
            used: used as u32,
            at: base + WIN + hist,
            len: written as u32,
            status,
        }
    }

    /// After a step that wrote `len` bytes: the last `HISTORY` of them and what preceded them
    /// move to the start of the window.
    fn slide(&mut self, s: u32, len: u32) {
        let base = s + 12;
        let end = self.heap.mem.u32(base + HIST) + len;
        let keep = end.min(HISTORY);
        self.heap
            .mem
            .copy(base + WIN, base + WIN + end - keep, keep);
        self.heap.mem.set_u32(base + HIST, keep);
    }

    /// `compress_inflater()`: a fresh raw DEFLATE decoding state.
    pub fn compress_inflater(&mut self) -> Res<u32> {
        let s = self.new_bytes(STATE_SIZE)?;
        self.reset_state(s);
        Ok(s)
    }

    fn reset_state(&mut self, s: u32) {
        let base = s + 12;
        self.heap.mem.set_u32(base + HIST, 0);
        self.store_decompressor(base, DecompressorOxide::new());
    }

    /// `compress_inflate_push(st, b, at, max)`: one step over `b[at..]` (nothing: finishes the
    /// stream), at most `max` bytes of output: `Ok(output)` or `Err(code)`; the input taken and
    /// whether the stream ended are recorded in the state.
    pub fn compress_inflate_push(&mut self, s: u32, b: u32, at: i64, max: i64) -> Res<u32> {
        let (n, p) = self.data(b);
        let from = at.clamp(0, i64::from(n)) as u32;
        let max = max.max(0) as u64;
        let st = self.inflate_step(s, p + from, n - from, max.saturating_add(1));
        let done = st.status == TINFLStatus::Done;
        self.heap.mem.set_u32(s + 12 + USED, st.used);
        self.heap.mem.set_u32(s + 12 + DONE, u32::from(done));
        let res = match status_code(st.status) {
            _ if u64::from(st.len) > max => self.new_int(OUTPUT_LIMIT).and_then(|e| self.err(e)),
            Some(code) => self.new_int(code).and_then(|e| self.err(e)),
            None => self.new_bytes(st.len).and_then(|o| {
                self.heap.mem.copy(o + 12, st.at, st.len);
                self.ok(o)
            }),
        };
        self.slide(s, st.len);
        self.drop2(s, b);
        res
    }

    /// `compress_inflate_used(st)`: the input bytes the last push took.
    pub fn compress_inflate_used(&mut self, s: u32, keep: u32) -> i64 {
        let used = self.heap.mem.u32(s + 12 + USED);
        self.drop_arg(keep, 0, s);
        i64::from(used)
    }

    /// `compress_inflate_done(st)`: whether the DEFLATE stream has ended.
    pub fn compress_inflate_done(&mut self, s: u32, keep: u32) -> bool {
        let done = self.heap.mem.u32(s + 12 + DONE) == 1;
        self.drop_arg(keep, 0, s);
        done
    }

    fn load_decompressor(&self, base: u32) -> DecompressorOxide {
        let size = size_of::<DecompressorOxide>() as u32;
        let b = self.heap.mem.bytes(base + DECOMP, size);
        // SAFETY: the bytes hold a valid `DecompressorOxide`: `reset_state` wrote one and only
        // `store_decompressor` writes them after (`Inflater` is opaque to Sakura code); the
        // range is `size_of::<DecompressorOxide>()` bytes, read without an alignment requirement.
        unsafe { b.as_ptr().cast::<DecompressorOxide>().read_unaligned() }
    }

    fn store_decompressor(&mut self, base: u32, d: DecompressorOxide) {
        let size = size_of::<DecompressorOxide>() as u32;
        let b = self.heap.mem.bytes_mut(base + DECOMP, size);
        // SAFETY: the range is `size_of::<DecompressorOxide>()` writable bytes, written without
        // an alignment requirement; a `DecompressorOxide` owns no memory, so overwriting drops
        // nothing.
        unsafe {
            b.as_mut_ptr()
                .cast::<DecompressorOxide>()
                .write_unaligned(d)
        }
    }

    /// A `Bytes` holding `b`.
    fn bytes_of(&mut self, b: &[u8]) -> Res<u32> {
        let len = u32::try_from(b.len()).map_err(|_| Defect::HEAP_EXHAUSTED)?;
        let p = self.new_bytes(len)?;
        self.heap.mem.bytes_mut(p + 12, len).copy_from_slice(b);
        Ok(p)
    }
}

/// The error code of a step's status, `None` when decoding goes on or is done: an input that
/// ends inside the stream leaves the decompressor unable to go on; anything else it refuses is
/// malformed.
fn status_code(status: TINFLStatus) -> Option<i64> {
    match status {
        TINFLStatus::Done | TINFLStatus::NeedsMoreInput | TINFLStatus::HasMoreOutput => None,
        TINFLStatus::FailedCannotMakeProgress => Some(TRUNCATED),
        _ => Some(MALFORMED),
    }
}

/// Runs `f`, which allocates about `bound` bytes besides a compressor. Off wasm32 the
/// allocations are the process's.
#[cfg(not(target_arch = "wasm32"))]
fn with_arena<R>(rt: &mut Rt, _bound: usize, f: impl FnOnce(&mut Rt) -> Res<R>) -> Res<R> {
    f(rt)
}

/// Room a `CompressorOxide` allocates (its LZ code buffer, output buffer, hash chains and
/// Huffman tables, about 320 KiB on wasm32), with slack.
#[cfg(target_arch = "wasm32")]
const COMPRESSOR_ROOM: u32 = 512 * 1024;

/// Runs `f` with the arena serving every allocation from a heap block of `COMPRESSOR_ROOM +
/// bound` bytes, freed after: nothing `f` allocates outlives it.
#[cfg(target_arch = "wasm32")]
fn with_arena<R>(rt: &mut Rt, bound: usize, f: impl FnOnce(&mut Rt) -> Res<R>) -> Res<R> {
    // The block's 8-byte header comes first.
    let size = u32::try_from(bound)
        .ok()
        .and_then(|b| b.checked_add(COMPRESSOR_ROOM + 8))
        .ok_or(Defect::HEAP_EXHAUSTED)?;
    let block = rt.heap.alloc(size)?;
    arena::open(block + 8, block + size);
    let r = f(rt);
    arena::close();
    rt.heap.free(block);
    r
}

/// The wasm32 global allocator: a bump allocator over the range `with_arena` opens; outside one
/// every allocation fails, and the runtime traps (ABI §8.6). Deallocation does nothing: the
/// block goes back to the heap whole.
#[cfg(target_arch = "wasm32")]
mod arena {
    use core::alloc::{GlobalAlloc, Layout};
    use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

    static NEXT: AtomicU32 = AtomicU32::new(0);
    static END: AtomicU32 = AtomicU32::new(0);

    pub fn open(start: u32, end: u32) {
        NEXT.store(start, Relaxed);
        END.store(end, Relaxed);
    }

    pub fn close() {
        open(0, 0);
    }

    struct Arena;

    // SAFETY: `alloc` returns null or `layout.size()` bytes at `layout.align()` inside the open
    // range, which no other allocation overlaps (the bump only grows) and which the heap keeps
    // allocated until `close`; the module is single-threaded.
    unsafe impl GlobalAlloc for Arena {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let (next, end) = (NEXT.load(Relaxed), END.load(Relaxed));
            let start = next.checked_next_multiple_of(layout.align() as u32);
            let range = start.and_then(|a| Some((a, a.checked_add(layout.size() as u32)?)));
            match range {
                Some((a, e)) if next != 0 && e <= end => {
                    NEXT.store(e, Relaxed);
                    a as usize as *mut u8
                }
                _ => core::ptr::null_mut(),
            }
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    #[global_allocator]
    static ARENA: Arena = Arena;
}
