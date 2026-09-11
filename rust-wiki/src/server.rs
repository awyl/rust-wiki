//! MCP streamable-HTTP server: a small, explicit JSON-RPC surface over axum.
//!
//! We hand-roll the RPC layer (initialize / tools/list / tools/call) because
//! the contract is tiny and fully under our control; the rmcp-style macros
//! would hide the wire format this server is tested against.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::WikiApi;
use crate::hub::Hub;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SERVER_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

type Shared = Arc<Hub>;

// ---------- JSON-RPC envelope ----------

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)] // envelope conformity; we never branch on it
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    #[allow(dead_code)] // serialized to the wire, never read in-process
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i64,
    message: String,
}

fn rpc_ok(id: Option<Value>, result: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}
pub fn rpc_err(id: Option<Value>, code: i64, message: String) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message }),
    }
}

// ---------- tool catalog ----------

/// (name, description, inputSchema) — explicit wire contract.
/// Page types the creation tools accept, read from the vault's single source
/// of truth so the published schema cannot drift from `pages::PAGE_TYPES`.
fn page_type_enum() -> Value {
    Value::Array(
        crate::vault::pages::PAGE_TYPES
            .iter()
            .map(|(t, _)| Value::String((*t).to_string()))
            .collect(),
    )
}

fn tools() -> &'static [(&'static str, &'static str, Value)] {
    static TOOLS: std::sync::OnceLock<Vec<(&'static str, &'static str, Value)>> =
        std::sync::OnceLock::new();
    TOOLS.get_or_init(|| vec![
    ("wiki_bootstrap", "Create this project's wiki vault (one-time).", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string", "description": "Space name (git-derived, e.g. my-project-abc1234)"},
            "topic": {"type": "string"}
        },
        "required": ["space"]
    })),
    ("wiki_use_space", "Pin the wiki space for this connection. Call once at session start.", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}},
        "required": ["space"]
    })),
    ("wiki_capture_source", "Capture text or a URL as an immutable source packet.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "text": {"type": "string"},
            "url": {"type": "string"},
            "file_path": {"type": "string", "description": "Server-local path"},
            "title": {"type": "string"}
        }
    })),
    ("wiki_ingest", "Get the next batch of uningested sources to synthesize. Pass mark_ingested after synthesizing a source.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "source_id": {"type": "string"},
            "batch_size": {"type": "number"},
            "mark_ingested": {"type": "array", "items": {"type": "string"}}
        }
    })),
    ("wiki_ensure_page", "Create a wiki page of the given type (no overwrite).", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "type": {"type": "string", "enum": page_type_enum()},
            "title": {"type": "string"},
            "content": {"type": "string"}
        },
        "required": ["type", "title"]
    })),
    ("wiki_template", "Get the authoritative page template for a type ({date} filled, {title} placeholder). Scaffold from this, never from frozen copies.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "type": {"type": "string", "enum": page_type_enum()}
        },
        "required": ["type"]
    })),
    ("wiki_read_page", "Read a wiki page by id (e.g. concepts/rag).", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}, "id": {"type": "string"}},
        "required": ["id"]
    })),
    ("wiki_write_page", "Update an existing wiki page (guardrailed, metadata auto-rebuilds).", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}, "id": {"type": "string"}, "content": {"type": "string"}},
        "required": ["id", "content"]
    })),
    ("wiki_delete_page", "Delete a wiki page (irreversible). Requires explicit user approval, refuses while the page is still linked unless force is set, and is disabled unless the operator enabled allow_delete.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "id": {"type": "string"},
            "confirm": {"type": "string", "description": "Repeat the exact id being deleted — a deliberate second confirmation."},
            "force": {"type": "boolean", "description": "Delete even though other pages still link to it (leaves those links dangling)."}
        },
        "required": ["id", "confirm"]
    })),
    ("wiki_write_personal_page", "Update an existing page in the PERSONAL/root layer (cross-project global wiki). The only write path to root — no space switch needed.", json!({
        "type": "object",
        "properties": {"id": {"type": "string"}, "content": {"type": "string"}},
        "required": ["id", "content"]
    })),
    ("wiki_ensure_personal_page", "Create a wiki page of the given type in the PERSONAL/root layer (no overwrite). No space switch needed.", json!({
        "type": "object",
        "properties": {
            "type": {"type": "string", "enum": page_type_enum()},
            "title": {"type": "string"},
            "content": {"type": "string"}
        },
        "required": ["type", "title"]
    })),
    ("wiki_capture_trajectory", "Capture a completed task's tool-call record + summary as an immutable trajectory packet plus a skeleton case page. Steps are caller-supplied.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "title": {"type": "string"},
            "outcome": {"type": "string", "enum": ["success", "failure", "partial"]},
            "steps": {"type": "array"},
            "summary": {"type": "string"}
        },
        "required": ["title", "steps", "summary"]
    })),
    ("wiki_distill_skills", "List undistilled trajectories for skill synthesis. Pass mark_distilled after generalizing a packet into a skill page.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "mark_distilled": {"type": "array", "items": {"type": "string"}}
        }
    })),
    ("wiki_recall_skill", "Layered recall filtered to distilled skill/case pages — have I done this before?", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "query": {"type": "string"},
            "kind": {"type": "string", "enum": ["skill", "case", "any"]},
            "max_results": {"type": "number"}
        },
        "required": ["query"]
    })),
    ("wiki_recall", "Layered relevance search (active space + personal layer) for task context.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "query": {"type": "string"},
            "max_results": {"type": "number"}
        },
        "required": ["query"]
    })),
    ("wiki_search", "Exact keyword lookup in the registry.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "query": {"type": "string"},
            "type": {"type": "string"}
        },
        "required": ["query"]
    })),
    ("wiki_retro", "Save an atomic insight from a completed task.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "slug": {"type": "string"},
            "title": {"type": "string"},
            "body": {"type": "string"},
            "category": {"type": "string"}
        },
        "required": ["slug", "title", "body"]
    })),
    ("wiki_observe", "Record a timestamped observation during a session.", json!({
        "type": "object",
        "properties": {
            "space": {"type": "string"},
            "title": {"type": "string"},
            "content": {"type": "string"},
            "relevance": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
            "tags": {"type": "string"},
            "source_context": {"type": "string"}
        },
        "required": ["title", "content", "relevance"]
    })),
    ("wiki_lint", "Health check: orphans, missing pages, contradictions, gaps. Optional auto_fix.", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}, "auto_fix": {"type": "boolean"}}
    })),
        ("wiki_status", "Wiki stats and health verdict, plus the running server version.", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}}
    })),
    ("wiki_rebuild_meta", "Force registry/backlinks/index rebuild.", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}}
    })),
    ("wiki_watch", "Built-in maintenance scheduler. No args = status (interval, enabled). {run: true} = run one mechanical maintenance cycle (lint + auto_fix + status) across all spaces now and return the reports.", json!({
        "type": "object",
        "properties": {"run": {"type": "boolean", "description": "Run an immediate maintenance cycle across all spaces"}}
    })),
    ("wiki_reindex_embeddings", "Re-embed all pages for semantic recall (no-op message when no embedding provider is configured).", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}}
    })),
    ("wiki_log_event", "Append a structured event to the activity stream.", json!({
        "type": "object",
        "properties": {"space": {"type": "string"}, "kind": {"type": "string"}, "details": {"type": "object"}},
        "required": ["kind"]
    })),
])
}

fn text_result(v: &impl Serialize, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}],
        "isError": is_error
    })
}

// ---------- dispatch ----------

/// Transport-free JSON-RPC handling: shared by the axum HTTP route and the
/// stdio loop. Returns `None` for notifications (no response on the wire).
pub fn handle_json_rpc(hub: &Hub, conn: &str, req: &RpcRequest) -> Option<RpcResponse> {
    if req.method.starts_with("notifications/") {
        return None;
    }
    let id = req.id.clone();
    let response = match req.method.as_str() {
        "initialize" => rpc_ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "rust-wiki", "version": SERVER_VERSION}
            }),
        ),
        "ping" => rpc_ok(id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = tools()
                .iter()
                .map(|(name, desc, schema)| json!({"name": name, "description": desc, "inputSchema": schema}))
                .collect();
            rpc_ok(id, json!({"tools": tools}))
        }
        "tools/call" => {
            let params = req.params.clone().unwrap_or(json!({}));
            let name = params["name"].as_str().unwrap_or("");
            let args = params["arguments"].clone();
            let space = arg_str(&args, "space");
            let result = dispatch(hub, conn, name, space.as_deref(), &args);
            match result {
                Ok(v) => rpc_ok(id, text_result(&v, false)),
                Err(e) => rpc_ok(
                    id,
                    json!({
                        "content": [{"type": "text", "text": format!("{}: {}", e.code, e.message)}],
                        "isError": true
                    }),
                ),
            }
        }
        other => rpc_err(id, -32601, format!("method not found: {other}")),
    };
    Some(response)
}

async fn handle_rpc(
    State(hub): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Json<RpcResponse> {
    let conn = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".into());
    Json(handle_json_rpc(hub.as_ref(), &conn, &req).unwrap_or_else(|| rpc_ok(None, json!({}))))
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args[key].as_str().map(|s| s.to_string())
}

fn dispatch(
    hub: &Hub,
    conn: &str,
    name: &str,
    space: Option<&str>,
    args: &Value,
) -> Result<Value, crate::api::ApiError> {
    macro_rules! need_space {
        () => {
            space.ok_or_else(|| {
                crate::api::ApiError::invalid(
                    "missing 'space' argument (or call wiki_use_space first)",
                )
            })?
        };
    }
    match name {
        "wiki_bootstrap" => {
            let s = space.ok_or_else(|| crate::api::ApiError::invalid("missing 'space'"))?;
            Ok(serde_json::to_value(
                hub.bootstrap(s, arg_str(args, "topic").as_deref())?,
            )?)
        }
        "wiki_use_space" => {
            let s = arg_str(args, "space")
                .ok_or_else(|| crate::api::ApiError::invalid("missing 'space'"))?;
            Ok(serde_json::to_value(hub.use_space(conn, &s)?)?)
        }
        "wiki_capture_source" => Ok(serde_json::to_value(hub.capture_source(
            need_space!(),
            args["text"].as_str(),
            args["url"].as_str(),
            args["file_path"].as_str(),
            args["title"].as_str(),
        )?)?),
        "wiki_ingest" => {
            let marks: Vec<String> = args["mark_ingested"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Ok(serde_json::to_value(hub.ingest(
                need_space!(),
                args["source_id"].as_str(),
                args["batch_size"].as_u64().map(|n| n as u32),
                &marks,
            )?)?)
        }
        "wiki_ensure_page" => Ok(serde_json::to_value(hub.ensure_page(
            need_space!(),
            args["type"].as_str().unwrap_or(""),
            args["title"].as_str().unwrap_or(""),
            args["content"].as_str(),
        )?)?),
        "wiki_read_page" => Ok(serde_json::to_value(
            hub.read_page(need_space!(), args["id"].as_str().unwrap_or(""))?,
        )?),
        "wiki_template" => Ok(serde_json::to_value(
            hub.template(need_space!(), args["type"].as_str().unwrap_or(""))?,
        )?),
        "wiki_capture_trajectory" => Ok(serde_json::to_value(hub.capture_trajectory(
            need_space!(),
            args["title"].as_str().unwrap_or(""),
            args["outcome"].as_str(),
            &args["steps"].clone(),
            args["summary"].as_str().unwrap_or(""),
        )?)?),
        "wiki_distill_skills" => {
            let marks: Vec<String> = args["mark_distilled"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Ok(serde_json::to_value(
                hub.distill_skills(need_space!(), &marks)?,
            )?)
        }
        "wiki_recall_skill" => Ok(serde_json::to_value(hub.recall_skill(
            need_space!(),
            args["query"].as_str().unwrap_or(""),
            args["kind"].as_str(),
            args["max_results"].as_u64().map(|n| n as u32),
        )?)?),
        "wiki_write_page" => Ok(serde_json::to_value(hub.write_page(
            need_space!(),
            args["id"].as_str().unwrap_or(""),
            args["content"].as_str().unwrap_or(""),
        )?)?),
        "wiki_delete_page" => Ok(serde_json::to_value(hub.delete_page(
            need_space!(),
            args["id"].as_str().unwrap_or(""),
            args["confirm"].as_str().unwrap_or(""),
            args["force"].as_bool().unwrap_or(false),
        )?)?),
        "wiki_write_personal_page" => Ok(serde_json::to_value(hub.write_page(
            crate::vault::layout::SPACE_PERSONAL,
            args["id"].as_str().unwrap_or(""),
            args["content"].as_str().unwrap_or(""),
        )?)?),
        "wiki_ensure_personal_page" => Ok(serde_json::to_value(hub.ensure_page(
            crate::vault::layout::SPACE_PERSONAL,
            args["type"].as_str().unwrap_or(""),
            args["title"].as_str().unwrap_or(""),
            args["content"].as_str(),
        )?)?),
        "wiki_recall" => Ok(serde_json::to_value(hub.recall(
            need_space!(),
            args["query"].as_str().unwrap_or(""),
            args["max_results"].as_u64().map(|n| n as u32),
        )?)?),
        "wiki_search" => Ok(serde_json::to_value(hub.search(
            need_space!(),
            args["query"].as_str().unwrap_or(""),
            args["type"].as_str(),
        )?)?),
        "wiki_status" => Ok(serde_json::to_value(hub.status(need_space!())?)?),
        "wiki_lint" => Ok(serde_json::to_value(
            hub.lint(need_space!(), args["auto_fix"].as_bool().unwrap_or(false))?,
        )?),
        "wiki_retro" => Ok(serde_json::to_value(hub.retro(
            need_space!(),
            args["slug"].as_str().unwrap_or(""),
            args["title"].as_str().unwrap_or(""),
            args["body"].as_str().unwrap_or(""),
            args["category"].as_str(),
        )?)?),
        "wiki_observe" => Ok(serde_json::to_value(hub.observe(
            need_space!(),
            args["title"].as_str().unwrap_or(""),
            args["content"].as_str().unwrap_or(""),
            args["relevance"].as_str().unwrap_or("medium"),
            args["tags"].as_str(),
            args["source_context"].as_str(),
        )?)?),
        "wiki_log_event" => {
            let details = args.get("details").cloned().unwrap_or(json!({}));
            Ok(serde_json::to_value(hub.log_event(
                need_space!(),
                args["kind"].as_str().unwrap_or(""),
                &details,
            )?)?)
        }
        "wiki_reindex_embeddings" => Ok(serde_json::to_value(hub.reembed(need_space!())?)?),
        "wiki_watch" => {
            if args["run"].as_bool() == Some(true) {
                let reports = crate::vault::watch::run_all_spaces(hub.root());
                Ok(json!({ "ran": reports.len(), "reports": reports }))
            } else {
                let interval = crate::vault::watch::interval_from_env();
                Ok(json!({
                    "enabled": interval > 0,
                    "interval_secs": interval,
                    "default_interval_secs": crate::vault::watch::DEFAULT_INTERVAL_SECS
                }))
            }
        }
        "wiki_rebuild_meta" => {
            // lint(false) rebuilds + reports; a pure rebuild: status computes from fresh registry
            let v = hub.lint(need_space!(), false)?;
            Ok(json!({"pages": v.pages, "rebuilt": true}))
        }
        other => Err(crate::api::ApiError::new(
            "unknown_tool",
            format!("unknown tool '{other}'"),
        )),
    }
}

pub fn router(hub: Hub) -> Router {
    Router::new()
        .route("/mcp", post(handle_rpc))
        .with_state(Arc::new(hub))
}

pub async fn serve(hub: Hub, addr: SocketAddr) -> anyhow::Result<()> {
    let app = router(hub);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("rust-wiki listening on http://{addr}/mcp");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// The published schema must offer every type the vault can create, or an
    /// agent is told a page type does not exist when it does.
    #[test]
    fn creation_tools_offer_every_page_type() {
        let want: Vec<&str> = crate::vault::pages::PAGE_TYPES
            .iter()
            .map(|(t, _)| *t)
            .collect();
        for name in [
            "wiki_ensure_page",
            "wiki_ensure_personal_page",
            "wiki_template",
        ] {
            let (_, _, schema) = tools().iter().find(|(n, _, _)| *n == name).unwrap();
            let got: Vec<&str> = schema["properties"]["type"]["enum"]
                .as_array()
                .unwrap_or_else(|| panic!("{name} has no type enum"))
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(got, want, "{name} type enum");
        }
        assert!(want.contains(&"retro"), "retros must be creatable");
    }

    #[test]
    fn stdio_handler_initialize_and_notification_suppression() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = crate::hub::Hub::new(tmp.path().to_path_buf());
        // initialize -> Some(response)
        let init = RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "initialize".into(),
            params: None,
        };
        let resp = handle_json_rpc(&hub, "stdio", &init).unwrap();
        assert_eq!(resp.id, Some(serde_json::json!(1)));
        // notification -> None (suppressed on the wire)
        let note = RpcRequest {
            jsonrpc: "2.0".into(),
            id: None,
            method: "notifications/initialized".into(),
            params: None,
        };
        assert!(handle_json_rpc(&hub, "stdio", &note).is_none());
    }

    use super::*;
    use crate::hub::UrlFetcher;

    struct StaticFetcher;
    impl UrlFetcher for StaticFetcher {
        fn fetch_markdown(&self, _url: &str) -> Result<String, String> {
            Ok("# Fetched\n\nbody\n".into())
        }
    }

    fn test_hub(tmp: &std::path::Path) -> Hub {
        Hub::with_injections(
            tmp.to_path_buf(),
            Box::new(StaticFetcher),
            std::sync::Arc::new(crate::vault::convert::DefaultConverter),
            Box::new(|| "2026-09-07T12:00:00Z".into()),
        )
    }

    async fn rpc(client: &reqwest::Client, url: &str, body: Value) -> Value {
        let resp = client.post(url).json(&body).send().await.unwrap();
        resp.json::<Value>().await.unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn full_wire_flow() {
        let tmp = tempfile::tempdir().unwrap();
        let app = router(test_hub(tmp.path()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let url = format!("http://{addr}/mcp");
        let client = reqwest::Client::new();

        // initialize
        let r = rpc(
            &client,
            &url,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .await;
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);

        // tools/list
        let r = rpc(
            &client,
            &url,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .await;
        assert_eq!(
            r["result"]["tools"].as_array().unwrap().len(),
            tools().len()
        );

        // bootstrap + use_space
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"wiki_bootstrap","arguments":{"space":"proj"}}})).await;
        assert_eq!(r["result"]["isError"], false);
        let created = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(created.contains("\"created\": true"));

        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"wiki_use_space","arguments":{"space":"proj"}}})).await;
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("proj"));

        // capture + retro via wire
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"wiki_capture_source","arguments":{"space":"proj","url":"https://ex.com/x"}}})).await;
        assert_eq!(r["result"]["isError"], false);

        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"wiki_retro","arguments":{"space":"proj","slug":"port-note","title":"Port note","body":"used axum"}}})).await;
        assert_eq!(r["result"]["isError"], false);

        // recall finds retro
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"wiki_recall","arguments":{"space":"proj","query":"axum"}}})).await;
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("sources/port-note"));

        // error surfaces as isError:true
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"wiki_read_page","arguments":{"space":"proj","id":"concepts/missing"}}})).await;
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not_found"));

        // personal tools: no space switch, write to root layer
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"wiki_bootstrap","arguments":{"space":"personal"}}})).await;
        assert_eq!(r["result"]["isError"], false);
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"wiki_ensure_personal_page","arguments":{"type":"concept","title":"Global Note","content":"---\ntitle: \"Global Note\"\ntype: concept\n---\n\nshared\n"}}})).await;
        assert_eq!(r["result"]["isError"], false);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("concepts/global-note"));
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"wiki_write_personal_page","arguments":{"id":"concepts/global-note","content":"---\ntitle: \"Global Note\"\ntype: concept\n---\n\nshared v2\n"}}})).await;
        assert_eq!(r["result"]["isError"], false);
        // project read of personal id stays scoped: not visible in proj
        let r = rpc(&client, &url, json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"wiki_read_page","arguments":{"space":"proj","id":"concepts/global-note"}}})).await;
        assert_eq!(r["result"]["isError"], true);

        // unknown method
        let r = rpc(
            &client,
            &url,
            json!({"jsonrpc":"2.0","id":13,"method":"nope"}),
        )
        .await;
        assert_eq!(r["error"]["code"], -32601);
    }
}
