//! Source map v3 generation (one mapping per emitted line that carries a Core position).

use std::fmt::Write;

/// Line/column lookup for one source file.
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i as u32 + 1),
        );
        LineIndex { starts }
    }

    /// Zero-based (line, column in bytes).
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|s| *s <= offset) - 1;
        (line as u32, offset - self.starts[line])
    }
}

/// A mapping from a generated line (column 0) to a source position.
#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    pub gen_line: u32,
    pub gen_col: u32,
    pub source: u32,
    pub line: u32,
    pub col: u32,
}

pub fn render(file: &str, sources: &[String], mut mappings: Vec<Mapping>) -> String {
    mappings.sort_by_key(|m| (m.gen_line, m.gen_col));
    let mut out = String::new();
    let (mut line, mut prev_col, mut prev_src, mut prev_line, mut prev_scol) = (0, 0, 0, 0, 0);
    let mut first_in_line = true;
    for m in mappings {
        while line < m.gen_line {
            out.push(';');
            line += 1;
            prev_col = 0;
            first_in_line = true;
        }
        if !first_in_line {
            out.push(',');
        }
        first_in_line = false;
        vlq(&mut out, m.gen_col as i64 - prev_col);
        vlq(&mut out, m.source as i64 - prev_src);
        vlq(&mut out, m.line as i64 - prev_line);
        vlq(&mut out, m.col as i64 - prev_scol);
        prev_col = m.gen_col as i64;
        prev_src = m.source as i64;
        prev_line = m.line as i64;
        prev_scol = m.col as i64;
    }
    let srcs: Vec<String> = sources.iter().map(|s| crate::js_string(s)).collect();
    format!(
        "{{\"version\":3,\"file\":{},\"sources\":[{}],\"names\":[],\"mappings\":\"{out}\"}}",
        crate::js_string(file),
        srcs.join(",")
    )
}

fn vlq(out: &mut String, v: i64) {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut n = if v < 0 { ((-v) << 1) | 1 } else { v << 1 };
    loop {
        let mut digit = n & 31;
        n >>= 5;
        if n > 0 {
            digit |= 32;
        }
        let _ = out.write_char(B64[digit as usize] as char);
        if n == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlq_matches_the_spec_examples() {
        let mut s = String::new();
        for v in [0, 1, -1, 16, 123] {
            vlq(&mut s, v);
            s.push(' ');
        }
        assert_eq!(s, "A C D gB 2H ");
    }

    #[test]
    fn line_index() {
        let ix = LineIndex::new("ab\ncd\n");
        assert_eq!(ix.line_col(0), (0, 0));
        assert_eq!(ix.line_col(4), (1, 1));
        assert_eq!(ix.line_col(6), (2, 0));
    }
}
