use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use keel_core::sqlite::SqliteGraphStore;
use keel_core::store::GraphStore;
use keel_core::types::EdgeDirection;
use keel_enforce::types::{
    CalleeInfo, CallerInfo, CompileInfo, CompileResult, DiscoverResult, ExplainResult,
    ModuleContext, NodeInfo, ResolutionStep,
};

type SharedStore = Arc<Mutex<SqliteGraphStore>>;

/// Build the axum router with all keel HTTP endpoints.
pub fn router(store: SharedStore) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/compile", post(compile))
        .route("/discover/{hash}", get(discover))
        .route("/where/{hash}", get(where_hash))
        .route("/explain", post(explain))
        .layer(cors)
        .with_state(store)
}

/// Start the HTTP server on the given port.
pub async fn serve(store: SharedStore, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(store);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// --- Request / Response types ---

#[derive(Deserialize)]
pub struct CompileRequest {
    pub files: Vec<String>,
}

#[derive(Deserialize)]
pub struct DiscoverQuery {
    pub depth: Option<u32>,
}

#[derive(Deserialize)]
pub struct ExplainRequest {
    pub error_code: String,
    pub hash: String,
}

#[derive(Serialize)]
pub struct WhereResponse {
    pub file: String,
    pub line: u32,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

// --- Handlers ---

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn compile(
    State(_store): State<SharedStore>,
    Json(req): Json<CompileRequest>,
) -> Json<CompileResult> {
    // Stub: return a clean compile result.
    // Real implementation will call EnforcementEngine::compile().
    Json(CompileResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "compile".to_string(),
        status: "ok".to_string(),
        files_analyzed: req.files,
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    })
}

async fn discover(
    State(store): State<SharedStore>,
    Path(hash): Path<String>,
    Query(query): Query<DiscoverQuery>,
) -> Result<Json<DiscoverResult>, StatusCode> {
    let store = store.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let node = store.get_node(&hash).ok_or(StatusCode::NOT_FOUND)?;
    let _depth = query.depth.unwrap_or(1);

    let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
    let outgoing = store.get_edges(node.id, EdgeDirection::Outgoing);

    let upstream: Vec<CallerInfo> = incoming
        .iter()
        .filter_map(|e| {
            store.get_node_by_id(e.source_id).map(|n| CallerInfo {
                hash: n.hash,
                name: n.name,
                signature: n.signature,
                file: n.file_path,
                line: n.line_start,
                docstring: n.docstring,
                call_line: e.line,
            })
        })
        .collect();

    let downstream: Vec<CalleeInfo> = outgoing
        .iter()
        .filter_map(|e| {
            store.get_node_by_id(e.target_id).map(|n| CalleeInfo {
                hash: n.hash,
                name: n.name,
                signature: n.signature,
                file: n.file_path,
                line: n.line_start,
                docstring: n.docstring,
                call_line: e.line,
            })
        })
        .collect();

    let module_context = build_module_context(&store, &node);

    Ok(Json(DiscoverResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: node.hash,
            name: node.name,
            signature: node.signature,
            file: node.file_path.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            docstring: node.docstring,
            type_hints_present: node.type_hints_present,
            has_docstring: node.has_docstring,
        },
        upstream,
        downstream,
        module_context,
    }))
}

fn build_module_context(
    store: &SqliteGraphStore,
    node: &keel_core::types::GraphNode,
) -> ModuleContext {
    if node.module_id == 0 {
        return ModuleContext {
            module: String::new(),
            sibling_functions: vec![],
            responsibility_keywords: vec![],
            function_count: 0,
            external_endpoints: vec![],
        };
    }
    let siblings = store.get_nodes_in_file(&node.file_path);
    let profile = store.get_module_profile(node.module_id);
    ModuleContext {
        module: profile
            .as_ref()
            .map(|p| p.path.clone())
            .unwrap_or_default(),
        sibling_functions: siblings.iter().map(|n| n.name.clone()).collect(),
        responsibility_keywords: profile
            .as_ref()
            .map(|p| p.responsibility_keywords.clone())
            .unwrap_or_default(),
        function_count: profile.as_ref().map(|p| p.function_count).unwrap_or(0),
        external_endpoints: vec![],
    }
}

async fn where_hash(
    State(store): State<SharedStore>,
    Path(hash): Path<String>,
) -> Result<Json<WhereResponse>, StatusCode> {
    let store = store.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let node = store.get_node(&hash).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(WhereResponse {
        file: node.file_path,
        line: node.line_start,
    }))
}

async fn explain(
    State(store): State<SharedStore>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ExplainResult>, StatusCode> {
    let store = store.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let node = store.get_node(&req.hash).ok_or(StatusCode::NOT_FOUND)?;

    // Stub: return a placeholder explain result.
    // Real implementation will use EnforcementEngine::explain().
    Ok(Json(ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: req.error_code,
        hash: node.hash.clone(),
        confidence: 1.0,
        resolution_tier: "tier1_treesitter".to_string(),
        resolution_chain: vec![ResolutionStep {
            kind: "lookup".to_string(),
            file: node.file_path,
            line: node.line_start,
            text: format!("Node '{}' found via hash lookup", node.name),
        }],
        summary: format!("Resolved '{}' via tree-sitter (Tier 1)", node.name),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_store() -> SharedStore {
        let store = SqliteGraphStore::in_memory().unwrap();
        Arc::new(Mutex::new(store))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = router(test_store());
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_where_not_found() {
        let app = router(test_store());
        let req = Request::builder()
            .uri("/where/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
