//! Builders for the tables `sakura_lir` will emit (src/layout.rs documents the format), so the
//! host tests can stand in for a compiled program's data segment.
#![allow(dead_code)]

use sakura_rt::Rt;
use sakura_rt::heap::{CLASSES, RUN_PAGE};
use sakura_rt::layout::{I64, NONE_ID, WORD};

/// A type of the test program, in layout-table order (the index is the `type_id`).
pub enum Ty<'a> {
    Record(&'a str, &'a [(&'a str, u32)]),
    Error(&'a str, &'a [(&'a str, u32)]),
    Tuple(&'a [u32]),
    Variant(&'a str, &'a [(&'a str, &'a [u32])]),
}

#[derive(Default)]
struct Buf(Vec<u8>);

impl Buf {
    fn at(&self) -> u32 {
        self.0.len() as u32
    }
    fn u32(&mut self, v: u32) -> u32 {
        let at = self.at();
        self.0.extend_from_slice(&v.to_le_bytes());
        at
    }
    fn patch(&mut self, at: u32, v: u32) {
        self.0[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn name(&mut self, s: &str) -> u32 {
        let at = self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
        at
    }
    fn kinds(&mut self, kinds: &[u32]) -> u32 {
        if kinds.is_empty() {
            return 0;
        }
        let at = self.at();
        for chunk in kinds.chunks(16) {
            let mut w = 0;
            for (i, k) in chunk.iter().enumerate() {
                w |= k << (2 * i);
            }
            self.u32(w);
        }
        at
    }
    fn row(&mut self, name: Option<&str>, fields: &[(&str, u32)]) -> u32 {
        let name = name.map_or(0, |n| self.name(n));
        let kinds: Vec<u32> = fields.iter().map(|f| f.1).collect();
        let kinds_at = self.kinds(&kinds);
        let field_names = if fields.iter().any(|f| !f.0.is_empty()) {
            let offs: Vec<u32> = fields.iter().map(|f| self.name(f.0)).collect();
            let at = self.at();
            for o in offs {
                self.u32(o);
            }
            at
        } else {
            0
        };
        let n = fields.len() as u32;
        let at = self.u32(16 + 8 * n);
        self.u32(n);
        let words = n.div_ceil(32).max(1);
        for w in 0..words {
            let mut bits = 0u32;
            for (i, k) in kinds.iter().enumerate().skip(32 * w as usize).take(32) {
                if *k == WORD {
                    bits |= 1 << (i % 32);
                }
            }
            self.u32(bits);
        }
        self.u32(kinds_at);
        self.u32(name);
        self.u32(field_names);
        at
    }
}

/// Well-known ids of the table header, by type index (`None` = absent).
#[derive(Default, Clone, Copy)]
pub struct Known {
    pub option: Option<u32>,
    pub result: Option<u32>,
    pub pair: Option<u32>,
    pub int_pair: Option<u32>,
}

pub fn layout_bytes(types: &[Ty], known: Known) -> Vec<u8> {
    let mut b = Buf::default();
    b.u32(types.len() as u32);
    for id in [known.option, known.result, known.pair, known.int_pair] {
        b.u32(id.unwrap_or(NONE_ID));
    }
    let dir = b.at();
    for _ in types {
        b.u32(0);
    }
    for (i, ty) in types.iter().enumerate() {
        let unnamed: Vec<(&str, u32)>;
        let (kind, name, rows): (u32, Option<&str>, Vec<u32>) = match ty {
            Ty::Record(n, fs) => (0, Some(*n), vec![b.row(Some(n), fs)]),
            Ty::Error(n, fs) => (1, Some(*n), vec![b.row(Some(n), fs)]),
            Ty::Tuple(ks) => {
                unnamed = ks.iter().map(|k| ("", *k)).collect();
                (2, None, vec![b.row(None, &unnamed)])
            }
            Ty::Variant(n, cs) => {
                let rows = cs
                    .iter()
                    .map(|(c, ks)| {
                        let fs: Vec<(&str, u32)> = ks.iter().map(|k| ("", *k)).collect();
                        b.row(Some(c), &fs)
                    })
                    .collect();
                (3, Some(*n), rows)
            }
        };
        let name = name.map_or(0, |n| b.name(n));
        let at = b.u32(kind);
        b.u32(name);
        b.u32(rows.len() as u32);
        for r in rows {
            b.u32(r);
        }
        b.patch(dir + 4 * i as u32, at);
    }
    b.0
}

/// A code descriptor: name, frame slot kinds, capture kinds.
pub struct CodeDesc<'a>(pub &'a str, pub &'a [u32], pub &'a [u32]);

pub fn codes_bytes(codes: &[Option<CodeDesc>]) -> Vec<u8> {
    let mut b = Buf::default();
    b.u32(codes.len() as u32);
    let dir = b.at();
    for _ in codes {
        b.u32(0);
    }
    for (i, c) in codes.iter().enumerate() {
        let Some(CodeDesc(name, frame, captures)) = c else {
            continue;
        };
        let name = b.name(name);
        let fk = b.kinds(frame);
        let ck = b.kinds(captures);
        let at = b.u32(name);
        b.u32(frame.len() as u32);
        b.u32(fk);
        b.u32(captures.len() as u32);
        b.u32(ck);
        b.patch(dir + 4 * i as u32, at);
    }
    b.0
}

/// A runtime with the given program types installed.
pub fn rt_with(types: &[Ty], known: Known) -> Rt {
    let mut rt = Rt::new(64 << 20).unwrap();
    let table = rt.install(&layout_bytes(types, known)).unwrap();
    rt.set_layout(table).unwrap();
    rt
}

/// The list-of-`Int` record used by the destruction tests: `Cons { head: Int, tail: word }`.
pub const CONS: &[Ty] = &[Ty::Record("Cons", &[("head", I64), ("tail", WORD)])];

/// Run the heap out: every page to the limit, the current chunk and every free list taken,
/// then one block of each size in `room` handed back, so the next allocations are served by
/// those alone and fail with `D096` past them. Free the returned blocks to undo it.
pub fn exhaust(rt: &mut Rt, room: &[u32]) -> Vec<u32> {
    let held: Vec<u32> = room.iter().map(|&s| rt.heap.alloc(s).unwrap()).collect();
    let mut fill = vec![];
    for size in [RUN_PAGE + 1].into_iter().chain(CLASSES) {
        while let Ok(p) = rt.heap.alloc(size) {
            fill.push(p);
        }
    }
    for p in held {
        rt.heap.free(p);
    }
    fill
}
