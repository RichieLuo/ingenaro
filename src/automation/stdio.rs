//! Newline JSON-RPC proxy for desktop MCP clients. Only protocol responses go to
//! stdout; credentials are never printed, followed through redirects, or proxied.

use std::io::{BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::server::MAX_RESPONSE_BYTES;
use serde_json::{Value, json};

const MAX_LINE: usize = super::MAX_ARGUMENT_BYTES + 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(130);

/// Forward MCP newline-delimited stdin/stdout to an explicitly supplied local
/// endpoint. This does not enable a server or open project files. Endpoint and
/// token should be obtained from an explicitly enabled GUI integration.
pub fn run_stdio(endpoint: &str, token: &str) -> Result<(), String> {
    let address = endpoint_address(endpoint)?;
    validate_token(token)?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let credential = Arc::new(token.to_owned());
    let mut workers: Vec<JoinHandle<Result<(), String>>> = Vec::new();
    let mut failure = None;
    loop {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                match workers.swap_remove(index).join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        failure = Some(error);
                        break;
                    }
                    Err(_) => {
                        failure = Some("MCP forwarding worker failed".into());
                        break;
                    }
                }
            } else {
                index += 1;
            }
        }
        if failure.is_some() {
            break;
        }
        let line = match read_line(&mut input) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(error) => {
                failure = Some(error);
                break;
            }
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let request = match serde_json::from_slice::<Value>(&line) {
            Ok(value) => value,
            Err(_) => {
                if let Err(error) = emit(&output, &rpc_error(Value::Null, -32700, "Parse error")) {
                    failure = Some(error);
                    break;
                }
                continue;
            }
        };
        let id = request.get("id").cloned();
        if !request.is_object() || request["jsonrpc"] != "2.0" || !request["method"].is_string() {
            if let Err(error) = emit(&output, &rpc_error(Value::Null, -32600, "Invalid Request")) {
                failure = Some(error);
                break;
            }
            continue;
        }
        // Notifications (including cancellation) bypass occupied call workers.
        let Some(id) = id else {
            if let Err(error) = forward(address, &credential, &line) {
                failure = Some(error);
                break;
            }
            continue;
        };
        if workers.len() >= 8 {
            if let Err(error) = emit(
                &output,
                &rpc_error(id, -32000, "Too many concurrent MCP calls"),
            ) {
                failure = Some(error);
                break;
            }
            continue;
        }
        let output = output.clone();
        let token = credential.clone();
        let worker = thread::Builder::new()
            .name("ingenaro-mcp-stdio".into())
            .spawn(move || {
                let response = match forward(address, &token, &line) {
                    Ok(Some(response)) => response,
                    Ok(None) => rpc_error(id, -32000, "MCP endpoint returned no response"),
                    Err(error) => rpc_error(id, -32000, &error),
                };
                emit(&output, &response)
            });
        match worker {
            Ok(worker) => workers.push(worker),
            Err(_) => {
                failure = Some("Cannot start MCP forwarding worker".into());
                break;
            }
        }
    }
    for worker in workers {
        match worker.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                failure.get_or_insert(error);
            }
            Err(_) => {
                failure.get_or_insert_with(|| "MCP forwarding worker failed".into());
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn read_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().map_err(|_| "Cannot read MCP stdin")?;
        if bytes.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        let take = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |i| i + 1);
        if line.len() + take > MAX_LINE {
            return Err("MCP stdin line exceeds 25 MiB".into());
        }
        line.extend_from_slice(&bytes[..take]);
        let done = bytes[take - 1] == b'\n';
        reader.consume(take);
        if done {
            return Ok(Some(line));
        }
    }
}

fn emit(output: &Mutex<std::io::Stdout>, value: &Value) -> Result<(), String> {
    let mut output = output.lock().map_err(|_| "Cannot lock MCP stdout")?;
    serde_json::to_writer(&mut *output, value).map_err(|_| "Cannot write MCP stdout")?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|_| "Cannot write MCP stdout".into())
}
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn endpoint_address(endpoint: &str) -> Result<SocketAddr, String> {
    let invalid = || "MCP endpoint must be http://127.0.0.1:<port>/mcp (or localhost)".to_string();
    let authority = endpoint
        .strip_prefix("http://")
        .and_then(|s| s.strip_suffix("/mcp"))
        .ok_or_else(invalid)?;
    let (host, port) = authority.split_once(':').ok_or_else(invalid)?;
    if !matches!(host, "127.0.0.1" | "localhost") || !port.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(invalid)?;
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}
fn validate_token(token: &str) -> Result<(), String> {
    if token.len() < 16
        || token.len() > 1024
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    {
        return Err("Invalid MCP bearer credential".into());
    }
    Ok(())
}

fn forward(address: SocketAddr, token: &str, body: &[u8]) -> Result<Option<Value>, String> {
    let start = Instant::now();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .map_err(|_| "Cannot connect to live Ingenaro endpoint")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|_| "Cannot configure MCP socket")?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .map_err(|_| "Cannot configure MCP socket")?;
    let header = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let bytes = [header.as_bytes(), body].concat();
    let mut offset = 0;
    while offset < bytes.len() {
        if start.elapsed() >= TIMEOUT {
            return Err("MCP HTTP request timed out".into());
        }
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err("MCP endpoint disconnected".into()),
            Ok(count) => offset += count,
            Err(error) if transient(&error) => {}
            Err(_) => return Err("Cannot write MCP HTTP request".into()),
        }
    }
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break index + 4;
        }
        if bytes.len() > 16 * 1024 {
            return Err("MCP HTTP response headers exceed 16 KiB".into());
        }
        let mut chunk = [0u8; 1024];
        let count = read(&mut stream, &mut chunk, start)?;
        if count == 0 {
            return Err("MCP endpoint returned incomplete headers".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    if header_end > 16 * 1024 {
        return Err("MCP HTTP response headers exceed 16 KiB".into());
    }
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| "Invalid MCP HTTP response headers")?;
    let mut lines = headers.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("Invalid MCP HTTP response status")?;
    if !matches!(status, 200 | 202 | 204) {
        return Err(format!("MCP endpoint returned HTTP {status}"));
    }
    let mut length = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or("Invalid MCP HTTP response header")?;
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Chunked MCP responses are unsupported".into());
        }
        if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Duplicate MCP response Content-Length".into());
            }
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid MCP response length")?,
            );
        }
    }
    let length = length.ok_or("Missing MCP response Content-Length")?;
    if length > MAX_RESPONSE_BYTES {
        return Err("MCP response exceeds 64 MiB".into());
    }
    let mut body = bytes.split_off(header_end);
    if body.len() > length {
        return Err("Invalid MCP response framing".into());
    }
    while body.len() < length {
        let mut chunk = [0u8; 8192];
        let take = chunk.len().min(length - body.len());
        let count = read(&mut stream, &mut chunk[..take], start)?;
        if count == 0 {
            return Err("MCP endpoint returned an incomplete response".into());
        }
        body.extend_from_slice(&chunk[..count]);
    }
    if matches!(status, 202 | 204) {
        return Ok(None);
    }
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| "MCP endpoint returned invalid JSON".into())
}
fn read(stream: &mut TcpStream, buffer: &mut [u8], start: Instant) -> Result<usize, String> {
    loop {
        if start.elapsed() >= TIMEOUT {
            return Err("MCP HTTP request timed out".into());
        }
        match stream.read(buffer) {
            Ok(count) => return Ok(count),
            Err(error) if transient(&error) => {}
            Err(_) => return Err("Cannot read MCP HTTP response".into()),
        }
    }
}
fn transient(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_endpoint_validation_never_leaks_credentials_off_loopback() {
        assert!(endpoint_address("http://127.0.0.1:1234/mcp").is_ok());
        assert!(endpoint_address("http://localhost:1234/mcp").is_ok());
        for endpoint in [
            "https://example.com/mcp",
            "http://127.0.0.1.evil:1234/mcp",
            "http://127.0.0.2:1234/mcp",
            "http://user@127.0.0.1:1234/mcp",
            "http://127.0.0.1:1234/mcp?token=x",
            "http://127.0.0.1:0/mcp",
        ] {
            assert!(endpoint_address(endpoint).is_err(), "{endpoint}");
        }
        assert!(validate_token("0123456789012345\r\nInjected: x").is_err());
    }
    #[test]
    fn proxy_http_forwarding_authenticates_live_endpoint_without_redirects() {
        let (_host, client) = super::super::ToolHost::new();
        let server = super::super::start(client).unwrap();
        let address = endpoint_address(server.endpoint()).unwrap();
        let response = forward(
            address,
            server.token(),
            br#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(response["id"], 5);
        assert_eq!(response["result"], json!({}));
        assert!(
            forward(
                address,
                "wrong-authentication-token",
                br#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#
            )
            .unwrap_err()
            .contains("401")
        );
        assert!(
            forward(
                address,
                server.token(),
                br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn bounded_newline_reader_handles_crlf_and_eof() {
        let mut input = std::io::Cursor::new(b"{}\r\nlast");
        assert_eq!(read_line(&mut input).unwrap().unwrap(), b"{}\r\n");
        assert_eq!(read_line(&mut input).unwrap().unwrap(), b"last");
        assert!(read_line(&mut input).unwrap().is_none());
    }
}
