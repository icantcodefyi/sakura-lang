//! Derived `Show` (design §4.8): strings are raw at the top level of an interpolation and quoted
//! inside structures; floats use the shortest round-trip form, printed like JS `Number#toString`
//! but always with a `.` or exponent (`2.0`). Containers render as Vow syntax (`[1, 2]`,
//! `Map.of([("a", 1)])`, `Set.of([1])`), following std's proposal in `compiler/REQUESTS.md`.

use vow_core::{Module, TypeKind};

use crate::Value;

pub fn show(module: &Module, v: &Value, quoted: bool) -> String {
    let mut out = String::new();
    write(module, v, quoted, &mut out);
    out
}

fn list(module: &Module, xs: &[Value], open: &str, close: &str, out: &mut String) {
    out.push_str(open);
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write(module, x, true, out);
    }
    out.push_str(close);
}

fn write(module: &Module, v: &Value, quoted: bool, out: &mut String) {
    match v {
        Value::Unit => out.push_str("()"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            let s = js_number(*f);
            let plain = s.bytes().all(|b| b.is_ascii_digit() || b == b'-');
            out.push_str(&s);
            if plain {
                out.push_str(".0");
            }
        }
        Value::Str(s) if quoted => out.push_str(&format!("{:?}", &**s)),
        Value::Str(s) => out.push_str(s),
        Value::Tuple(xs) => list(module, xs, "(", ")", out),
        Value::List(xs) => list(module, xs, "[", "]", out),
        Value::Set(xs) => list(module, xs, "Set.of([", "])", out),
        Value::Map(entries) => {
            out.push_str("Map.of([");
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push('(');
                write(module, k, true, out);
                out.push_str(", ");
                write(module, v, true, out);
                out.push(')');
            }
            out.push_str("])");
        }
        Value::Record(t, fields) => {
            let def = module.ty(*t);
            out.push_str(&def.name);
            if !fields.is_empty() {
                out.push_str(" { ");
                for (i, (f, v)) in def.fields().iter().zip(fields.iter()).enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&f.name);
                    out.push_str(": ");
                    write(module, v, true, out);
                }
                out.push_str(" }");
            }
        }
        Value::Variant(t, c, payload) => {
            let name = match &module.ty(*t).kind {
                TypeKind::Variant { ctors } => ctors.get(*c as usize).map(|c| c.name.as_str()),
                TypeKind::Record { .. } => None,
            };
            out.push_str(name.unwrap_or("?"));
            if !payload.is_empty() {
                list(module, payload, "(", ")", out);
            }
        }
        Value::Fn(_) => out.push_str("<fn>"),
        Value::Handler(_) | Value::Instance(_) => out.push_str("<handler>"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn containers_render_as_vow_syntax() {
        let m = Module::default();
        let s = |x: &str| Value::Str(x.into());
        let map = Value::Map(Rc::new(vec![(s("a"), Value::Float(2.0))]));
        assert_eq!(show(&m, &map, false), "Map.of([(\"a\", 2.0)])");
        let set = Value::Set(Rc::new(vec![Value::Int(1)]));
        assert_eq!(show(&m, &set, false), "Set.of([1])");
        let tup = Value::Tuple(Rc::new([Value::Float(-0.5), s("x"), Value::Unit]));
        assert_eq!(show(&m, &tup, false), "(-0.5, \"x\", ())");
        assert_eq!(show(&m, &s("raw"), false), "raw");
    }

    #[test]
    fn floats_print_like_js() {
        for (f, s) in [
            (1.0, "1"),
            (0.1, "0.1"),
            (-2.5, "-2.5"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (123.456, "123.456"),
            (0.1 + 0.2, "0.30000000000000004"),
            (f64::NAN, "NaN"),
            (-0.0, "0"),
        ] {
            assert_eq!(js_number(f), s, "{f}");
        }
    }
}
