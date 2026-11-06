use std::collections::BTreeMap;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const HEADER_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: usize = super::super::MAX_ARGUMENT_BYTES + 1024 * 1024;
type HttpResult<T> = Result<T, (u16, &'static str)>;

pub(super) fn read_request(
    stream: &mut TcpStream,
    address: SocketAddr,
    token: &str,
    stopped: &AtomicBool,
    timeout: Duration,
) -> HttpResult<Vec<u8>> {
    let start = Instant::now();
    let mut bytes = Vec::with_capacity(2048);
    let header_end = loop {
        if let Some(position) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        if bytes.len() >= HEADER_LIMIT {
            return Err((431, "Headers exceed 16 KiB"));
        }
        let mut chunk = [0u8; 1024];
        let count = read(stream, &mut chunk, stopped, start, timeout)?;
        if count == 0 {
            return Err((400, "Incomplete HTTP headers"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    if header_end > HEADER_LIMIT {
        return Err((431, "Headers exceed 16 KiB"));
    }
    let headers =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| (400, "Invalid HTTP headers"))?;
    let mut lines = headers.split("\r\n");
    let request = lines
        .next()
        .unwrap_or_default()
        .split(' ')
        .collect::<Vec<_>>();
    if request.len() != 3 || request[2] != "HTTP/1.1" {
        return Err((400, "Expected HTTP/1.1"));
    }
    if request[0] != "POST" {
        return Err((405, "Only POST is supported"));
    }
    if request[1] != "/mcp" {
        return Err((404, "Endpoint not found"));
    }
    let mut map = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or((400, "Invalid HTTP header"))?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b < 32 && b != b'\t' || b == 127)
        {
            return Err((400, "Invalid HTTP header"));
        }
        if map.insert(key.to_ascii_lowercase(), value.trim()).is_some() {
            return Err((400, "Duplicate HTTP header"));
        }
    }
    let port = address.port();
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let host = map
        .get("host")
        .copied()
        .ok_or((400, "Missing Host header"))?;
    if !hosts
        .iter()
        .any(|expected| host.eq_ignore_ascii_case(expected))
    {
        return Err((403, "Host is not this loopback listener"));
    }
    if let Some(origin) = map.get("origin") {
        let origins = hosts.map(|host| format!("http://{host}"));
        if !origins
            .iter()
            .any(|expected| origin.eq_ignore_ascii_case(expected))
        {
            return Err((403, "Origin is not this loopback listener"));
        }
    }
    let expected = format!("Bearer {token}");
    let authorization = map.get("authorization").copied().unwrap_or_default();
    if !constant_time_equal(authorization.as_bytes(), expected.as_bytes()) {
        return Err((401, "Bearer authentication required"));
    }
    // Chunked/pipelined requests are deliberately unsupported. A single strict
    // framing rule avoids request smuggling and unbounded buffered reads.
    if map.contains_key("transfer-encoding") {
        return Err((400, "Transfer-Encoding is unsupported"));
    }
    let content_type = map
        .get("content-type")
        .copied()
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if !content_type.eq_ignore_ascii_case("application/json") {
        return Err((415, "Expected application/json"));
    }
    let length = map
        .get("content-length")
        .copied()
        .ok_or((400, "Missing Content-Length"))?;
    if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
        return Err((400, "Invalid Content-Length"));
    }
    let length = length
        .parse::<usize>()
        .map_err(|_| (413, "Request body is too large"))?;
    if length > BODY_LIMIT {
        return Err((413, "Request body exceeds 25 MiB"));
    }
    if bytes.len() - header_end > length {
        return Err((400, "HTTP pipelining is unsupported"));
    }
    let mut body = bytes.split_off(header_end);
    while body.len() < length {
        let mut chunk = [0u8; 8192];
        let take = chunk.len().min(length - body.len());
        let count = read(stream, &mut chunk[..take], stopped, start, timeout)?;
        if count == 0 {
            return Err((400, "Incomplete request body"));
        }
        body.extend_from_slice(&chunk[..count]);
    }
    Ok(body)
}
fn read(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    stopped: &AtomicBool,
    start: Instant,
    timeout: Duration,
) -> HttpResult<usize> {
    loop {
        if stopped.load(Ordering::Acquire) {
            return Err((408, "Server is stopping"));
        }
        if start.elapsed() >= timeout {
            return Err((408, "HTTP request timed out"));
        }
        match stream.read(buffer) {
            Ok(count) => return Ok(count),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err((400, "Cannot read HTTP request")),
        }
    }
}
fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}
