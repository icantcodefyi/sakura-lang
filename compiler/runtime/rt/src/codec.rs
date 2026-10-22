//! The hot codecs of `std/bytes` and `std/encoding` (RFC 0002 §2.1): `Bytes.slice`/`concat`,
//! Base64 (RFC 4648; HTML's forgiving-base64 for `atob`) and decoding UTF-8, UTF-16LE/BE and
//! Windows-1252 with the Encoding Standard's replacement rule. Each matches the JS runtime's
//! function of the same name; `tests/vectors/` and the semantics fixtures hold the two equal.
//! Arguments are taken owned and results returned owned (ABI.md P2). ponytail: bytes are read
//! and written one at a time through `Mem`; batch through a stack buffer if a benchmark asks.

use crate::mem::Mem;
use crate::{Defect, Res, Rt, tag};

/// `Base64Error` reasons, in the order `std/bytes`'s `reasons` names them: an error is
/// `Err(offset * 4 + reason)`.
const CHARACTER: u32 = 0;
const PADDING: u32 = 1;
const LENGTH: u32 = 2;
const TRAILING: u32 = 3;

/// `std/encoding`'s `code(enc)`.
const UTF8: i64 = 0;
const UTF16BE: i64 = 2;
const WINDOWS1252: i64 = 3;

/// Windows-1252's bytes 0x80..=0x9F; every other byte is its own code point (Encoding Standard,
/// `index-windows-1252.txt`).
const W1252_HIGH: [u16; 32] = [
    0x20ac, 0x0081, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008d, 0x017d, 0x008f, 0x0090, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x009d, 0x017e, 0x0178,
];

const FFFD: char = '\u{fffd}';

/// The value of Base64 digit `c` in the standard or URL alphabet.
fn sextet(c: u8, url: bool) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' if !url => 62,
        b'/' if !url => 63,
        b'-' if url => 62,
        b'_' if url => 63,
        _ => return None,
    } as u32)
}

/// HTML's ASCII whitespace, which forgiving-base64 removes.
fn ascii_space(c: u8) -> bool {
    matches!(c, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

/// Checks Base64 `text`: the decoded length, or `offset * 4 + reason`. Strict (ECMAScript's
/// `fromBase64` with `lastChunkHandling: "strict"`): no whitespace, padding required for the
/// standard alphabet and optional for the URL one but canonical when present, zero trailing
/// bits. Lenient (forgiving-base64): whitespace skipped, standard alphabet, padding optional,
/// trailing bits ignored.
pub fn base64_check(text: &[u8], url: bool, lenient: bool) -> Result<u32, u32> {
    let err = |offset: usize, reason: u32| offset as u32 * 4 + reason;
    let (mut n, mut pads, mut pad_at) = (0u32, 0u32, None);
    let (mut last, mut last_v) = (0, 0);
    for (i, &c) in text.iter().enumerate() {
        if lenient && ascii_space(c) {
            continue;
        }
        if c == b'=' {
            pad_at.get_or_insert(i);
            pads += 1;
            continue;
        }
        if let Some(p) = pad_at {
            return Err(err(p, PADDING));
        }
        let Some(v) = sextet(c, url) else {
            return Err(err(i, CHARACTER));
        };
        n += 1;
        (last, last_v) = (i, v);
    }
    let rem = n % 4;
    if rem == 1 {
        return Err(err(text.len(), LENGTH));
    }
    match pad_at {
        Some(p) if rem == 0 || pads != 4 - rem => return Err(err(p, PADDING)),
        None if !lenient && !url && rem != 0 => return Err(err(text.len(), PADDING)),
        _ => {}
    }
    let mask = [0, 0, 0x0f, 0x03][rem as usize];
    if !lenient && last_v & mask != 0 {
        return Err(err(last, TRAILING));
    }
    Ok(n / 4 * 3 + [0, 0, 1, 2][rem as usize])
}

impl Rt {
    /// `Bytes.slice(b, start, end)`, clamped like `List.slice`.
    pub fn bytes_slice(&mut self, b: u32, start: i64, end: i64) -> Res<u32> {
        let (n, at) = self.data(b);
        let s = start.clamp(0, n as i64) as u32;
        let e = end.clamp(s as i64, n as i64) as u32;
        if s == 0 && e == n {
            return Ok(b);
        }
        let r = self
            .new_bytes(e - s)
            .inspect(|&p| self.heap.mem.copy(p + 12, at + s, e - s));
        self.drop(b);
        r
    }

    /// `Bytes.concat(parts)`: one allocation and one copy of each part, so linear in the total.
    pub fn bytes_concat(&mut self, parts: u32) -> Res<u32> {
        let k = self.list_n(parts);
        let mut total = 0u32;
        for i in 0..k {
            let (len, _) = self.data(self.at(parts, i));
            total = total.checked_add(len).ok_or(Defect::HEAP_EXHAUSTED)?;
        }
        let r = self.new_bytes(total).inspect(|&p| {
            let mut o = p + 12;
            for i in 0..k {
                let (len, at) = self.data(self.at(parts, i));
                self.heap.mem.copy(o, at, len);
                o += len;
            }
        });
        self.drop(parts);
        r
    }

    /// `Bytes.to_base64(b, alphabet, padded)` with `url`/`padded` as 0 or 1.
    pub fn bytes_to_base64(&mut self, b: u32, url: i64, padded: i64, keep: u32) -> Res<u32> {
        const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let digits = if url == 1 { URL } else { STD };
        let (n, at) = self.data(b);
        let tail = n % 3;
        let len = n / 3 * 4
            + match (tail, padded == 1) {
                (0, _) => 0,
                (_, true) => 4,
                (t, false) => t + 1,
            };
        let r = self.new_text(len).inspect(|&s| {
            let m = &mut self.heap.mem;
            let mut o = s + 16;
            for g in (0..n).step_by(3) {
                let k = (n - g).min(3);
                let mut x = [0u8; 3];
                x[..k as usize].copy_from_slice(m.bytes(at + g, k));
                let v = u32::from(x[0]) << 16 | u32::from(x[1]) << 8 | u32::from(x[2]);
                let mut q = [b'='; 4];
                for (j, d) in q.iter_mut().enumerate().take(k as usize + 1) {
                    *d = digits[(v >> (18 - 6 * j) & 63) as usize];
                }
                let w = (len - (o - s - 16)).min(4);
                m.bytes_mut(o, w).copy_from_slice(&q[..w as usize]);
                o += w;
            }
        });
        self.drop_arg(keep, 0, b);
        r
    }

    /// `Bytes.from_base64(text, alphabet)` (strict): `Ok(bytes)` or `Err(offset * 4 + reason)`.
    pub fn bytes_from_base64(&mut self, text: u32, url: i64, keep: u32) -> Res<u32> {
        self.base64_decode(text, url == 1, false, keep)
    }

    /// `Bytes.from_base64_lenient(text)` (forgiving-base64, `atob`).
    pub fn bytes_from_base64_lenient(&mut self, text: u32, keep: u32) -> Res<u32> {
        self.base64_decode(text, false, true, keep)
    }

    fn base64_decode(&mut self, text: u32, url: bool, lenient: bool, keep: u32) -> Res<u32> {
        let (n, at) = self.data(text);
        let checked = base64_check(self.heap.mem.bytes(at, n), url, lenient);
        let r = match checked {
            Err(code) => self.new_int(i64::from(code)).and_then(|e| self.err(e)),
            Ok(len) => self.new_bytes(len).and_then(|p| {
                let m = &mut self.heap.mem;
                let (mut acc, mut bits, mut o) = (0u32, 0, p + 12);
                for i in 0..n {
                    let c = m.u8(at + i);
                    if c == b'=' {
                        break;
                    }
                    let Some(v) = sextet(c, url) else { continue };
                    acc = acc << 6 | v;
                    bits += 6;
                    if bits >= 8 {
                        bits -= 8;
                        m.set_u8(o, (acc >> bits) as u8);
                        o += 1;
                        acc &= (1 << bits) - 1;
                    }
                }
                self.ok(p)
            }),
        };
        self.drop_arg(keep, 0, text);
        r
    }

    /// `std/encoding`'s decoder: `b` in encoding `enc` (`code(enc)`) as `Ok(text)`, malformed
    /// sequences replaced by U+FFFD unless `fatal` is 1, when the result is `Err(offset)` of the
    /// first one. A byte order mark is text here; `std/encoding` strips it.
    pub fn encoding_decode(&mut self, b: u32, enc: i64, fatal: i64) -> Res<u32> {
        let (n, at) = self.data(b);
        let (len, bad) = self.decode_into(at, n, enc, None);
        let r = match bad {
            Some(offset) if fatal == 1 => self.new_int(i64::from(offset)).and_then(|e| self.err(e)),
            _ => self.new_text(len).and_then(|s| {
                self.decode_into(at, n, enc, Some(s + 16));
                self.ok(s)
            }),
        };
        self.drop(b);
        r
    }

    /// Decodes `n` bytes at `at`, writing the UTF-8 text to `out` when given: its length and
    /// the offset of the first malformed sequence.
    fn decode_into(&mut self, at: u32, n: u32, enc: i64, out: Option<u32>) -> (u32, Option<u32>) {
        let m = &mut self.heap.mem;
        let (mut len, mut bad) = (0u32, None);
        let put = |m: &mut Mem, len: &mut u32, c: char| {
            let mut buf = [0; 4];
            let e = c.encode_utf8(&mut buf).as_bytes();
            if let Some(o) = out {
                m.bytes_mut(o + *len, e.len() as u32).copy_from_slice(e);
            }
            *len += e.len() as u32;
        };
        match enc {
            UTF8 => {
                let mut i = 0;
                while i < n {
                    let (valid, invalid) = {
                        let chunk = m.bytes(at + i, n - i).utf8_chunks().next();
                        chunk.map_or((0, 0), |c| {
                            (c.valid().len() as u32, c.invalid().len() as u32)
                        })
                    };
                    if let Some(o) = out {
                        m.copy(o + len, at + i, valid);
                    }
                    len += valid;
                    i += valid;
                    if invalid > 0 {
                        bad.get_or_insert(i);
                        put(m, &mut len, FFFD);
                        i += invalid;
                    }
                }
            }
            WINDOWS1252 => {
                for i in 0..n {
                    let x = m.u8(at + i);
                    let c = match x {
                        0x80..=0x9f => W1252_HIGH[x as usize - 0x80],
                        _ => u16::from(x),
                    };
                    put(m, &mut len, char::from_u32(u32::from(c)).unwrap_or(FFFD));
                }
            }
            _ => {
                let be = enc == UTF16BE;
                let unit = |m: &Mem, k: u32| {
                    let (a, b) = (m.u8(at + 2 * k), m.u8(at + 2 * k + 1));
                    u32::from(if be {
                        u16::from_be_bytes([a, b])
                    } else {
                        u16::from_le_bytes([a, b])
                    })
                };
                let units = n / 2;
                let mut k = 0;
                while k < units {
                    let u = unit(m, k);
                    let c = if (0xd800..0xdc00).contains(&u)
                        && k + 1 < units
                        && (0xdc00..0xe000).contains(&unit(m, k + 1))
                    {
                        k += 1;
                        0x10000 + ((u - 0xd800) << 10) + (unit(m, k) - 0xdc00)
                    } else {
                        if (0xd800..0xe000).contains(&u) {
                            bad.get_or_insert(2 * k);
                        }
                        u
                    };
                    put(m, &mut len, char::from_u32(c).unwrap_or(FFFD));
                    k += 1;
                }
                if n % 2 == 1 {
                    bad.get_or_insert(n - 1);
                    put(m, &mut len, FFFD);
                }
            }
        }
        (len, bad)
    }

    /// `Encoding.encode(s, enc)` for UTF-16LE (`enc` 1) and UTF-16BE (2).
    pub fn encoding_encode(&mut self, s: u32, enc: i64) -> Res<u32> {
        let (n, at) = self.data(s);
        let units: u32 = self
            .text_at(at, n)
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        let r = self.new_bytes(units * 2).inspect(|&p| {
            let mut o = p + 12;
            let mut i = 0;
            while i < n {
                let c = self
                    .text_at(at + i, (n - i).min(4))
                    .chars()
                    .next()
                    .unwrap_or(FFFD);
                i += c.len_utf8() as u32;
                let mut buf = [0u16; 2];
                for &u in c.encode_utf16(&mut buf).iter() {
                    let b = if enc == UTF16BE {
                        u.to_be_bytes()
                    } else {
                        u.to_le_bytes()
                    };
                    self.heap.mem.bytes_mut(o, 2).copy_from_slice(&b);
                    o += 2;
                }
            }
        });
        self.drop(s);
        r
    }

    /// The UTF-8 prefix of the `n` bytes at `at` that ends on a character boundary.
    fn text_at(&self, at: u32, n: u32) -> &str {
        let b = self.heap.mem.bytes(at, n);
        match core::str::from_utf8(b) {
            Ok(t) => t,
            Err(e) => core::str::from_utf8(&b[..e.valid_up_to()]).unwrap_or(""),
        }
    }

    /// A `String` of `len` bytes for the caller to fill.
    fn new_text(&mut self, len: u32) -> Res<u32> {
        let size = len.checked_add(16).ok_or(Defect::HEAP_EXHAUSTED)?;
        let s = self.new_object(tag::STRING, size)?;
        self.heap.mem.set_u32(s + 8, len);
        Ok(s)
    }
}
