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
    /// Each forwarded request body with the node's answer body, in the order the answers came.
    pub answers: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeRpc {
    pub fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }

    pub fn answers(&self) -> Vec<(String, String)> {
        self.answers.lock().unwrap().clone()
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
    let answers = Arc::new(Mutex::new(Vec::new()));
    let seen_answers = answers.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mode = mode.clone();
            let seen = seen.clone();
            let seen_bodies = seen_bodies.clone();
            let seen_answers = seen_answers.clone();
            tokio::spawn(async move {
                let _ = serve(stream, mode, seen, seen_bodies, seen_answers).await;
            });
        }
    });
    FakeRpc {
        url,
        methods,
        bodies,
        answers,
    }
}

async fn serve(
    mut stream: TcpStream,
    mode: Mode,
    seen: Arc<Mutex<Vec<String>>>,
    seen_bodies: Arc<Mutex<Vec<String>>>,
    seen_answers: Arc<Mutex<Vec<(String, String)>>>,
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
    let (status, out) = post_raw(&target, &body).await?;
    seen_answers.lock().unwrap().push((
        String::from_utf8_lossy(&body).to_string(),
        String::from_utf8_lossy(&out).to_string(),
    ));
    let out = &out[..];
    let reply = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        out.len()
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.write_all(out).await?;
    stream.shutdown().await
}

/// POSTs `body` to the node at `target` (`host:port`) and returns the HTTP status and the answer
/// body. HTTP/1.0: a plain body with no chunks, and the node closes after it.
pub async fn post_raw(target: &str, body: &[u8]) -> std::io::Result<(String, Vec<u8>)> {
    let mut node = TcpStream::connect(target).await?;
    let request = format!(
        "POST / HTTP/1.0\r\nhost: {target}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );
    node.write_all(request.as_bytes()).await?;
    node.write_all(body).await?;
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
    Ok((status, answer[split..].to_vec()))
}

/// The `error.cause` and the top-level `error.data` of a JSON-RPC answer, or `None` when it has no
/// error or is not JSON. nearcore puts the detail of INVALID_TRANSACTION only in `data`.
pub fn rpc_error_parts(answer: &str) -> Option<(serde_json::Value, serde_json::Value)> {
    let v: serde_json::Value = serde_json::from_str(answer).ok()?;
    let error = v.get("error")?;
    Some((
        error.get("cause").cloned().unwrap_or_default(),
        error.get("data").cloned().unwrap_or_default(),
    ))
}

/// What a `view_access_key` query answered.
#[derive(Debug, PartialEq, Eq)]
pub struct KeyAnswer {
    pub nonce: u64,
    pub block_height: u64,
    pub block_hash: String,
}

/// The last `view_access_key` answer for `account` among recorded (request, answer) pairs.
pub fn access_key_answer(pairs: &[(String, String)], account: &str) -> Option<KeyAnswer> {
    pairs.iter().rev().find_map(|(request, answer)| {
        let request: serde_json::Value = serde_json::from_str(request).ok()?;
        let params = &request["params"];
        if params["request_type"] != "view_access_key" || params["account_id"] != account {
            return None;
        }
        let result = serde_json::from_str::<serde_json::Value>(answer).ok()?["result"].clone();
        Some(KeyAnswer {
            nonce: result["nonce"].as_u64()?,
            block_height: result["block_height"].as_u64()?,
            block_hash: result["block_hash"].as_str()?.to_string(),
        })
    })
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
