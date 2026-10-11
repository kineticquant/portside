//! MCP server over stdio for `portside-cli mcp`.
//!
//! Hand-rolled newline-delimited JSON-RPC 2.0 on serde_json+tokio (zero new
//! protocol deps): `initialize`, `tools/list`, `tools/call`. stdout is
//! protocol only — all logging goes to stderr.

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Req {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn ok(id: &Value, result: Value) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result })).unwrap()
}

fn err(id: &Value, code: i32, message: &str) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id,
        "error": { "code": code, "message": message } }))
    .unwrap()
}

fn tool(name: &str, desc: &str, schema: Value) -> Value {
    json!({ "name": name, "description": desc, "inputSchema": schema })
}

fn obj(props: &[(&str, &str)], required: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = props
        .iter()
        .map(|(k, t)| (k.to_string(), json!({ "type": t })))
        .collect();
    json!({ "type": "object", "properties": properties, "required": required })
}

pub(crate) fn tools_list() -> Value {
    json!({ "tools": [
        tool("list_instances", "List Portside database instances", obj(&[], &[])),
        tool("list_catalog", "Show available engine versions", obj(&[], &[])),
        tool("create_instance",
            "Create a database instance; port 0 = auto; waits until healthy and returns connect URLs",
            obj(&[("engine","string"),("tag","string"),("port","integer"),("lan","boolean"),("password","string"),("wait","integer")], &["engine"])),
        tool("get_connection", "Connect strings for an instance (localhost + LAN)",
            obj(&[("id","string"),("lan","boolean"),("strict","boolean")], &["id"])),
        tool("create_database", "CREATE DATABASE (SQL engines)",
            obj(&[("id","string"),("name","string")], &["id","name"])),
        tool("list_databases", "List databases in an instance",
            obj(&[("id","string")], &["id"])),
        tool("instance_health", "Container/tcp/query health; optional wait seconds",
            obj(&[("id","string"),("wait","integer")], &["id"])),
        tool("container_logs", "Tail container logs",
            obj(&[("id","string"),("tail","integer")], &["id"])),
        tool("start_instance", "Start a stopped instance",
            obj(&[("id","string")], &["id"])),
        tool("stop_instance", "Stop a running instance",
            obj(&[("id","string")], &["id"])),
        tool("remove_instance",
            "Remove container; wipe=true also drops the data volume (irreversible)",
            obj(&[("id","string"),("wipe","boolean")], &["id"])),
    ]})
}

fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

fn tool_error(msg: String) -> Value {
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true })
}

/// Run one tool. `Ok` = result object (possibly `isError`), `Err` = unknown
/// tool (protocol error -32602). Core-call failures are tool errors, not
/// protocol errors.
async fn call_tool(name: &str, a: &Value) -> Result<Value, String> {
    let s = |k: &str| {
        a.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let n = |k: &str, d: u64| a.get(k).and_then(|v| v.as_u64()).unwrap_or(d);
    match name {
        "list_instances" => match portside::docker::list_instances().await {
            Ok(v) => Ok(text_result(serde_json::to_string(&v).unwrap())),
            Err(e) => Ok(tool_error(e)),
        },
        "list_catalog" => Ok(text_result(
            serde_json::to_string(&portside::catalog::bundled_catalog()).unwrap(),
        )),
        "create_instance" => {
            let engine = s("engine");
            if engine.is_empty() {
                return Ok(tool_error("engine is required".to_string()));
            }
            let port_raw = n("port", 0);
            if port_raw > 65535 {
                return Ok(tool_error("port must be 0-65535 (0 = auto)".to_string()));
            }
            let (tag, port) =
                match crate::cmds::resolve_create(&engine, &s("tag"), port_raw as u16) {
                    Ok(v) => v,
                    Err(e) => return Ok(tool_error(e)),
                };
            let lan = b("lan");
            let bind = if lan { "0.0.0.0" } else { "127.0.0.1" }.to_string();
            let inst = match portside::docker::create_instance(
                engine,
                tag,
                port,
                Some(bind),
                Some(s("password")),
            )
            .await
            {
                Ok(i) => i,
                Err(e) => return Ok(tool_error(e)),
            };
            let wait = n("wait", 60);
            if wait > 0 {
                if let Err(e) = portside::health::wait_healthy(&inst.id, wait).await {
                    return Ok(tool_error(format!("created {} but {e}", inst.id)));
                }
            }
            let fresh = portside::state::load_instance(&inst.id).unwrap_or(inst);
            let lan_ip = if lan { portside::certs::lan_ip() } else { None };
            let info = portside::connect::for_instance(&fresh, lan_ip.as_deref(), false);
            Ok(text_result(
                serde_json::to_string(&json!({
                    "id": fresh.id, "engine": fresh.engine, "tag": fresh.tag,
                    "port": fresh.port, "status": fresh.status,
                    "localhost_url": info.localhost_url, "lan_url": info.lan_url,
                    "tls": info.tls,
                    "summary": format!("{} {}:{} ready — {}",
                        fresh.engine, fresh.tag, fresh.port, info.localhost_url),
                }))
                .unwrap(),
            ))
        }
        "get_connection" => {
            let id = s("id");
            if id.is_empty() {
                return Ok(tool_error("id is required".to_string()));
            }
            match portside::state::load_instance(&id) {
                Err(e) => Ok(tool_error(e)),
                Ok(inst) => {
                    let lan = b("lan");
                    let lan_ip = if lan { portside::certs::lan_ip() } else { None };
                    if lan && lan_ip.is_none() {
                        return Ok(tool_error(
                            "no LAN address detected (offline?)".to_string(),
                        ));
                    }
                    let info =
                        portside::connect::for_instance(&inst, lan_ip.as_deref(), b("strict"));
                    Ok(text_result(serde_json::to_string(&info).unwrap()))
                }
            }
        }
        "create_database" => {
            match portside::schemas::create_database(s("id"), s("name")).await {
                Ok(()) => Ok(text_result("{\"ok\":true}".to_string())),
                Err(e) => Ok(tool_error(e)),
            }
        }
        "list_databases" => match portside::schemas::list_databases(s("id")).await {
            Ok(v) => Ok(text_result(serde_json::to_string(&v).unwrap())),
            Err(e) => Ok(tool_error(e)),
        },
        "instance_health" => {
            let id = s("id");
            let wait = n("wait", 0);
            let h = if wait > 0 {
                portside::health::wait_healthy(&id, wait).await
            } else {
                portside::health::health(id).await
            };
            match h {
                Ok(v) => Ok(text_result(serde_json::to_string(&v).unwrap())),
                Err(e) => Ok(tool_error(e)),
            }
        }
        "container_logs" => {
            let tail = n("tail", 200) as usize;
            match portside::health::container_logs(s("id"), Some(tail)).await {
                Ok(logs) => Ok(text_result(serde_json::to_string(&json!({ "logs": logs })).unwrap())),
                Err(e) => Ok(tool_error(e)),
            }
        }
        "start_instance" => match portside::docker::start_instance(s("id")).await {
            Ok(()) => Ok(text_result("{\"ok\":true}".to_string())),
            Err(e) => Ok(tool_error(e)),
        },
        "stop_instance" => match portside::docker::stop_instance(s("id")).await {
            Ok(()) => Ok(text_result("{\"ok\":true}".to_string())),
            Err(e) => Ok(tool_error(e)),
        },
        "remove_instance" => {
            match portside::docker::remove_instance(s("id"), b("wipe")).await {
                Ok(()) => Ok(text_result("{\"ok\":true}".to_string())),
                Err(e) => Ok(tool_error(e)),
            }
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

async fn handle(req: Req) -> Option<String> {
    let null = Value::Null;
    let id = req.id.as_ref().unwrap_or(&null);
    match req.method.as_str() {
        "initialize" => Some(ok(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "portside", "version": env!("CARGO_PKG_VERSION") }
            }),
        )),
        "tools/list" => Some(ok(id, tools_list())),
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));
            match call_tool(name, &args).await {
                Ok(v) => Some(ok(id, v)),
                Err(e) => Some(err(id, -32602, &e)),
            }
        }
        m if m.starts_with("notifications/") => None,
        _ if req.id.is_none() => None, // unknown notification: swallow
        _ => Some(err(id, -32601, "method not found")),
    }
}

pub async fn serve() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let resp: Option<String> = match serde_json::from_str::<Req>(&line) {
            Err(_) => Some(
                serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": Value::Null,
                    "error": { "code": -32700, "message": "parse error" } }))
                .unwrap(),
            ),
            Ok(req) => handle(req).await,
        };
        if let Some(r) = resp {
            stdout.write_all(r.as_bytes()).await.ok();
            stdout.write_all(b"\n").await.ok();
            stdout.flush().await.ok();
        }
    }
}
