//! The runtime functions of `vow_core::builtins` that the evaluator implements. Anything else
//! stops the run with `Stop::Unsupported`.

use std::rc::Rc;

use vow_core::Module;

use crate::{Called, Handlers, Machine, R, Stop, Value, compare, defect, equal, int};

pub(crate) fn option(module: &Module, v: Option<Value>) -> R<Value> {
    let t = module
        .builtin_type("Option")
        .ok_or_else(|| Stop::Unsupported("Option is not in this module".into()))?;
    Ok(match v {
        None => Value::Variant(t, 0, Rc::new([])),
        Some(v) => Value::Variant(t, 1, Rc::new([v])),
    })
}

fn list(xs: Vec<Value>) -> Value {
    Value::List(Rc::new(xs))
}

fn bad(name: &str, args: &[Value]) -> Stop {
    Stop::Internal(format!("bad arguments to {name}: {args:?}"))
}

fn index(i: i64, len: usize) -> Option<usize> {
    usize::try_from(i).ok().filter(|&i| i < len)
}

/// The runtime's `requires`: a std precondition failure is a caller defect.
fn requires(cond: bool, function: &str, clause: &str) -> R<()> {
    if cond {
        Ok(())
    } else {
        Err(defect(
            "D020",
            format!("precondition of {function} violated: {clause}"),
        ))
    }
}

/// The runtime's `floatToInt`: NaN/±∞ is `D004`, beyond the safe range `D003` (or `D001`).
fn float_to_int(f: f64, what: &str) -> R<Value> {
    if !f.is_finite() {
        return Err(defect(
            "D004",
            format!("{what} of {} is not an integer", crate::show::show_float(f)),
        ));
    }
    if f.abs() >= -(i64::MIN as f64) {
        return Err(defect("D001", "integer overflow"));
    }
    int(f as i64)
}

pub(crate) fn call(m: &mut Machine, name: &str, args: Vec<Value>, hs: &Rc<Handlers>) -> R<Called> {
    use Value::*;
    let module = m.module;
    // Higher-order list builtins call back into Vow code; a failing callback fails the builtin.
    macro_rules! cb {
        ($f:expr, $($arg:expr),*) => {
            match m.call($f, vec![$($arg),*], hs)? {
                Ok(v) => v,
                Err(e) => return Ok(Err(e)),
            }
        };
    }
    let v = match (name, args.as_slice()) {
        ("list_empty", []) => list(vec![]),
        ("list_len", [List(xs)]) => Int(xs.len() as i64),
        ("list_get", [List(xs), Int(i)]) => {
            option(module, index(*i, xs.len()).map(|i| xs[i].clone()))?
        }
        ("list_push", [List(xs), x]) => {
            let mut ys = (**xs).clone();
            ys.push(x.clone());
            list(ys)
        }
        ("list_concat", [List(xs), List(ys)]) => {
            list(xs.iter().chain(ys.iter()).cloned().collect())
        }
        ("list_range", [Int(a), Int(b)]) => list((*a..*b).map(Int).collect()),
        ("list_reverse", [List(xs)]) => list(xs.iter().rev().cloned().collect()),
        ("list_join", [List(xs), Str(sep)]) => {
            let parts: Option<Vec<&str>> = xs
                .iter()
                .map(|x| match x {
                    Str(s) => Some(&**s),
                    _ => None,
                })
                .collect();
            Str(parts.ok_or_else(|| bad(name, &args))?.join(sep).into())
        }
        ("list_sum", [List(xs)]) => {
            let mut total = 0i64;
            for x in xs.iter() {
                let Int(i) = x else {
                    return Err(bad(name, &args));
                };
                total = total
                    .checked_add(*i)
                    .ok_or_else(|| defect("D001", "integer overflow in List.sum"))?;
            }
            int(total)?
        }
        ("list_at", [List(xs), Int(i)]) => index(*i, xs.len())
            .map(|i| xs[i].clone())
            .ok_or_else(|| bad(name, &args))?,
        ("list_slice", [List(xs), Int(a), Int(b)]) => {
            let n = xs.len() as i64;
            let s = (*a).clamp(0, n);
            let e = (*b).clamp(s, n);
            list(xs[s as usize..e as usize].to_vec())
        }
        ("list_unique", [List(xs)]) => {
            let mut out: Vec<Value> = Vec::new();
            for x in xs.iter() {
                if !out.iter().any(|y| equal(x, y)) {
                    out.push(x.clone());
                }
            }
            list(out)
        }
        ("list_first", [List(xs)]) => option(module, xs.first().cloned())?,
        ("list_last", [List(xs)]) => option(module, xs.last().cloned())?,
        ("list_contains", [List(xs), x]) => Bool(xs.iter().any(|y| equal(x, y))),
        ("list_sort", [List(xs)]) => {
            let mut ys = (**xs).clone();
            ys.sort_by(|a, b| compare(a, b).unwrap_or(std::cmp::Ordering::Equal));
            list(ys)
        }
        ("list_enumerate", [List(xs)]) => list(
            xs.iter()
                .enumerate()
                .map(|(i, x)| Tuple(Rc::new([Int(i as i64), x.clone()])))
                .collect(),
        ),
        ("list_zip", [List(xs), List(ys)]) => list(
            xs.iter()
                .zip(ys.iter())
                .map(|(x, y)| Tuple(Rc::new([x.clone(), y.clone()])))
                .collect(),
        ),
        ("list_map", [List(xs), f]) => {
            let mut out = Vec::with_capacity(xs.len());
            for x in xs.iter() {
                out.push(cb!(f, x.clone()));
            }
            list(out)
        }
        ("list_filter", [List(xs), f]) => {
            let mut out = Vec::new();
            for x in xs.iter() {
                if let Bool(true) = cb!(f, x.clone()) {
                    out.push(x.clone());
                }
            }
            list(out)
        }
        ("list_fold", [List(xs), init, f]) => {
            let mut acc = init.clone();
            for x in xs.iter() {
                acc = cb!(f, acc, x.clone());
            }
            acc
        }
        ("list_find", [List(xs), f]) => {
            let mut found = None;
            for x in xs.iter() {
                if let Bool(true) = cb!(f, x.clone()) {
                    found = Some(x.clone());
                    break;
                }
            }
            option(module, found)?
        }
        ("list_all", [List(xs), f]) => {
            let mut all = true;
            for x in xs.iter() {
                if let Bool(false) = cb!(f, x.clone()) {
                    all = false;
                    break;
                }
            }
            Bool(all)
        }
        ("list_any", [List(xs), f]) => {
            let mut any = false;
            for x in xs.iter() {
                if let Bool(true) = cb!(f, x.clone()) {
                    any = true;
                    break;
                }
            }
            Bool(any)
        }
        ("list_flat_map", [List(xs), f]) => {
            let mut out = Vec::new();
            for x in xs.iter() {
                match cb!(f, x.clone()) {
                    List(ys) => out.extend(ys.iter().cloned()),
                    _ => return Err(bad(name, &args)),
                }
            }
            list(out)
        }
        ("list_sort_by", [List(xs), f]) => {
            let mut keyed = Vec::with_capacity(xs.len());
            for x in xs.iter() {
                keyed.push((cb!(f, x.clone()), x.clone()));
            }
            // Stable, like the runtime's index tie-break.
            keyed.sort_by(|(a, _), (b, _)| compare(a, b).unwrap_or(std::cmp::Ordering::Equal));
            list(keyed.into_iter().map(|(_, x)| x).collect())
        }
        ("set_empty", []) => Set(Rc::new(vec![])),
        ("set_of", [List(xs)]) => {
            let mut out: Vec<Value> = Vec::new();
            for x in xs.iter() {
                if !out.iter().any(|y| equal(x, y)) {
                    out.push(x.clone());
                }
            }
            Set(Rc::new(out))
        }
        ("set_insert", [Set(xs), x]) if xs.iter().any(|y| equal(x, y)) => Set(xs.clone()),
        ("set_insert", [Set(xs), x]) => {
            let mut ys = (**xs).clone();
            ys.push(x.clone());
            Set(Rc::new(ys))
        }
        ("set_contains", [Set(xs), x]) => Bool(xs.iter().any(|y| equal(x, y))),
        ("set_to_list", [Set(xs)]) => list((**xs).clone()),
        ("map_of", [List(pairs)]) => {
            let mut es: Vec<(Value, Value)> = Vec::new();
            for p in pairs.iter() {
                let Tuple(kv) = p else {
                    return Err(bad(name, &args));
                };
                let (k, v) = (kv[0].clone(), kv[1].clone());
                match es.iter_mut().find(|(k2, _)| equal(&k, k2)) {
                    Some(e) => e.1 = v,
                    None => es.push((k, v)),
                }
            }
            Map(Rc::new(es))
        }
        ("map_entries", [Map(es)]) => list(
            es.iter()
                .map(|(k, v)| Tuple(Rc::new([k.clone(), v.clone()])))
                .collect(),
        ),
        ("map_update", [Map(es), k, f]) => {
            let old = es
                .iter()
                .find(|(k2, _)| equal(k, k2))
                .map(|(_, v)| v.clone());
            let v = cb!(f, option(module, old)?);
            return call(m, "map_insert", vec![Map(es.clone()), k.clone(), v], hs);
        }
        ("map_empty", []) => Map(Rc::new(vec![])),
        ("map_get", [Map(es), k]) => option(
            module,
            es.iter()
                .find(|(k2, _)| equal(k, k2))
                .map(|(_, v)| v.clone()),
        )?,
        ("map_insert", [Map(es), k, v]) => {
            let mut es = (**es).clone();
            match es.iter_mut().find(|(k2, _)| equal(k, k2)) {
                Some(e) => e.1 = v.clone(),
                None => es.push((k.clone(), v.clone())),
            }
            Map(Rc::new(es))
        }
        ("map_remove", [Map(es), k]) => Map(Rc::new(
            es.iter().filter(|(k2, _)| !equal(k, k2)).cloned().collect(),
        )),
        ("map_contains", [Map(es), k]) => Bool(es.iter().any(|(k2, _)| equal(k, k2))),
        ("map_len", [Map(es)]) => Int(es.len() as i64),
        ("map_keys", [Map(es)]) => list(es.iter().map(|(k, _)| k.clone()).collect()),
        ("map_values", [Map(es)]) => list(es.iter().map(|(_, v)| v.clone()).collect()),
        ("string_len", [Str(s)]) => Int(s.len() as i64),
        ("string_trim", [Str(s)]) => Str(s.trim().into()),
        ("string_contains", [Str(s), Str(t)]) => Bool(s.contains(&**t)),
        ("string_starts_with", [Str(s), Str(t)]) => Bool(s.starts_with(&**t)),
        ("string_ends_with", [Str(s), Str(t)]) => Bool(s.ends_with(&**t)),
        ("string_to_lower", [Str(s)]) => Str(s.to_lowercase().into()),
        ("string_to_upper", [Str(s)]) => Str(s.to_uppercase().into()),
        ("string_split", [Str(s), Str(sep)]) if sep.is_empty() => {
            list(s.chars().map(|c| Str(c.to_string().into())).collect())
        }
        ("string_split", [Str(s), Str(sep)]) => {
            list(s.split(&**sep).map(|p| Str(p.into())).collect())
        }
        ("string_repeat", [Str(s), Int(n)]) => {
            requires(*n >= 0, "String.repeat", "n >= 0")?;
            Str(s.repeat(*n as usize).into())
        }
        ("string_replace", [Str(s), Str(from), Str(to)]) if from.is_empty() => {
            let mut out = to.to_string();
            for c in s.chars() {
                out.push(c);
                out.push_str(to);
            }
            Str(out.into())
        }
        ("string_replace", [Str(s), Str(from), Str(to)]) => Str(s.replace(&**from, to).into()),
        ("string_slice", [Str(s), Int(a), Int(b)]) => {
            let len = s.len() as i64;
            let b0 = (*a).clamp(0, len) as usize;
            let b1 = (*b).clamp(b0 as i64, len) as usize;
            for byte in [b0, b1] {
                if !s.is_char_boundary(byte) {
                    return Err(defect(
                        "D031",
                        format!("byte offset {byte} is not on a character boundary"),
                    ));
                }
            }
            Str(s[b0..b1].into())
        }
        ("string_chars", [Str(s)]) => list(s.chars().map(|c| Str(c.to_string().into())).collect()),
        // Like the runtime: not an integer or beyond i64 is `None`; within i64 but beyond the
        // JS safe range is defect D003.
        ("string_parse_int", [Str(s)]) => {
            let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
            let well_formed = !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
            match s.parse::<i64>().ok().filter(|_| well_formed) {
                Some(i) => option(module, Some(int(i)?))?,
                None => option(module, None)?,
            }
        }
        ("int_to_string", [Int(i)]) => Str(i.to_string().into()),
        ("int_to_float", [Int(i)]) => Float(*i as f64),
        ("int_abs", [Int(i)]) => int(i.abs())?,
        ("int_clamp", [Int(i), Int(lo), Int(hi)]) => {
            requires(lo <= hi, "Int.clamp", "lo <= hi")?;
            Int(*i.clamp(lo, hi))
        }
        ("int_pow", [Int(b), Int(e)]) => {
            requires(*e >= 0, "Int.pow", "exp >= 0")?;
            match u32::try_from(*e).ok().and_then(|e| b.checked_pow(e)) {
                Some(r) => int(r)?,
                None => return Err(defect("D001", "integer overflow")),
            }
        }
        ("int_checked_add", [Int(a), Int(b)]) => match a.checked_add(*b) {
            Some(r) => option(module, Some(int(r)?))?,
            None => option(module, None)?,
        },
        ("float_round", [Float(f)]) => float_to_int(f.signum() * f.abs().round(), "Float.round")?,
        ("float_to_int_checked", [Float(f)]) if f.is_finite() => option(
            module,
            Some(float_to_int(f.trunc(), "Float.to_int_checked")?),
        )?,
        ("float_to_int_checked", [Float(_)]) => option(module, None)?,
        ("int_min", [Int(a), Int(b)]) => Int(*a.min(b)),
        ("int_max", [Int(a), Int(b)]) => Int(*a.max(b)),
        ("float_sqrt", [Float(f)]) => Float(f.sqrt()),
        ("float_is_nan", [Float(f)]) => Bool(f.is_nan()),
        ("console_print", [Str(line)]) => {
            m.print(line);
            Unit
        }
        ("console_host" | "console_captured", []) => {
            let effect = module
                .effects
                .iter()
                .position(|e| e.qualified_name() == "std::console::Console")
                .ok_or_else(|| {
                    Stop::Unsupported("std::console::Console is not in this module".into())
                })?;
            let native = |n| Fn(Rc::new(crate::Callable::Native(n)));
            Handler(Rc::new(crate::HandlerValue {
                effect: vow_core::EffectId(effect as u32),
                state: vec![],
                clauses: vec![
                    native(crate::Native::ConsolePrint),
                    native(crate::Native::ConsoleReadLine),
                ],
            }))
        }
        ("option_expect", [Variant(_, c, payload), Str(msg)]) => match (c, payload.first()) {
            (1, Some(v)) => v.clone(),
            _ => return Err(defect("D030", format!("Option.expect on None: {msg}"))),
        },
        _ => return Err(Stop::Unsupported(format!("builtin {name}"))),
    };
    Ok(Ok(v))
}
