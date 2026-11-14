//! A local fake RPC for tests: it records the JSON-RPC method of each request and either forwards
//! the request to a real node (the sandbox), drops signed sends with no answer, or answers 429.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub enum Mode {
    /// Forward every request to this `host:port`.
    Forward(String),
    /// Forward reads to this `host:port`; close a signed send with no answer.
    DropSends(String),
    /// Answer every request with HTTP 429 and Retry-After 0.
    Always429,
    /// Answer every request with HTTP 403.
    Always403,
}

pub struct FakeRpc {
    pub url: String,
    pub methods: Arc<Mutex<Vec<String>>>,
    /// The JSON-RPC body of each request, in order.
    pub bodies: Arc<Mutex<Vec<String>>>,
}

impl FakeRpc {
    pub fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }

    pub fn methods(&self) -> Vec<String> {
        self.methods.lock().unwrap().clone()
    }

    /// How many signed transactions reached this RPC.
    pub fn sends(&self) -> usize {
        self.methods().iter().filter(|m| is_send(m)).count()
    }
}

pub fn is_send(method: &str) -> bool {
    method == "send_tx" || method.starts_with("broadcast_tx")
}

pub async fn start(mode: Mode) -> FakeRpc {
    start_with_path(mode, "").await
}

/// The same, with `path` after the address in its URL (for example a fake key).
pub async fn start_with_path(mode: Mode, path: &str) -> FakeRpc {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/{path}", listener.local_addr().unwrap());
    let methods = Arc::new(Mutex::new(Vec::new()));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = methods.clone();
    let seen_bodies = bodies.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mode = mode.clone();
            let seen = seen.clone();
            let seen_bodies = seen_bodies.clone();
            tokio::spawn(async move {
                let _ = serve(stream, mode, seen, seen_bodies).await;
            });
        }
    });
    FakeRpc {
        url,
        methods,
        bodies,
    }
}

async fn serve(
    mut stream: TcpStream,
    mode: Mode,
    seen: Arc<Mutex<Vec<String>>>,
    seen_bodies: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_ascii_lowercase();
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[header_end..].to_vec();
    let method = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["method"].as_str().map(str::to_string))
        .unwrap_or_default();
    seen.lock().unwrap().push(method.clone());
    seen_bodies
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(&body).to_string());
    let target = match mode {
        Mode::Always429 => {
            let reply = "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            stream.write_all(reply.as_bytes()).await?;
            return stream.shutdown().await;
        }
        Mode::Always403 => {
            let reply = "HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            stream.write_all(reply.as_bytes()).await?;
            return stream.shutdown().await;
        }
        Mode::DropSends(_) if is_send(&method) => return Ok(()),
        Mode::Forward(t) | Mode::DropSends(t) => t,
    };
    // HTTP/1.0 to the node: a plain body with no chunks, and the node closes after it.
    let mut node = TcpStream::connect(&target).await?;
    let request = format!(
        "POST / HTTP/1.0\r\nhost: {target}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );
    node.write_all(request.as_bytes()).await?;
    node.write_all(&body).await?;
    let mut answer = Vec::new();
    node.read_to_end(&mut answer).await?;
    let split = answer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(answer.len(), |i| i + 4);
    let status = String::from_utf8_lossy(&answer[..split])
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_else(|| "502".to_string());
    let out = &answer[split..];
    let reply = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        out.len()
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.write_all(out).await?;
    stream.shutdown().await
}

/// The `host:port` of a network's first RPC.
pub fn host_port(network: &near_api::NetworkConfig) -> String {
    let url = &network.rpc_endpoints[0].url;
    format!(
        "{}:{}",
        url.host_str().unwrap(),
        url.port_or_known_default().unwrap()
    )
}

/// A network on `url` with one endpoint and one try.
pub fn network(url: &str) -> near_api::NetworkConfig {
    let mut n = near_api::NetworkConfig::from_rpc_url("testnet", url.parse().unwrap());
    n.rpc_endpoints = vec![near_api::RPCEndpoint::new(url.parse().unwrap()).with_retries(1)];
    n
}
