//! MCP stdio smoke test: initialize -> tools/list -> tools/call(list_catalog).
//! No Docker needed (list_catalog is a pure read).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps -> debug
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!(
        "portside-cli{}",
        std::env::consts::EXE_SUFFIX
    ))
}

fn send(stream: &mut impl Write, v: &Value) {
    writeln!(stream, "{v}").unwrap();
}

fn spawn() -> (std::process::Child, std::process::ChildStdin, BufReader<std::process::ChildStdout>) {
    let mut child = Command::new(bin())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn portside-cli mcp");
    let stdin = child.stdin.take().unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    (child, stdin, out)
}

#[test]
fn mcp_handshake_lists_eleven_tools_and_calls_catalog() {
    let (mut child, mut stdin, mut out) = spawn();
    let mut line = String::new();

    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
    );
    out.read_line(&mut line).unwrap();
    let init: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "portside");

    line.clear();
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }),
    );
    out.read_line(&mut line).unwrap();
    let list: Value = serde_json::from_str(&line).unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "list_instances",
        "create_instance",
        "get_connection",
        "create_database",
        "list_databases",
        "instance_health",
        "container_logs",
        "start_instance",
        "stop_instance",
        "remove_instance",
        "list_catalog",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    assert_eq!(names.len(), 11);

    line.clear();
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "list_catalog", "arguments": {} } }),
    );
    out.read_line(&mut line).unwrap();
    let call: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(call["result"]["isError"], false);
    assert!(
        call["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("postgres:17")
    );

    drop(stdin);
    child.wait().ok();
}

#[test]
fn mcp_unknown_tool_is_protocol_error() {
    let (mut child, mut stdin, mut out) = spawn();
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
            "params": { "name": "nope", "arguments": {} } }),
    );
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["error"]["code"], -32602);
    drop(stdin);
    child.wait().ok();
}
