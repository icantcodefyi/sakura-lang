//! `Http.host()` and `Http.recording` on the native host (design §10.4, §17.73, §17.88,
//! §17.122; plan M5 N1b), ported from the JS runtime's `http_*` functions with their messages.
//! A send is `reqwest` over `rustls` (ring) with the platform's roots, no proxy (Node's `fetch`
//! reads none) and no redirects: `std/http`'s `follow` sends each hop itself, so the manifest's
//! `net` list is checked on every hop (`refusal`). A body is read up to `SAKURA_MAX_COPY_BYTES`;
//! a streamed one chunk by chunk. A cassette is the JS runtime's JSON Lines file, validated and
//! keyed the same way; `secret_headers` never reach it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sakura_rt::exec::status;
use sakura_rt::native::{HttpRequest, HttpResponse};
use sakura_rt::vt::t;
use serde_json::{Value, json};

use crate::fs::Dir;

/// JS `TOO_LARGE`.
const TOO_LARGE: &str = "response too large";

/// A response body: bytes received but not returned yet, and the live response while it has
/// more (`None` for a buffered body and after the end).
#[derive(Default)]
pub(crate) struct Body {
    pub buf: std::collections::VecDeque<u8>,
    pub live: Option<reqwest::Response>,
    pub closed: bool,
}

pub(crate) type Shared = Arc<Mutex<Body>>;

pub(crate) fn lock(b: &Shared) -> std::sync::MutexGuard<'_, Body> {
    b.lock().unwrap_or_else(|e| e.into_inner())
}

/// A completion: status and payload.
pub(crate) type Done = (u32, Vec<u8>);

/// The client every send of one host uses, built at its first send.
pub(crate) fn client() -> Result<reqwest::Client, String> {
    // reqwest has no default provider here (`rustls-no-provider`); another install wins.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|e| failure(&e))
}

/// JS `failureOf`: the innermost cause's message.
fn failure(e: &(dyn std::error::Error + 'static)) -> String {
    let mut e = e;
    while let Some(next) = e.source() {
        e = next;
    }
    e.to_string()
}

/// JS `netRefusal` inside `fetchArgs`: why the manifest's `net` list refuses `req`, checked
/// before the send starts (an immediate `Err`, as JS answers without suspending).
pub(crate) fn refusal(net: Option<&[String]>, url: &str) -> Option<String> {
    let net = net?;
    let Ok(u) = reqwest::Url::parse(url) else {
        return Some(format!("invalid URL {url}"));
    };
    let secure = matches!(u.scheme(), "https" | "wss");
    let port = u
        .port()
        .map_or(if secure { "443" } else { "80" }.to_string(), |p| {
            p.to_string()
        });
    let host = format!("{}:{port}", u.host_str().unwrap_or(""));
    (!net.contains(&host)).then(|| format!("{host} is not in the manifest's net list"))
}

/// The `reqwest` request for `req` (JS `fetchArgs`): headers appended, `secret_headers` set
/// over them and marked sensitive, no body for `GET`, `HEAD` or `Empty`. An error never
/// carries a header's value.
pub(crate) fn build(
    client: &reqwest::Client,
    req: &HttpRequest<'_>,
) -> Result<reqwest::Request, String> {
    let url = reqwest::Url::parse(req.url).map_err(|_| format!("invalid URL {}", req.url))?;
    let method = reqwest::Method::from_bytes(req.method.as_bytes()).map_err(|e| e.to_string())?;
    let name = |n: &str| {
        HeaderName::from_bytes(n.as_bytes()).map_err(|_| format!("invalid header name {n}"))
    };
    let value = |n: &str, v: &str| {
        HeaderValue::from_bytes(v.as_bytes()).map_err(|_| format!("invalid value for header {n}"))
    };
    let mut headers = HeaderMap::new();
    for (n, v) in &req.headers {
        headers.append(name(n)?, value(n, v)?);
    }
    for (n, v) in &req.secret_headers {
        let mut v = value(n, v)?;
        v.set_sensitive(true);
        headers.insert(name(n)?, v);
    }
    let mut b = client.request(method.clone(), url).headers(headers);
    if let Some(body) = req.body
        && method != reqwest::Method::GET
        && method != reqwest::Method::HEAD
    {
        b = b.body(body.to_vec());
    }
    b.build().map_err(|e| failure(&e))
}

/// The response headers as Fetch's `Headers` iterates them: lower-case names, sorted, values of
/// one name joined with `", "` except `set-cookie`, bytes read as Latin-1.
fn header_pairs(h: &HeaderMap) -> Vec<(String, String)> {
    let mut names: Vec<&HeaderName> = h.keys().collect();
    names.sort_by_key(|n| n.as_str());
    let latin1 = |v: &HeaderValue| {
        v.as_bytes()
            .iter()
            .map(|&b| char::from(b))
            .collect::<String>()
    };
    let mut out = vec![];
    for n in names {
        let vs = h.get_all(n).iter().map(latin1);
        if n == "set-cookie" {
            out.extend(vs.map(|v| (n.to_string(), v)));
        } else {
            out.push((n.to_string(), vs.collect::<Vec<_>>().join(", ")));
        }
    }
    out
}

/// `(Int status, List headers, …)`: the tuple prefix of a send's or an open's completion.
fn head(status: u16, headers: &[(String, String)]) -> Vec<u8> {
    let mut v = vec![t::TUPLE];
    v.extend_from_slice(&3u32.to_le_bytes());
    v.push(t::INT);
    v.extend_from_slice(&i64::from(status).to_le_bytes());
    v.push(t::LIST);
    v.extend_from_slice(&(headers.len() as u32).to_le_bytes());
    for (n, val) in headers {
        v.push(t::TUPLE);
        v.extend_from_slice(&2u32.to_le_bytes());
        for s in [n, val] {
            v.push(t::STRING);
            v.extend_from_slice(&(s.len() as u32).to_le_bytes());
            v.extend_from_slice(s.as_bytes());
        }
    }
    v
}

fn err(m: String) -> Done {
    (status::ERR, m.into_bytes())
}

/// JS `http_send`: the response, its body read up to `max` bytes (`response too large` past
/// it, and the connection dropped).
pub(crate) async fn send(client: reqwest::Client, rq: reqwest::Request, max: u64) -> Done {
    let head_only = rq.method() == reqwest::Method::HEAD;
    let mut r = match client.execute(rq).await {
        Ok(r) => r,
        Err(e) => return err(failure(&e)),
    };
    let st = r.status().as_u16();
    let headers = header_pairs(r.headers());
    // A HEAD or a null-body status may declare a length with no body to follow.
    let null_body = head_only || matches!(st, 101 | 103 | 204 | 205 | 304);
    let declared = r
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok());
    if !null_body && declared.is_some_and(|n| n > max) {
        return err(TOO_LARGE.into());
    }
    let mut body = vec![];
    loop {
        match r.chunk().await {
            Ok(Some(c)) => {
                if (body.len() + c.len()) as u64 > max {
                    return err(TOO_LARGE.into());
                }
                body.extend_from_slice(&c);
            }
            Ok(None) => break,
            Err(e) => return err(failure(&e)),
        }
    }
    let mut v = head(st, &headers);
    v.push(t::BYTES);
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(&body);
    (status::OK, v)
}

/// JS `http_stream_open`: the status and headers, the body left in `shared` (whose handle is
/// `handle`, the tuple's third field) for `read`.
pub(crate) async fn open(
    client: reqwest::Client,
    rq: reqwest::Request,
    shared: Shared,
    handle: [u32; 3],
) -> Done {
    let r = match client.execute(rq).await {
        Ok(r) => r,
        Err(e) => return err(failure(&e)),
    };
    let mut v = head(r.status().as_u16(), &header_pairs(r.headers()));
    lock(&shared).live = Some(r);
    v.push(t::HANDLE);
    for w in handle {
        v.extend_from_slice(&w.to_le_bytes());
    }
    (status::OK, v)
}

/// JS `http_stream_read` on a live body with nothing buffered: the next chunk's first `max`
/// bytes (the rest stays buffered), empty at the end.
pub(crate) async fn read(shared: Shared, max: i64) -> Done {
    let Some(mut r) = lock(&shared).live.take() else {
        return (status::OK, vec![]);
    };
    let chunk = r.chunk().await;
    let mut b = lock(&shared);
    if b.closed {
        return err("the response body is closed".into());
    }
    match chunk {
        Ok(Some(c)) => {
            b.live = Some(r);
            let n = c.len().min(usize::try_from(max).unwrap_or(0));
            b.buf.extend(&c[n..]);
            (status::OK, c[..n].to_vec())
        }
        Ok(None) => (status::OK, vec![]),
        Err(e) => {
            b.live = Some(r);
            err(failure(&e))
        }
    }
}

// ---- Cassettes (JS `http_recording_*`) ----------------------------------------------------

/// The ordinary request headers that carry credentials and are refused (§17.122).
const CREDENTIAL_HEADERS: [&str; 6] = [
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "x-auth-token",
    "cookie",
];

const CASSETTE_HEADER: &str = r#"{"schema_version":"1.0","kind":"cassette"}"#;

/// `SAKURA_RECORD=1`, set only by `sakura test --record`, allows appends.
fn recording() -> bool {
    std::env::var("SAKURA_RECORD").is_ok_and(|v| v == "1")
}

struct Keyed {
    next: usize,
    exchanges: Vec<Value>,
}

pub(crate) struct Cassette {
    dir: Dir,
    path: String,
    by_key: HashMap<String, Keyed>,
    headed: bool,
}

/// Open cassettes by id - 1.
#[derive(Default)]
pub(crate) struct Cassettes(Vec<Cassette>);

/// `btoa` of `b`.
fn base64(b: &[u8]) -> String {
    const D: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for g in b.chunks(3) {
        let x = [g[0], *g.get(1).unwrap_or(&0), *g.get(2).unwrap_or(&0)];
        let v = u32::from(x[0]) << 16 | u32::from(x[1]) << 8 | u32::from(x[2]);
        for j in 0..4 {
            s.push(if j <= g.len() {
                char::from(D[(v >> (18 - 6 * j) & 63) as usize])
            } else {
                '='
            });
        }
    }
    s
}

/// `atob` of `text` (forgiving-base64), `None` when it throws.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    sakura_rt::codec::base64_check(text.as_bytes(), false, true).ok()?;
    let (mut acc, mut bits, mut out) = (0u32, 0, vec![]);
    for c in text.bytes().take_while(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// JS `storedBody`: `(field, value)` with `prefix`, text when UTF-8, else `body_base64`.
fn stored(bytes: &[u8], prefix: &str) -> (String, String) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (format!("{prefix}body"), s.to_string()),
        Err(_) => (format!("{prefix}body_base64"), base64(bytes)),
    }
}

/// JS `exchangeKey`: the line's key (`body`, else `["base64", body_base64]`).
fn exchange_key(method: &Value, url: &Value, headers: &Value, x: &Value) -> String {
    let body = match x.get("body") {
        Some(b) if !b.is_null() => b.clone(),
        _ => json!(["base64", x.get("body_base64")]),
    };
    serde_json::to_string(&json!([method, url, headers, body])).unwrap_or_default()
}

/// JS `requestKey`, and the stored request body field.
fn request_key(req: &HttpRequest<'_>) -> (String, (String, String)) {
    let headers: Vec<[&str; 2]> = req.headers.iter().map(|(n, v)| [*n, *v]).collect();
    let body = stored(req.body.unwrap_or_default(), "");
    let x = json!({ &body.0: &body.1 });
    let key = exchange_key(&json!(req.method), &json!(req.url), &json!(headers), &x);
    (key, body)
}

/// JS `credentialHeader`.
fn credential(req: &HttpRequest<'_>) -> Option<String> {
    req.headers
        .iter()
        .find(|(n, _)| CREDENTIAL_HEADERS.contains(&n.to_lowercase().as_str()))
        .map(|(n, _)| format!("header {n} belongs in secret_headers (Http.bearer); not recorded"))
}

/// JS `missingExchangeField`.
fn missing(x: &Value) -> Option<&'static str> {
    let is = |f: &str, ok: &dyn Fn(&Value) -> bool| x.get(f).is_some_and(ok);
    let str_ = |v: &Value| v.is_string();
    let b64 = |v: &Value| v.as_str().and_then(unbase64).is_some();
    for f in ["method", "url"] {
        if !is(f, &str_) {
            return Some(f);
        }
    }
    if !is("body", &str_) && !is("body_base64", &b64) {
        return Some("body");
    }
    for f in ["headers", "response_headers"] {
        if !is(f, &Value::is_array) {
            return Some(f);
        }
    }
    if !is("status", &|v| v.as_f64().is_some_and(|n| n.fract() == 0.0)) {
        return Some("status");
    }
    if !is("response_body", &str_) && !is("response_body_base64", &b64) {
        return Some("response_body");
    }
    None
}

/// A recorded exchange's response (JS `recordedHeaders`, `recordedBytes`).
fn response(x: &Value) -> HttpResponse {
    let s = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
    let headers = x["response_headers"]
        .as_array()
        .map(|hs| hs.iter().map(|h| (s(h.get(0)), s(h.get(1)))).collect())
        .unwrap_or_default();
    let body = match x.get("response_body_base64") {
        Some(b) => b.as_str().and_then(unbase64).unwrap_or_default(),
        None => s(x.get("response_body")).into_bytes(),
    };
    HttpResponse {
        status: x["status"].as_f64().unwrap_or_default() as i64,
        headers,
        body,
    }
}

impl Cassettes {
    /// JS `http_recording_open`: reads `path` inside `dir` (a missing file is empty).
    pub fn open(&mut self, dir: Dir, path: &str) -> Result<i64, String> {
        if matches!(dir, Dir::Memory { .. }) {
            return Err(format!("{path}: this directory has no host access"));
        }
        let text = match dir.read(path, u64::MAX) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) if e == "not found" => String::new(),
            Err(e) => return Err(format!("{path}: {e}")),
        };
        let mut by_key: HashMap<String, Keyed> = HashMap::new();
        let mut headed = false;
        for (i, line) in text.split('\n').enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let x: Value =
                serde_json::from_str(line).map_err(|e| format!("{path}:{}: {e}", i + 1))?;
            if !headed {
                match x.get("schema_version").and_then(Value::as_str) {
                    Some(v) if v.split('.').next() == Some("1") => {}
                    v => {
                        let found = v.map_or("no schema_version line".into(), |v| {
                            format!("schema_version {v}")
                        });
                        return Err(format!(
                            "{path}:{}: not a version 1 cassette ({found}); re-record it with sakura test --record",
                            i + 1
                        ));
                    }
                }
                headed = true;
                continue;
            }
            if let Some(f) = missing(&x) {
                return Err(format!(
                    "{path}:{}: not a cassette exchange (missing {f})",
                    i + 1
                ));
            }
            let key = exchange_key(&x["method"], &x["url"], &x["headers"], &x);
            by_key
                .entry(key)
                .or_insert(Keyed {
                    next: 0,
                    exchanges: vec![],
                })
                .exchanges
                .push(x);
        }
        self.0.push(Cassette {
            dir,
            path: path.to_string(),
            by_key,
            headed,
        });
        Ok(self.0.len() as i64)
    }

    fn get(&mut self, id: i64) -> Result<&mut Cassette, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.0.get_mut(i))
            .ok_or_else(|| "an unknown cassette".into())
    }

    /// JS `replayed`: the next recorded response, `Ok(None)` for a miss while recording.
    pub fn lookup(
        &mut self,
        id: i64,
        req: &HttpRequest<'_>,
    ) -> Result<Option<HttpResponse>, String> {
        let c = self.get(id)?;
        if let Some(refused) = credential(req) {
            return Err(refused);
        }
        if let Some(hit) = c.by_key.get_mut(&request_key(req).0) {
            let x = &hit.exchanges[hit.next.min(hit.exchanges.len() - 1)];
            hit.next += 1;
            return Ok(Some(response(x)));
        }
        if recording() {
            return Ok(None);
        }
        Err(format!(
            "no recording for {} {} in {}; run sakura test --record",
            req.method, req.url, c.path
        ))
    }

    /// JS `appendExchange`.
    pub fn append(
        &mut self,
        id: i64,
        req: &HttpRequest<'_>,
        resp: &HttpResponse,
    ) -> Result<(), String> {
        let c = self.get(id)?;
        if !recording() {
            return Err(format!("{}: recording needs sakura test --record", c.path));
        }
        if let Some(refused) = credential(req) {
            return Err(refused);
        }
        let (key, (bf, bv)) = request_key(req);
        let headers: Vec<[&str; 2]> = req.headers.iter().map(|(n, v)| [*n, *v]).collect();
        let rh: Vec<[&str; 2]> = resp
            .headers
            .iter()
            .map(|(n, v)| {
                [
                    n.as_str(),
                    if n.eq_ignore_ascii_case("set-cookie") {
                        "<redacted>"
                    } else {
                        v.as_str()
                    },
                ]
            })
            .collect();
        let (rf, rv) = stored(&resp.body, "response_");
        // `JSON.stringify` with the keys in JS's insertion order.
        let line = format!(
            "{{\"method\":{},\"url\":{},\"headers\":{},{}:{},\"status\":{},\"response_headers\":{},{}:{}}}",
            json!(req.method),
            json!(req.url),
            json!(headers),
            json!(bf),
            json!(bv),
            resp.status,
            json!(rh),
            json!(rf),
            json!(rv),
        );
        let x: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let text = format!(
            "{}{line}\n",
            if c.headed {
                String::new()
            } else {
                format!("{CASSETTE_HEADER}\n")
            }
        );
        c.dir
            .append(&c.path, text.as_bytes())
            .map_err(|e| format!("{}: {e}", c.path))?;
        c.headed = true;
        c.by_key
            .entry(key)
            .or_insert(Keyed {
                next: 1,
                exchanges: vec![],
            })
            .exchanges
            .push(x);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(method: &'a str, url: &'a str, headers: Vec<(&'a str, &'a str)>) -> HttpRequest<'a> {
        HttpRequest {
            method,
            url,
            headers,
            ..Default::default()
        }
    }

    fn dir(files: &[(&str, &str)]) -> (std::path::PathBuf, Dir) {
        let d = std::env::temp_dir().join(format!(
            "sakura-cassette-{}-{}",
            std::process::id(),
            files.len() + files.iter().map(|f| f.1.len()).sum::<usize>()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (n, text) in files {
            std::fs::write(d.join(n), text).unwrap();
        }
        let root = Dir::root(&d, None).unwrap();
        (d, root)
    }

    #[test]
    fn base64_round_trips_as_btoa_and_atob() {
        for b in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0xff, 0, 0x80]] {
            assert_eq!(unbase64(&base64(b)).as_deref(), Some(b));
        }
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(unbase64(" Zm 8").as_deref(), Some(&b"fo"[..]));
        assert_eq!(unbase64("Z"), None);
        assert_eq!(unbase64("Zm9v!"), None);
    }

    #[test]
    fn a_cassette_replays_in_order_and_validates_as_js() {
        let x = |status: u16, body: &str| {
            format!(
                r#"{{"method":"GET","url":"https://a.example/x","headers":[],"body":"","status":{status},"response_headers":[["set-cookie","<redacted>"]],"response_body":"{body}"}}"#
            )
        };
        let text = format!(
            "{CASSETTE_HEADER}\n\n{}\n{}\n",
            x(200, "one"),
            x(201, "two")
        );
        let (d, root) = dir(&[
            ("ok.jsonl", &text),
            ("old.jsonl", "{\"schema_version\":\"2.0\"}\n"),
            ("bare.jsonl", &x(200, "")),
            (
                "short.jsonl",
                &format!(
                    "{CASSETTE_HEADER}\n{{\"method\":\"GET\",\"url\":\"u\",\"headers\":[],\"body\":\"\",\"status\":200,\"response_body\":\"\"}}\n"
                ),
            ),
        ]);
        let mut c = Cassettes::default();
        let id = c.open(root.clone(), "ok.jsonl").unwrap();
        let get = req("GET", "https://a.example/x", vec![]);
        let statuses: Vec<i64> = (0..3)
            .map(|_| c.lookup(id, &get).unwrap().unwrap().status)
            .collect();
        assert_eq!(statuses, [200, 201, 201], "in order, the last repeats");
        assert_eq!(
            c.lookup(id, &req("GET", "https://a.example/y", vec![])),
            Err(
                "no recording for GET https://a.example/y in ok.jsonl; run sakura test --record"
                    .into()
            )
        );
        assert_eq!(
            c.lookup(
                id,
                &req("GET", "https://a.example/x", vec![("Cookie", "k")])
            ),
            Err("header Cookie belongs in secret_headers (Http.bearer); not recorded".into())
        );
        assert_eq!(
            c.open(root.clone(), "old.jsonl"),
            Err("old.jsonl:1: not a version 1 cassette (schema_version 2.0); re-record it with sakura test --record".into())
        );
        assert_eq!(
            c.open(root.clone(), "bare.jsonl"),
            Err("bare.jsonl:1: not a version 1 cassette (no schema_version line); re-record it with sakura test --record".into())
        );
        assert_eq!(
            c.open(root.clone(), "short.jsonl"),
            Err("short.jsonl:2: not a cassette exchange (missing response_headers)".into())
        );
        let empty = c.open(root.clone(), "absent.jsonl").unwrap();
        assert!(c.lookup(empty, &get).is_err());
        assert_eq!(
            c.append(empty, &get, &HttpResponse::default()),
            Err("absent.jsonl: recording needs sakura test --record".into())
        );
        assert_eq!(
            c.open(Dir::memory(vec![]), "m.jsonl"),
            Err("m.jsonl: this directory has no host access".into())
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_net_list_is_checked_by_host_and_default_port() {
        let net = ["a.example:443".to_string(), "localhost:8080".to_string()];
        assert_eq!(refusal(None, "https://b.example/"), None);
        assert_eq!(refusal(Some(&net), "https://a.example/x"), None);
        assert_eq!(refusal(Some(&net), "http://localhost:8080/"), None);
        assert_eq!(
            refusal(Some(&net), "http://a.example/"),
            Some("a.example:80 is not in the manifest's net list".into())
        );
        assert_eq!(refusal(Some(&net), "nope"), Some("invalid URL nope".into()));
    }
}
