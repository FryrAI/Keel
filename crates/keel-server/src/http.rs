use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{FromRef, Path as AxumPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::{DiscoverResult, ExplainResult};
use keel_parsers::resolver::FileIndex;

pub type SharedEngine = Arc<Mutex<EnforcementEngine>>;

/// Router state: the shared engine plus the project root that compile targets
/// are confined to. `SharedEngine` is extractable on its own via [`FromRef`],
/// so handlers that don't touch the filesystem keep taking `State<SharedEngine>`.
#[derive(Clone)]
struct AppState {
    engine: SharedEngine,
    root: Arc<PathBuf>,
}

impl FromRef<AppState> for SharedEngine {
    fn from_ref(state: &AppState) -> Self {
        state.engine.clone()
    }
}

/// Build the axum router, confining compile targets to the current working
/// directory (where `keel serve` was launched — the project root).
pub fn router(engine: SharedEngine) -> Router {
    router_with_root(engine, current_root())
}

/// Build the axum router with an explicit project root (used by tests).
pub fn router_with_root(engine: SharedEngine, root: PathBuf) -> Router {
    let state = AppState {
        engine,
        root: Arc::new(root),
    };
    // No CORS layer: this is unauthenticated localhost tooling for the CLI and
    // the VS Code extension (neither is a browser bound by same-origin policy).
    // Allowing arbitrary cross-origin access would let any visited web page
    // read and mutate the code graph, so we simply don't permit it.
    Router::new()
        .route("/health", get(health))
        .route("/compile", post(compile))
        .route("/discover/{ident}", get(discover))
        .route("/where/{hash}", get(where_hash))
        .route("/explain", post(explain))
        .route("/map", get(map))
        .route("/search", get(search))
        .with_state(state)
}

/// The project root compile targets are confined to: the canonicalized current
/// working directory, falling back to `.` if it can't be resolved.
fn current_root() -> PathBuf {
    std::env::current_dir()
        .and_then(|d| d.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Start the HTTP server on the given port, bound to loopback only.
pub async fn serve(engine: SharedEngine, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(engine);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Acquire the shared engine lock, mapping a poisoned mutex to a 500 rather
/// than panicking — mirrors the MCP path's `lock_store`.
fn lock_engine(
    engine: &SharedEngine,
) -> Result<std::sync::MutexGuard<'_, EnforcementEngine>, StatusCode> {
    engine.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Request / Response types ---

/// Compile request. Accepts both call shapes the tooling uses:
/// `{"files":["a.rs","b.rs"]}` (explicit list) and `{"path":"src"}`
/// (a single file, or a directory to walk).
#[derive(Deserialize)]
pub struct CompileRequest {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct DiscoverQuery {
    pub depth: Option<u32>,
    /// When present, `{ident}` is resolved as a symbol name scoped to this file
    /// (name+position mode) rather than as a content hash.
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Deserialize)]
pub struct MapQuery {
    /// `llm` for a plain-text map, `json` (default) for structured output.
    pub format: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    /// Search term. Accepted as `q` (used by the extension) or `query`.
    pub q: Option<String>,
    pub query: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<usize>,
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

async fn compile(State(state): State<AppState>, Json(req): Json<CompileRequest>) -> Response {
    let targets = match collect_compile_targets(&req, &state.root) {
        Ok(t) => t,
        Err(code) => return code.into_response(),
    };

    let mut parser = FileParser::new();
    let file_indexes: Vec<FileIndex> = targets
        .iter()
        .filter_map(|path| parser.parse(path))
        .collect();

    let mut engine = match lock_engine(&state.engine) {
        Ok(g) => g,
        Err(code) => return code.into_response(),
    };
    Json(engine.compile(&file_indexes)).into_response()
}

/// Expand a compile request into a flat list of file paths: the explicit
/// `files`, plus `path` — walked into its supported files when it is a
/// directory, or used verbatim when it is a single file.
///
/// Every target is confined to `root`; an absolute-outside-root or `..`-escape
/// path aborts the whole request with `400 Bad Request` rather than being
/// read.
fn collect_compile_targets(req: &CompileRequest, root: &Path) -> Result<Vec<String>, StatusCode> {
    let mut targets = Vec::new();
    for file in &req.files {
        let confined = crate::http_confine::confine(root, file).ok_or(StatusCode::BAD_REQUEST)?;
        targets.push(confined.to_string_lossy().to_string());
    }
    if let Some(path) = &req.path {
        let confined = crate::http_confine::confine(root, path).ok_or(StatusCode::BAD_REQUEST)?;
        if confined.is_dir() {
            for entry in keel_parsers::walker::FileWalker::new(&confined).walk() {
                targets.push(entry.path.to_string_lossy().to_string());
            }
        } else {
            targets.push(confined.to_string_lossy().to_string());
        }
    }
    Ok(targets)
}

/// Discover a node's callers/callees.
///
/// - `/discover/{hash}` (no `file` query): hash lookup, returns the full
///   [`DiscoverResult`] — unchanged behavior.
/// - `/discover/{name}?file=&line=`: name+position mode, returns a flat
///   `{hash, name, callers, callees, module_context}` shape tailored to editor
///   hover/CodeLens clients.
async fn discover(
    State(engine): State<SharedEngine>,
    AxumPath(ident): AxumPath<String>,
    Query(query): Query<DiscoverQuery>,
) -> Result<Json<Value>, StatusCode> {
    let depth = query.depth.unwrap_or(1);
    let engine = lock_engine(&engine)?;

    if query.file.is_some() {
        let result = engine
            .discover_named(&ident, query.file.as_deref(), query.line, depth)
            .ok_or(StatusCode::NOT_FOUND)?;
        Ok(Json(flatten_discover(&result)))
    } else {
        let result = engine
            .discover(&ident, depth)
            .ok_or(StatusCode::NOT_FOUND)?;
        serde_json::to_value(result)
            .map(Json)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// Flatten a [`DiscoverResult`] into the compact shape editor clients parse:
/// top-level `hash`/`name`, `callers`/`callees` as `{name, file, line}`, and a
/// scalar `module_context`.
fn flatten_discover(result: &DiscoverResult) -> Value {
    let callers: Vec<Value> = result
        .upstream
        .iter()
        .map(|c| serde_json::json!({ "name": c.name, "file": c.file, "line": c.line }))
        .collect();
    let callees: Vec<Value> = result
        .downstream
        .iter()
        .map(|c| serde_json::json!({ "name": c.name, "file": c.file, "line": c.line }))
        .collect();

    serde_json::json!({
        "hash": result.target.hash,
        "name": result.target.name,
        "callers": callers,
        "callees": callees,
        "module_context": result.module_context.module,
    })
}

/// Graph-wide map. `?format=llm` returns a plain-text listing (for dumping into
/// an editor document); the default `json` returns a structured summary.
async fn map(State(engine): State<SharedEngine>, Query(query): Query<MapQuery>) -> Response {
    let engine = match lock_engine(&engine) {
        Ok(g) => g,
        Err(code) => return code.into_response(),
    };
    let modules = engine.module_map();
    let total_nodes: usize = modules.iter().map(|m| m.node_count).sum();

    if query.format.as_deref() == Some("llm") {
        let mut text = format!("MAP modules={} nodes={}\n", modules.len(), total_nodes);
        for m in &modules {
            text.push_str(&format!("MODULE {} nodes={}\n", m.file, m.node_count));
        }
        ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
    } else {
        let entries: Vec<Value> = modules
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name": m.name, "file": m.file, "node_count": m.node_count
                })
            })
            .collect();
        Json(serde_json::json!({
            "status": "ok",
            "format": "json",
            "module_count": modules.len(),
            "total_nodes": total_nodes,
            "modules": entries,
        }))
        .into_response()
    }
}

/// Search the graph by name substring: `/search?q=<term>&kind=&limit=`.
async fn search(
    State(engine): State<SharedEngine>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, StatusCode> {
    let term = query.q.or(query.query).ok_or(StatusCode::BAD_REQUEST)?;
    let limit = query.limit.unwrap_or(20);

    let engine = lock_engine(&engine)?;
    let nodes = engine.search_graph(&term, query.kind.as_deref(), limit);

    let results: Vec<Value> = nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "hash": n.hash,
                "name": n.name,
                "kind": n.kind.as_str(),
                "file": n.file_path,
                "line": n.line_start,
                "signature": n.signature,
                "is_public": n.is_public,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "query": term,
        "count": results.len(),
        "results": results,
    })))
}

async fn where_hash(
    State(engine): State<SharedEngine>,
    AxumPath(hash): AxumPath<String>,
) -> Result<Json<WhereResponse>, StatusCode> {
    let engine = lock_engine(&engine)?;
    engine
        .where_hash(&hash)
        .map(|(file, line)| Json(WhereResponse { file, line }))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn explain(
    State(engine): State<SharedEngine>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ExplainResult>, StatusCode> {
    let engine = lock_engine(&engine)?;
    engine
        .explain(&req.error_code, &req.hash)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// --- File parsing helper (delegated to parse_shared) ---

use crate::parse_shared::FileParser;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Method, Request};
    use keel_core::sqlite::SqliteGraphStore;
    use keel_core::types::GraphNode;
    use keel_enforce::types::CompileResult;
    use tower::ServiceExt;

    fn test_engine() -> SharedEngine {
        let store = SqliteGraphStore::in_memory().unwrap();
        let engine = EnforcementEngine::new(Box::new(store));
        Arc::new(Mutex::new(engine))
    }

    /// A canonicalized existing directory to use as a confinement root.
    fn canonical_temp_root() -> PathBuf {
        std::env::temp_dir()
            .canonicalize()
            .unwrap_or_else(|_| std::env::temp_dir())
    }

    fn test_engine_with_node() -> SharedEngine {
        let store = SqliteGraphStore::in_memory().unwrap();
        store
            .insert_node(&GraphNode {
                id: 1,
                hash: "abc12345678".to_string(),
                kind: keel_core::types::NodeKind::Function,
                name: "handleRequest".to_string(),
                signature: "fn handleRequest(req: Request) -> Response".to_string(),
                file_path: "src/handler.rs".to_string(),
                line_start: 5,
                line_end: 20,
                docstring: Some("Handles requests".to_string()),
                is_public: true,
                type_hints_present: true,
                has_docstring: true,
                external_endpoints: vec![],
                previous_hashes: vec![],
                module_id: 0,
                package: None,
            })
            .unwrap();
        let engine = EnforcementEngine::new(Box::new(store));
        Arc::new(Mutex::new(engine))
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = router(test_engine());
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
    async fn test_no_wildcard_cors() {
        // The server must NOT hand a cross-origin allow header to a browser:
        // any visited web page could otherwise read/mutate the graph.
        let app = router(test_engine());
        let req = Request::builder()
            .uri("/health")
            .header(header::ORIGIN, "http://evil.example.com")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let allow = resp
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok());
        assert!(
            allow != Some("*") && allow != Some("http://evil.example.com"),
            "unexpected CORS allow-origin: {allow:?}"
        );
    }

    #[tokio::test]
    async fn test_compile_rejects_parent_escape() {
        let app = router_with_root(test_engine(), canonical_temp_root());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"path":"../../etc/passwd"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_compile_rejects_absolute_outside_root() {
        let app = router_with_root(test_engine(), canonical_temp_root());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"files":["/etc/passwd"]}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_poisoned_engine_returns_500() {
        let engine = test_engine();
        // Poison the mutex by panicking while it is held.
        let poisoner = engine.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        let app = router(engine);
        let req = Request::builder().uri("/map").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_compile_empty_files() {
        let app = router(test_engine());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"files":[]}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let result: CompileResult = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.status, "ok");
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_discover_not_found() {
        let app = router(test_engine());
        let req = Request::builder()
            .uri("/discover/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_discover_found() {
        let app = router(test_engine_with_node());
        let req = Request::builder()
            .uri("/discover/abc12345678")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let result: DiscoverResult = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.target.name, "handleRequest");
        assert_eq!(result.target.hash, "abc12345678");
    }

    #[tokio::test]
    async fn test_compile_accepts_path() {
        // The extension POSTs {"path": ...} rather than {"files": [...]}.
        let app = router(test_engine());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/compile")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"path":"does/not/exist.rs"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let result: CompileResult = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.status, "ok");
    }

    #[tokio::test]
    async fn test_discover_by_name_flat_shape() {
        let app = router(test_engine_with_node());
        let req = Request::builder()
            .uri("/discover/handleRequest?file=src/handler.rs&line=5")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["hash"], "abc12345678");
        assert_eq!(json["name"], "handleRequest");
        assert!(json["callers"].is_array());
        assert!(json["callees"].is_array());
    }

    #[tokio::test]
    async fn test_map_llm_is_text() {
        let app = router(test_engine());
        let req = Request::builder()
            .uri("/map?format=llm")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let content_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("text/plain"));
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        assert!(String::from_utf8_lossy(&body).starts_with("MAP modules="));
    }

    #[tokio::test]
    async fn test_search_requires_term() {
        let app = router(test_engine());
        let req = Request::builder()
            .uri("/search")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_where_not_found() {
        let app = router(test_engine());
        let req = Request::builder()
            .uri("/where/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_where_found() {
        let app = router(test_engine_with_node());
        let req = Request::builder()
            .uri("/where/abc12345678")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let result: WhereResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.file, "src/handler.rs");
        assert_eq!(result.line, 5);
    }

    #[tokio::test]
    async fn test_explain_not_found() {
        let app = router(test_engine());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/explain")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error_code":"E001","hash":"nonexistent"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_explain_found() {
        let app = router(test_engine_with_node());
        let req = Request::builder()
            .method(Method::POST)
            .uri("/explain")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error_code":"E001","hash":"abc12345678"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
        let result: ExplainResult = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.error_code, "E001");
        assert_eq!(result.hash, "abc12345678");
        assert_eq!(result.resolution_tier, "tree-sitter");
    }

    #[test]
    fn test_detect_language() {
        use crate::parse_shared::detect_language;
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
        assert_eq!(detect_language("lib/index.ts"), Some("typescript"));
        assert_eq!(detect_language("app.py"), Some("python"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("README.md"), None);
    }
}
