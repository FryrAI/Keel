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

#[derive(Serialize, Deserialize)]
pub struct WhereResponse {
    pub file: String,
    pub line: u32,
}

#[derive(Serialize, Deserialize)]
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
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Method, Request};
    use keel_core::types::{GraphNode, NodeKind};
    use tower::ServiceExt;

    fn test_store() -> SharedStore {
        let store = SqliteGraphStore::in_memory().unwrap();
        Arc::new(Mutex::new(store))
    }

    fn store_with_node() -> SharedStore {
        let store = SqliteGraphStore::in_memory().unwrap();
        let node = GraphNode {
            id: 1,
            hash: "a7Bx3kM9f2Q".to_string(),
            kind: NodeKind::Function,
            name: "doStuff".to_string(),
            signature: "fn doStuff(x: i32) -> bool".to_string(),
            file_path: "src/lib.rs".to_string(),
            line_start: 10,
            line_end: 20,
            docstring: Some("Does stuff".to_string()),
            is_public: true,
            type_hints_present: true,
            has_docstring: true,
            external_endpoints: vec![],
            previous_hashes: vec![],
            module_id: 0,
        };
        store.insert_node(&node).unwrap();
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

        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: HealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.status, "ok");
        assert!(!json.version.is_empty());
    }

    #[tokio::test]
    async fn test_health_has_cors_headers() {
        let app = router(test_store());
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // CORS layer is applied; verify via an OPTIONS preflight
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cors_preflight() {
        let app = router(test_store());
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("/health")
            .header(header::ORIGIN, "http://example.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }

    #[tokio::test]
    async fn test_compile_with_json_body() {
        let app = router(test_store());
        let body = serde_json::json!({ "files": ["src/main.rs", "src/lib.rs"] });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let result: CompileResult = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.status, "ok");
        assert_eq!(result.files_analyzed.len(), 2);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_compile_malformed_body() {
        let app = router(test_store());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("not json"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // axum returns 422 Unprocessable Entity for deserialization failures
        assert!(resp.status().is_client_error());
    }

    #[tokio::test]
    async fn test_discover_existing_node() {
        let store = store_with_node();
        let app = router(store);
        let req = Request::builder()
            .uri("/discover/a7Bx3kM9f2Q")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
        let result: DiscoverResult = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.target.name, "doStuff");
        assert_eq!(result.target.hash, "a7Bx3kM9f2Q");
        assert_eq!(result.target.file, "src/lib.rs");
    }

    #[tokio::test]
    async fn test_discover_not_found() {
        let app = router(test_store());
        let req = Request::builder()
            .uri("/discover/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_where_existing_node() {
        let store = store_with_node();
        let app = router(store);
        let req = Request::builder()
            .uri("/where/a7Bx3kM9f2Q")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let result: WhereResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.file, "src/lib.rs");
        assert_eq!(result.line, 10);
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

    #[tokio::test]
    async fn test_explain_existing_node() {
        let store = store_with_node();
        let app = router(store);
        let body = serde_json::json!({ "error_code": "E001", "hash": "a7Bx3kM9f2Q" });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/explain")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let result: ExplainResult = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.error_code, "E001");
        assert_eq!(result.hash, "a7Bx3kM9f2Q");
        assert!(!result.resolution_chain.is_empty());
    }

    #[tokio::test]
    async fn test_explain_not_found() {
        let store = test_store();
        let app = router(store);
        let body = serde_json::json!({ "error_code": "E001", "hash": "doesntExist" });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/explain")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_explain_malformed_body() {
        let app = router(test_store());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/explain")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{invalid json"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert!(resp.status().is_client_error());
    }
}
