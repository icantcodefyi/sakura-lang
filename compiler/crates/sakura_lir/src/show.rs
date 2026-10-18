//! Number formatting for derived `Show`, identical to `$rt.showFloat` in the JS runtime.
//!
//! Copied from `tools/sakura_eval/src/show.rs` (which `sakura_lir` may not depend on before the
//! M3 gates, plan §3.5); L3 moves the interpreter into `sakura_eval` and the copy goes away.

/// `$rt.showFloat`: JS number formatting, always with a `.` in the mantissa.
pub fn show_float(f: f64) -> String {
    if f == 0.0 && f.is_sign_negative() {
        return "-0.0".into();
    }
    let s = js_number(f);
    if f.is_nan() || f.is_infinite() {
        return s;
    }
    match s.split_once('e') {
        None if s.contains('.') => s,
        None => format!("{s}.0"),
        Some((m, e)) => {
            let m = if m.contains('.') {
                m.to_owned()
            } else {
                format!("{m}.0")
            };
            format!("{m}e{}", e.replace('+', ""))
        }
    }
}

/// JS `Number.prototype.toString()` for a finite or special float (ECMA-262 Number::toString).
pub fn js_number(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if f == 0.0 {
        return "0".into();
    }
    // `{:e}` gives the shortest round-trip digits: `-1.2345e3`.
    let sci = format!("{:e}", f.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap() + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let rest = if k > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        let e = n - 1;
        format!(
            "{}{rest}e{}{}",
            &digits[..1],
            if e < 0 { "-" } else { "+" },
            e.abs()
        )
    };
    if f < 0.0 { format!("-{body}") } else { body }
}

/// JS `Number.prototype.toFixed(digits)` as the runtime's `float_format` uses it: non-finite
/// values show as `show_float`, magnitudes from 10^21 as `js_number`, and an exact tie rounds
/// away from zero (ECMA-262 picks the larger `n`), unlike Rust's ties-to-even formatting.
pub fn to_fixed(f: f64, digits: usize) -> String {
    if !f.is_finite() {
        return show_float(f);
    }
    if f.abs() >= 1e21 {
        return js_number(f);
    }
    // A double's exact decimal expansion has at most 1074 fractional digits.
    let exact = format!("{:.1100}", f.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits_v: Vec<u8> = int
        .bytes()
        .chain(frac.bytes().take(digits))
        .map(|b| b - b'0')
        .collect();
    let round_up = frac.as_bytes().get(digits).is_some_and(|&b| b >= b'5');
    if round_up {
        let mut i = digits_v.len();
        loop {
            if i == 0 {
                digits_v.insert(0, 1);
                break;
            }
            i -= 1;
            if digits_v[i] == 9 {
                digits_v[i] = 0;
            } else {
                digits_v[i] += 1;
                break;
            }
        }
    }
    let n_int = digits_v.len() - digits;
    let mut out = String::new();
    if f < 0.0 {
        out.push('-');
    }
    out.extend(digits_v[..n_int].iter().map(|d| char::from(b'0' + d)));
    if digits > 0 {
        out.push('.');
        out.extend(digits_v[n_int..].iter().map(|d| char::from(b'0' + d)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_like_js() {
        for (f, s) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e21, "1.0e21"),
            (1.5e-7, "1.5e-7"),
            (-0.0, "-0.0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(show_float(f), s, "{f}");
        }
    }

    #[test]
    fn to_fixed_matches_js() {
        for (f, d, s) in [
            (4.56789, 2, "4.57"),
            (0.5, 0, "1"),
            (2.5, 0, "3"),
            (-2.5, 0, "-3"),
            (1.005, 2, "1.00"),
            (9.995, 2, "9.99"),
            (99.5, 0, "100"),
            (0.0, 3, "0.000"),
            (-0.0001, 2, "-0.00"),
            (1e21, 2, "1e+21"),
        ] {
            assert_eq!(to_fixed(f, d), s, "{f} {d}");
        }
    }
}
