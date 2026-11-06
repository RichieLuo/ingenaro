//! Authenticated, bounded, stateless Streamable HTTP MCP on an ephemeral IPv4
//! loopback port. Construction is explicit; importing the library starts nothing.

mod http;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{ToolCall, ToolClient};
use serde_json::{Value, json};

const MAX_WORKERS: usize = 16;
const MAX_PENDING_CALLS: usize = 8;
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const PROTOCOL: &str = "2025-11-25";

pub struct Server {
    endpoint: String,
    token: String,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    /// Secret bearer credential. Never include this in logs or diagnostics.
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Start only after explicit UI enablement. Supply a client scoped to the
/// current document generation, and drop this server when that document changes.
pub fn start(client: ToolClient) -> Result<Server, String> {
    let token = secure_token()?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| format!("Cannot bind MCP loopback listener: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let endpoint = format!("http://{address}/mcp");
    let stopped = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(State {
        client,
        token: token.clone(),
        address,
        stopped: stopped.clone(),
        pending: Mutex::new(BTreeMap::new()),
    });
    let thread = thread::Builder::new()
        .name("rpshop-mcp".into())
        .spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = Vec::new();
            while !shared.stopped.load(Ordering::Acquire) {
                let mut index = 0;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let _ = workers.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
                // Leave excess connections in the OS backlog instead of creating
                // unbounded threads, queues or request bodies.
                if workers.len() >= MAX_WORKERS {
                    thread::park_timeout(Duration::from_millis(10));
                    continue;
                }
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        let state = shared.clone();
                        if let Ok(worker) = thread::Builder::new()
                            .name("rpshop-mcp-request".into())
                            .spawn(move || serve(stream, &state))
                        {
                            workers.push(worker);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
            shared.stopped.store(true, Ordering::Release);
            // Worker reads/writes poll shutdown and live calls are cancelled while
            // waiting. No detached socket or GUI-waiter thread survives the server.
            for worker in workers {
                let _ = worker.join();
            }
        })
        .map_err(|e| format!("Cannot start MCP listener: {e}"))?;
    Ok(Server {
        endpoint,
        token,
        stopped,
        thread: Some(thread),
    })
}

struct State {
    client: ToolClient,
    token: String,
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    pending: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
}

fn serve(mut stream: TcpStream, state: &State) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let response = match http::read_request(
        &mut stream,
        state.address,
        &state.token,
        &state.stopped,
        HTTP_TIMEOUT,
    ) {
        Ok(body) => match serde_json::from_slice::<Value>(&body) {
            Ok(request) => dispatch(state, request),
            Err(_) => Response::json(200, rpc_error(Value::Null, -32700, "Parse error")),
        },
        Err((status, message)) => Response::json(status, json!({"error":message})),
    };
    write_response(&mut stream, response, &state.stopped);
}

struct Response {
    status: u16,
    body: Option<Value>,
}
impl Response {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: Some(body),
        }
    }
    fn accepted() -> Self {
        Self {
            status: 202,
            body: None,
        }
    }
}
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn dispatch(state: &State, request: Value) -> Response {
    let id = request.get("id").cloned();
    let valid_id = id
        .as_ref()
        .is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64() || id.is_null());
    let method = request.get("method").and_then(Value::as_str);
    if !request.is_object()
        || request["jsonrpc"] != "2.0"
        || method.is_none()
        || !valid_id
        || request.get("params").is_some_and(|p| !p.is_object())
    {
        return Response::json(200, rpc_error(Value::Null, -32600, "Invalid Request"));
    }
    let method = method.unwrap();
    if id.is_none() {
        if method == "notifications/cancelled"
            && let Some(request_id) = request["params"].get("requestId")
            && let Some(cancelled) = state.pending.lock().unwrap().get(&request_id.to_string())
        {
            cancelled.store(true, Ordering::Release);
        }
        return Response::accepted();
    }
    let id = id.unwrap();
    let params = &request["params"];
    let result = match method {
        "initialize" => {
            let Some(version) = params["protocolVersion"].as_str() else {
                return Response::json(200, rpc_error(id, -32602, "Missing protocolVersion"));
            };
            let version = if [PROTOCOL, "2025-06-18", "2025-03-26", "2024-11-05"].contains(&version)
            {
                version
            } else {
                PROTOCOL
            };
            json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":"rpshop-live","version":env!("CARGO_PKG_VERSION")},
                "instructions":"All tools operate on the live GUI document. Editing and filesystem actions require host approval. Calls do not open project files."})
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools":state.client.tool_definitions()}),
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return Response::json(200, rpc_error(id, -32602, "Missing tool name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Response::json(
                    200,
                    rpc_error(id, -32602, "Tool arguments must be an object"),
                );
            }
            if !state.client.supports_tool(name) {
                return Response::json(200, rpc_error(id, -32602, "Unknown tool"));
            }
            let key = id.to_string();
            let cancelled = Arc::new(AtomicBool::new(false));
            {
                let mut pending = state.pending.lock().unwrap();
                if pending.len() >= MAX_PENDING_CALLS {
                    return Response::json(
                        200,
                        rpc_error(id, -32000, "Too many concurrent tool calls"),
                    );
                }
                if pending.contains_key(&key) {
                    return Response::json(
                        200,
                        rpc_error(id, -32600, "Request ID is already in flight"),
                    );
                }
                pending.insert(key.clone(), cancelled.clone());
            }
            let result = state.client.call_interruptible(
                ToolCall {
                    name: name.into(),
                    arguments,
                },
                &state.stopped,
                &cancelled,
                super::bridge::CALL_TIMEOUT,
            );
            state.pending.lock().unwrap().remove(&key);
            match result {
                Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],
                    "structuredContent":if value.is_object(){value}else{json!({"result":value})},"isError":false}),
                Err(error) => json!({"content":[{"type":"text","text":error}],"isError":true}),
            }
        }
        _ => return Response::json(200, rpc_error(id, -32601, "Method not found")),
    };
    Response::json(200, json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn write_response(stream: &mut TcpStream, response: Response, stopped: &AtomicBool) {
    let mut body = response
        .body
        .map(|v| v.to_string().into_bytes())
        .unwrap_or_default();
    let status = if body.len() > MAX_RESPONSE_BYTES {
        body = b"{\"error\":\"Response exceeds 64 MiB\"}".to_vec();
        413
    } else {
        response.status
    };
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n{}\r\n",
        body.len(),
        if status == 401 {
            "WWW-Authenticate: Bearer\r\n"
        } else {
            ""
        }
    );
    let bytes = [headers.as_bytes(), body.as_slice()].concat();
    let start = Instant::now();
    let mut offset = 0;
    while offset < bytes.len() && start.elapsed() < HTTP_TIMEOUT && !stopped.load(Ordering::Acquire)
    {
        match stream.write(&bytes[offset..]) {
            Ok(0) => break,
            Ok(count) => offset += count,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => break,
        }
    }
}

fn secure_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(
                algorithm: *mut std::ffi::c_void,
                buffer: *mut u8,
                length: u32,
                flags: u32,
            ) -> i32;
        }
        // BCRYPT_USE_SYSTEM_PREFERRED_RNG requests the OS cryptographic RNG.
        if unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                2,
            )
        } < 0
        {
            return Err("Cannot generate MCP authentication credential".into());
        }
    }
    #[cfg(unix)]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|_| "Cannot generate MCP authentication credential".to_string())?;
    }
    #[cfg(not(any(windows, unix)))]
    return Err("MCP authentication is unsupported on this platform".into());
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
