//! `compress.rs`: raw DEFLATE, CRC-32 and Adler-32 on the checked-in files of
//! `compiler/tests/vectors/compress` (gzip 487.40.2 `-9`, Python's `zlib` at level 9, and a gzip
//! header with every optional field around the raw stream), which
//! `tests/semantics/decompress_known_gzip_bytes.sak` embeds as Base64; the last test is that
//! fixture's staleness gate. Every test drops what it built and checks the live-object count.

mod common;

use common::{Known, Ty, layout_bytes};
use sakura_rt::Rt;
use sakura_rt::compress::{MALFORMED, OUTPUT_LIMIT, TRUNCATED, adler32, crc32};
use sakura_rt::layout::WORD;

const PLAIN: &[u8] = include_bytes!("../../../tests/vectors/compress/blossom.txt");
const GZ: &[u8] = include_bytes!("../../../tests/vectors/compress/blossom.txt.gz");
const ZZ: &[u8] = include_bytes!("../../../tests/vectors/compress/blossom.zz");
const RAW: &[u8] = include_bytes!("../../../tests/vectors/compress/blossom.deflate");
const FIELDS: &[u8] = include_bytes!("../../../tests/vectors/compress/blossom-fields.gz");

fn rt() -> Rt {
    let mut rt = Rt::new(1 << 26).unwrap();
    let types = vec![
        Ty::Variant("Option", &[("None", &[]), ("Some", &[WORD])]),
        Ty::Variant("Result", &[("Ok", &[WORD]), ("Err", &[WORD])]),
        Ty::Tuple(&[WORD, WORD]),
    ];
    let known = Known {
        option: Some(0),
        result: Some(1),
        pair: Some(2),
        int_pair: None,
    };
    let layout = rt.install(&layout_bytes(&types, known)).unwrap();
    rt.set_layout(layout).unwrap();
    rt
}

fn bytes(rt: &mut Rt, b: &[u8]) -> u32 {
    let p = rt.new_bytes(b.len() as u32).unwrap();
    rt.heap
        .mem
        .bytes_mut(p + 12, b.len() as u32)
        .copy_from_slice(b);
    p
}

/// `Ok(x)` as `Ok(x)` (borrowed word) or `Err(code)`; `r` is dropped by the caller.
fn result(rt: &Rt, r: u32) -> Result<u32, i64> {
    let payload = rt.slot(r, 0) as u32;
    match rt.heap.mem.u32(r + 12) {
        0 => Ok(payload),
        _ => Err(rt.int_of(payload)),
    }
}

/// The raw DEFLATE stream inside a gzip member (RFC 1952 §2.3), past its optional fields.
fn gzip_body(b: &[u8]) -> &[u8] {
    let flags = b[3];
    let mut at = 10;
    if flags & 4 != 0 {
        at += 2 + usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            at += b[at..].iter().position(|&x| x == 0).unwrap() + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    &b[at..]
}

/// Whole-buffer decoding: `(output, rest)` or the error code.
fn inflate(rt: &mut Rt, input: &[u8], max: i64) -> Result<(Vec<u8>, Vec<u8>), i64> {
    let b = bytes(rt, input);
    let r = rt.compress_inflate(b, 0, max, 0).unwrap();
    let out = result(rt, r).map(|pair| {
        let (o, rest) = (rt.slot(pair, 0) as u32, rt.slot(pair, 1) as u32);
        (rt.data_bytes(o).to_vec(), rt.data_bytes(rest).to_vec())
    });
    rt.drop(r);
    out
}

/// Decoding through a state as `std/compress`'s view drives it: `piece` input bytes per item,
/// an item pushed until it is used up, and empty pushes (finishing) only after the last.
fn stream(rt: &mut Rt, input: &[u8], piece: usize, max: i64) -> Result<(Vec<u8>, usize), i64> {
    let st = rt.compress_inflater().unwrap();
    let (mut out, mut taken) = (Vec::new(), 0usize);
    let pieces: Vec<&[u8]> = input.chunks(piece).collect();
    let (mut i, mut at, mut done) = (0, 0, false);
    while !done {
        let item = pieces.get(i).copied().unwrap_or(&[]);
        if at == item.len() && i < pieces.len() {
            (i, at) = (i + 1, 0);
            continue;
        }
        let b = bytes(rt, item);
        rt.dup(st).unwrap();
        let left = max - out.len() as i64;
        let r = rt.compress_inflate_push(st, b, at as i64, left).unwrap();
        let got = result(rt, r).map(|o| rt.data_bytes(o).to_vec());
        rt.drop(r);
        let got = got.inspect_err(|_| rt.drop(st))?;
        out.extend(got.iter());
        rt.dup(st).unwrap();
        let used = rt.compress_inflate_used(st, 0) as usize;
        rt.dup(st).unwrap();
        done = rt.compress_inflate_done(st, 0);
        at += used;
        taken += used;
    }
    rt.drop(st);
    Ok((out, taken))
}

#[test]
fn the_checked_in_files_decode_to_their_text() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    let gz = gzip_body(GZ);
    let fields = gzip_body(FIELDS);
    for (body, trailer) in [
        (gz, &GZ[GZ.len() - 8..]),
        (fields, &FIELDS[FIELDS.len() - 8..]),
        (&ZZ[2..], &ZZ[ZZ.len() - 4..]),
        (RAW, &[][..]),
    ] {
        let (out, rest) = inflate(&mut rt, body, PLAIN.len() as i64).unwrap();
        assert_eq!(out, PLAIN);
        assert_eq!(rest, trailer);
        for piece in [1, 7, 4096] {
            let (out, taken) = stream(&mut rt, body, piece, PLAIN.len() as i64).unwrap();
            assert_eq!(out, PLAIN, "piece {piece}");
            assert_eq!(taken, body.len() - trailer.len(), "piece {piece}");
        }
    }
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn checksums_match_the_trailers() {
    let crc = u32::from_le_bytes(GZ[GZ.len() - 8..GZ.len() - 4].try_into().unwrap());
    assert_eq!(crc32(0, PLAIN), crc);
    let (a, b) = PLAIN.split_at(1000);
    assert_eq!(crc32(crc32(0, a), b), crc);
    let adler = u32::from_be_bytes(ZZ[ZZ.len() - 4..].try_into().unwrap());
    assert_eq!(adler32(1, PLAIN), adler);
    assert_eq!(adler32(adler32(1, a), b), adler);
    assert_eq!(crc32(0, b"123456789"), 0xcbf4_3926);
}

#[test]
fn the_limit_counts_output_bytes() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    let n = PLAIN.len() as i64;
    assert!(inflate(&mut rt, RAW, n).is_ok());
    assert_eq!(inflate(&mut rt, RAW, n - 1), Err(OUTPUT_LIMIT));
    assert_eq!(inflate(&mut rt, RAW, 0), Err(OUTPUT_LIMIT));
    assert!(stream(&mut rt, RAW, 7, n).is_ok());
    assert_eq!(stream(&mut rt, RAW, 7, n - 1), Err(OUTPUT_LIMIT));
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn truncated_and_malformed_streams_are_told_apart() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    for cut in [0, 1, RAW.len() / 2, RAW.len() - 1] {
        assert_eq!(
            inflate(&mut rt, &RAW[..cut], 1 << 20),
            Err(TRUNCATED),
            "cut {cut}"
        );
        assert_eq!(
            stream(&mut rt, &RAW[..cut], 5, 1 << 20),
            Err(TRUNCATED),
            "cut {cut}"
        );
    }
    // Block type 3 is reserved (RFC 1951 §3.2.3).
    assert_eq!(inflate(&mut rt, &[0xff, 0xff], 1 << 20), Err(MALFORMED));
    assert_eq!(stream(&mut rt, &[0xff, 0xff], 1, 1 << 20), Err(MALFORMED));
    // A fixed block whose first symbol copies from distance 1, before any output: zlib's
    // "invalid distance too far back", never bytes of a zeroed history.
    assert_eq!(inflate(&mut rt, &[3, 2, 0], 1 << 20), Err(MALFORMED));
    assert_eq!(stream(&mut rt, &[3, 2, 0], 1, 1 << 20), Err(MALFORMED));
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn every_level_round_trips_and_segments_concatenate() {
    let mut rt = rt();
    let base = rt.stats().live_objects;
    for level in 0..=9 {
        let b = bytes(&mut rt, PLAIN);
        let z = rt.compress_deflate(b, level, 1, 0).unwrap();
        let packed = rt.data_bytes(z).to_vec();
        rt.drop(z);
        assert_eq!(
            inflate(&mut rt, &packed, 1 << 20).unwrap().0,
            PLAIN,
            "level {level}"
        );
        // `compress_source`'s shape: sync-flushed pieces, then an empty final block.
        let mut joined = Vec::new();
        for piece in PLAIN.chunks(500) {
            let b = bytes(&mut rt, piece);
            let z = rt.compress_deflate(b, level, 0, 0).unwrap();
            joined.extend_from_slice(rt.data_bytes(z));
            rt.drop(z);
        }
        joined.extend_from_slice(&[0x03, 0x00]);
        let (out, rest) = inflate(&mut rt, &joined, 1 << 20).unwrap();
        assert_eq!((out.as_slice(), rest.len()), (PLAIN, 0), "level {level}");
    }
    let b = bytes(&mut rt, &[]);
    let z = rt.compress_deflate(b, 6, 1, 0).unwrap();
    let packed = rt.data_bytes(z).to_vec();
    rt.drop(z);
    assert_eq!(inflate(&mut rt, &packed, 0).unwrap().0, b"");
    assert_eq!(rt.stats().live_objects, base);
}

#[test]
fn the_known_bytes_fixture_embeds_these_files() {
    let fixture = include_str!("../../../tests/semantics/decompress_known_gzip_bytes.sak");
    for file in [GZ, ZZ, RAW, FIELDS] {
        assert!(
            fixture.contains(&base64(file)),
            "regenerate the fixture's Base64"
        );
    }
    let text = std::str::from_utf8(PLAIN).unwrap();
    let stdout = include_str!("../../../tests/semantics/decompress_known_gzip_bytes.stdout");
    assert!(stdout.contains(text.lines().next().unwrap()));
}

fn base64(b: &[u8]) -> String {
    const D: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for g in b.chunks(3) {
        let v = g.iter().fold(0u32, |v, &x| v << 8 | u32::from(x)) << (8 * (3 - g.len()));
        for j in 0..4 {
            s.push(if j <= g.len() {
                D[(v >> (18 - 6 * j) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}
