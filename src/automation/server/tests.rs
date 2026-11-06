use super::super::ToolHost;
use super::*;
use std::io::Read;

fn address(server: &Server) -> SocketAddr {
    server
        .endpoint()
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/mcp")
        .unwrap()
        .parse()
        .unwrap()
}
fn raw(server: &Server, headers: &str, body: &[u8]) -> (u16, Option<Value>) {
    let address = address(server);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    );
    let bytes = [request.as_bytes(), body].concat();
    stream.write_all(&bytes).unwrap();
    let mut result = Vec::new();
    stream.read_to_end(&mut result).unwrap();
    let end = result.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let status = std::str::from_utf8(&result[..end])
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let body = if result.len() > end {
        Some(serde_json::from_slice(&result[end..]).unwrap())
    } else {
        None
    };
    (status, body)
}
fn rpc(server: &Server, request: Value) -> (u16, Option<Value>) {
    raw(
        server,
        &format!("Authorization: Bearer {}\r\n", server.token()),
        request.to_string().as_bytes(),
    )
}
fn next_request(host: &ToolHost) -> super::super::ToolRequest {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(request) = host.try_recv() {
            return request;
        }
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn authentication_origin_host_and_http_bounds_are_enforced() {
    let (_host, client) = ToolHost::new();
    let server = start(client).unwrap();
    assert!(address(&server).ip().is_loopback());
    assert_eq!(server.token().len(), 64);
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    assert_eq!(raw(&server, "", body).0, 401);
    assert_eq!(raw(&server, "Authorization: Bearer wrong\r\n", body).0, 401);
    let auth = format!("Authorization: Bearer {}\r\n", server.token());
    assert_eq!(
        raw(
            &server,
            &format!("{auth}Origin: https://evil.example\r\n"),
            body
        )
        .0,
        403
    );
    assert_eq!(
        raw(&server, &format!("{auth}Origin: null\r\n"), body).0,
        403
    );
    assert_eq!(
        raw(
            &server,
            &format!(
                "{auth}Origin: http://127.0.0.1:{}\r\n",
                address(&server).port()
            ),
            body
        )
        .0,
        200
    );
    // Duplicate Host is rejected before authentication or dispatch.
    assert_eq!(
        raw(&server, &format!("{auth}Host: evil.example\r\n"), body).0,
        400
    );
    let mut stream = TcpStream::connect(address(&server)).unwrap();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {}\r\n{auth}Content-Type: application/json\r\nContent-Length: 999999999\r\n\r\n",
        address(&server)
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 413"));
}

#[test]
fn mcp_initialize_list_ping_notifications_and_errors() {
    let (_host, client) = ToolHost::new();
    let server = start(client).unwrap();
    let (_, response) = rpc(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}
        }}),
    );
    let response = response.unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        response["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    let (_, response) = rpc(
        &server,
        json!({"jsonrpc":"2.0","id":"list","method":"tools/list"}),
    );
    assert_eq!(response.unwrap()["result"]["tools"], super::super::tools());
    let (_, response) = rpc(&server, json!({"jsonrpc":"2.0","id":2,"method":"ping"}));
    assert_eq!(response.unwrap()["result"], json!({}));
    assert_eq!(
        rpc(
            &server,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .0,
        202
    );
    let (_, response) = rpc(&server, json!({"jsonrpc":"2.0","id":3,"method":"unknown"}));
    assert_eq!(response.unwrap()["error"]["code"], -32601);
    let (_, response) = rpc(
        &server,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"unknown"}}),
    );
    assert_eq!(response.unwrap()["error"]["code"], -32602);
    let (_, response) = raw(
        &server,
        &format!("Authorization: Bearer {}\r\n", server.token()),
        b"{",
    );
    assert_eq!(response.unwrap()["error"]["code"], -32700);
}

#[test]
fn http_calls_forward_to_scoped_live_host_and_wrap_tool_errors() {
    let (host, client) = ToolHost::new();
    let client = client.scoped("generation-fixed".into(), Arc::new(AtomicBool::new(false)));
    let server = Arc::new(start(client).unwrap());
    let copy = server.clone();
    let worker = thread::spawn(move || {
        rpc(
            &copy,
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"rpshop_read_project","arguments":{}}}),
        )
    });
    let request = next_request(&host);
    assert_eq!(request.scope, "generation-fixed");
    assert_eq!(request.call.name, "rpshop_read_project");
    request
        .reply
        .send(Ok(json!({"name":"Unsaved live project"})))
        .unwrap();
    let (_, response) = worker.join().unwrap();
    let response = response.unwrap();
    assert_eq!(
        response["result"]["structuredContent"]["name"],
        "Unsaved live project"
    );
    assert_eq!(response["result"]["isError"], false);
    let copy = server.clone();
    let worker = thread::spawn(move || {
        rpc(
            &copy,
            json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"rpshop_save"}}),
        )
    });
    let request = next_request(&host);
    request
        .reply
        .send(Err("User denied permission".into()))
        .unwrap();
    let (_, response) = worker.join().unwrap();
    assert_eq!(response.unwrap()["result"]["isError"], true);
}

#[test]
fn protocol_cancellation_reaches_received_gui_request() {
    let (host, client) = ToolHost::new();
    let server = Arc::new(start(client).unwrap());
    let copy = server.clone();
    let worker = thread::spawn(move || {
        rpc(
            &copy,
            json!({"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"rpshop_read_project"}}),
        )
    });
    let request = next_request(&host);
    assert_eq!(
        rpc(
            &server,
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":19}})
        )
        .0,
        202
    );
    let (_, response) = worker.join().unwrap();
    assert_eq!(response.unwrap()["result"]["isError"], true);
    assert!(request.cancelled.load(Ordering::Acquire));
}

#[test]
fn dropping_server_cancels_calls_and_slow_http_reads_and_closes_listener() {
    let (host, client) = ToolHost::new();
    let server = start(client).unwrap();
    let address = address(&server);
    let token = server.token().to_owned();
    let worker = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        let body=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rpshop_read_pages"}}).to_string();
        let header = format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(body.as_bytes()).unwrap();
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
    });
    let request = next_request(&host);
    let mut slow = TcpStream::connect(address).unwrap();
    slow.write_all(b"POST /mcp HTTP/1.1\r\n").unwrap();
    let start = Instant::now();
    drop(server);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(request.cancelled.load(Ordering::Acquire));
    assert!(TcpStream::connect(address).is_err());
    worker.join().unwrap();
}
