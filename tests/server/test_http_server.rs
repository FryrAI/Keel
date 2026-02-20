// Tests for HTTP server endpoints (Spec 010)
use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use tower::ServiceExt;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{GraphNode, NodeKind};
use keel_enforce::engine::EnforcementEngine;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};
use keel_server::http::{router, HealthResponse, SharedEngine, WhereResponse};

fn test_engine() -> SharedEngine {
    let store = SqliteGraphStore::in_memory().unwrap();
    let engine = EnforcementEngine::new(Box::new(store));
    Arc::new(Mutex::new(engine))
}

fn engine_with_node() -> SharedEngine {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .insert_node(&GraphNode {
            id: 1,
            hash: "httpTestHash".into(),
            kind: NodeKind::Function,
            name: "handleRoute".into(),
            signature: "fn handleRoute(r: Req) -> Resp".into(),
            file_path: "src/routes.rs".into(),
            line_start: 10,
            line_end: 30,
            docstring: Some("Handle a route".into()),
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
async fn test_http_compile_endpoint() {
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
async fn test_http_discover_endpoint() {
    let app = router(engine_with_node());
    let req = Request::builder()
        .uri("/discover/httpTestHash")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
    let result: DiscoverResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.target.name, "handleRoute");
    assert_eq!(result.target.hash, "httpTestHash");
}

#[tokio::test]
async fn test_http_health_endpoint() {
    let app = router(test_engine());
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024).await.unwrap();
    let health: HealthResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(health.status, "ok");
    assert!(!health.version.is_empty());
}

#[tokio::test]
async fn test_http_explain_endpoint() {
    let app = router(engine_with_node());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/explain")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"error_code":"E001","hash":"httpTestHash"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
    let result: ExplainResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.error_code, "E001");
    assert_eq!(result.hash, "httpTestHash");
}

#[tokio::test]
async fn test_http_where_endpoint() {
    let app = router(engine_with_node());
    let req = Request::builder()
        .uri("/where/httpTestHash")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 10_000).await.unwrap();
    let result: WhereResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.file, "src/routes.rs");
    assert_eq!(result.line, 10);
}

#[tokio::test]
async fn test_http_cors_headers_present() {
    let app = router(test_engine());
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/health")
        .header(header::ORIGIN, "http://localhost:3000")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn test_http_error_response_format() {
    let app = router(test_engine());
    // Discover a non-existent hash
    let req = Request::builder()
        .uri("/discover/nonexistent_hash")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_http_unknown_endpoint_returns_404() {
    let app = router(test_engine());
    let req = Request::builder()
        .uri("/api/nonexistent")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
