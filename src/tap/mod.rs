/// HTTP tap — a transparent reverse proxy that intercepts every HTTP request
/// between a frontend and a backend, capturing method / path / status / duration.
///
/// Usage: run with --tap LISTEN_PORT:TARGET_PORT
///   arch-live listens on LISTEN_PORT (e.g. 3001).
///   Your frontend calls http://localhost:3001/api/...
///   arch-live forwards to http://localhost:TARGET_PORT/api/...
///   Before returning the response it emits an HttpRequest CollectorEvent.
///
/// HTTPS note: TLS-encrypted paths are opaque from the outside.
/// To tap HTTPS, terminate TLS before the tap (e.g. a local mkcert proxy).

use std::net::SocketAddr;
use std::time::Instant;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::collector::CollectorEvent;

pub async fn run(listen_port: u16, target_port: u16, tx: mpsc::Sender<CollectorEvent>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], listen_port));

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[tap] failed to bind :{}: {}", listen_port, e);
            return;
        }
    };

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => continue,
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
                let tx = tx.clone();
                async move { handle(req, target_port, tx).await }
            });

            if let Err(e) = server_http1::Builder::new().serve_connection(io, svc).await {
                // Client disconnected mid-stream — not an error worth surfacing
                tracing::debug!("tap connection closed: {}", e);
            }
        });
    }
}

/// Forward a single request to the target and emit an HttpRequest event.
async fn handle(
    req: Request<Incoming>,
    target_port: u16,
    tx: mpsc::Sender<CollectorEvent>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_owned())
        .unwrap_or_else(|| "/".to_owned());

    match forward(req, target_port).await {
        Ok((resp, status)) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = tx.try_send(CollectorEvent::HttpRequest {
                method,
                path,
                status,
                duration_ms,
                timestamp: chrono::Utc::now(),
            });
            Ok(resp)
        }
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            tracing::debug!("tap forward error: {}", e);
            let _ = tx.try_send(CollectorEvent::HttpRequest {
                method,
                path,
                status: 502,
                duration_ms,
                timestamp: chrono::Utc::now(),
            });
            // Return 502 to the client
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(format!("arch-live tap: {}", e))))
                .unwrap())
        }
    }
}

/// Connect to target, forward the request, collect the response.
async fn forward(
    req: Request<Incoming>,
    target_port: u16,
) -> Result<(Response<Full<Bytes>>, u16)> {
    use hyper::client::conn::http1 as client_http1;
    use tokio::net::TcpStream;

    let (parts, body) = req.into_parts();

    // Buffer the request body so we can re-use it
    let body_bytes = body
        .collect()
        .await
        .context("read request body")?
        .to_bytes();

    // Open a connection to the target
    let stream = TcpStream::connect(("127.0.0.1", target_port))
        .await
        .with_context(|| format!("connect to target :{}", target_port))?;

    let io = TokioIo::new(stream);
    let (mut sender, conn) = client_http1::handshake(io)
        .await
        .context("http1 handshake")?;

    // Drive the connection in the background
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!("tap client conn error: {}", e);
        }
    });

    // Rebuild the request with corrected Host header
    let authority = format!("127.0.0.1:{}", target_port);
    let uri = format!(
        "http://{}{}",
        authority,
        parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/")
    );

    let mut builder = Request::builder()
        .method(parts.method)
        .uri(uri)
        .version(hyper::Version::HTTP_11);

    for (name, value) in parts.headers.iter() {
        // Replace Host with target host; skip connection-level headers
        if name == hyper::header::HOST
            || name == hyper::header::CONNECTION
            || name.as_str() == "keep-alive"
            || name.as_str() == "proxy-connection"
        {
            continue;
        }
        builder = builder.header(name, value);
    }
    let fwd_req = builder
        .header(hyper::header::HOST, &authority)
        .body(Full::new(body_bytes))
        .context("build forwarded request")?;

    let resp = sender
        .send_request(fwd_req)
        .await
        .context("send to target")?;

    let status = resp.status().as_u16();
    let (resp_parts, resp_body) = resp.into_parts();

    let resp_bytes = resp_body
        .collect()
        .await
        .context("read response body")?
        .to_bytes();

    // Rebuild the response, stripping transfer-encoding (we already buffered)
    let mut rb = Response::builder().status(resp_parts.status);
    for (name, value) in resp_parts.headers.iter() {
        if name == hyper::header::TRANSFER_ENCODING {
            continue;
        }
        rb = rb.header(name, value);
    }
    let response = rb
        .body(Full::new(resp_bytes))
        .context("build response")?;

    Ok((response, status))
}
