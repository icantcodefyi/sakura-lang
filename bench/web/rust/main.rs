// TechEmpower-style JSON and plaintext server on hyper 1.x and tokio (bench/web/README.md).
// AI-assisted (Claude), reviewed by the contributor.
//
// `main <port> [single|multi]` (port also from PORT) serves on 127.0.0.1 until the process
// stops: `single` (the default) runs tokio's current-thread runtime, the single-threaded lane;
// `multi` runs the multi-thread runtime with one worker per CPU. HTTP/1.1 only, keep-alive and
// pipelining on (hyper's defaults). `/json` serializes a fresh struct per request with
// serde_json; hyper adds `date` and `content-length`.

use std::convert::Infallible;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{CONTENT_TYPE, HeaderValue, SERVER};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::net::TcpListener;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

fn reply(content_type: &'static str, body: Vec<u8>) -> Response<Full<Bytes>> {
    let mut r = Response::new(Full::new(Bytes::from(body)));
    let h = r.headers_mut();
    h.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(SERVER, HeaderValue::from_static("hyper"));
    r
}

async fn handle(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(match req.uri().path() {
        "/json" => {
            let body = serde_json::to_vec(&Message {
                message: "Hello, World!",
            })
            .expect("a struct of one string serializes");
            reply("application/json", body)
        }
        "/plaintext" => reply("text/plain", b"Hello, World!".to_vec()),
        _ => {
            let mut r = Response::new(Full::new(Bytes::from_static(b"not found")));
            *r.status_mut() = StatusCode::NOT_FOUND;
            r
        }
    })
}

async fn serve(port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        stream.set_nodelay(true)?;
        tokio::spawn(async move {
            let _ = http1::Builder::new()
                .pipeline_flush(true)
                .serve_connection(TokioIo::new(stream), service_fn(handle))
                .await;
        });
    }
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .or_else(|| std::env::var("PORT").ok())
        .unwrap_or_else(|| "8080".into())
        .parse::<u16>()
        .expect("port is a number in 0..=65535");
    let mut runtime = match args.next().as_deref() {
        None | Some("single") => tokio::runtime::Builder::new_current_thread(),
        Some("multi") => tokio::runtime::Builder::new_multi_thread(),
        Some(other) => panic!("lane is `single` or `multi`, not `{other}`"),
    };
    runtime.enable_all().build()?.block_on(serve(port))
}
