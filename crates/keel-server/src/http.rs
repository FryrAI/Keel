use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};
use keel_parsers::resolver::{FileIndex, LanguageResolver};

pub type SharedEngine = Arc<Mutex<EnforcementEngine>>;

/// Build the axum router with all keel HTTP endpoints.
pub fn router(engine: SharedEngine) -> Router {
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
        .with_state(engine)
}

/// Start the HTTP server on the given port.
pub async fn serve(engine: SharedEngine, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = router(engine);
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
    State(engine): State<SharedEngine>,
    Json(req): Json<CompileRequest>,
) -> Json<CompileResult> {
    let file_indexes: Vec<FileIndex> = req
        .files
        .iter()
        .filter_map(|path| parse_file_to_index(path))
        .collect();

    let mut engine = engine.lock().unwrap();
    Json(engine.compile(&file_indexes))
}

async fn discover(
    State(engine): State<SharedEngine>,
    AxumPath(hash): AxumPath<String>,
    Query(query): Query<DiscoverQuery>,
) -> Result<Json<DiscoverResult>, StatusCode> {
    let depth = query.depth.unwrap_or(1);
    let engine = engine.lock().unwrap();
    engine
        .discover(&hash, depth)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn where_hash(
    State(engine): State<SharedEngine>,
    AxumPath(hash): AxumPath<String>,
) -> Result<Json<WhereResponse>, StatusCode> {
    let engine = engine.lock().unwrap();
    engine
        .where_hash(&hash)
        .map(|(file, line)| Json(WhereResponse { file, line }))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn explain(
    State(engine): State<SharedEngine>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ExplainResult>, StatusCode> {
    let engine = engine.lock().unwrap();
    engine
        .explain(&req.error_code, &req.hash)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

// --- File parsing helper ---

/// Detect language from file extension.
fn detect_language(path: &str) -> Option<&'static str> {
    match Path::new(path).extension()?.to_str()? {
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => Some("typescript"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/// Parse a single file from disk into a FileIndex.
fn parse_file_to_index(path: &str) -> Option<FileIndex> {
    let content = std::fs::read_to_string(path).ok()?;
    let lang = detect_language(path)?;

    let resolver: Box<dyn LanguageResolver> = match lang {
        "typescript" => Box::new(keel_parsers::typescript::TsResolver::new()),
        "python" => Box::new(keel_parsers::python::PyResolver::new()),
        "go" => Box::new(keel_parsers::go::GoResolver::new()),
        "rust" => Box::new(keel_parsers::rust_lang::RustLangResolver::new()),
        _ => return None,
    };

    let result = resolver.parse_file(Path::new(path), &content);
    let content_hash = xxhash_rust::xxh64::xxh64(content.as_bytes(), 0);

    Some(FileIndex {
        file_path: path.to_string(),
        content_hash,
        definitions: result.definitions,
        references: result.references,
        imports: result.imports,
        external_endpoints: result.external_endpoints,
        parse_duration_us: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Method, Request};
    use keel_core::sqlite::SqliteGraphStore;
    use keel_core::types::GraphNode;
    use tower::ServiceExt;

    fn test_engine() -> SharedEngine {
        let store = SqliteGraphStore::in_memory().unwrap();
        let engine = EnforcementEngine::new(Box::new(store));
        Arc::new(Mutex::new(engine))
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
    async fn test_cors_preflight() {
        let app = router(test_engine());
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
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
        assert_eq!(detect_language("lib/index.ts"), Some("typescript"));
        assert_eq!(detect_language("app.py"), Some("python"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("README.md"), None);
    }
}
