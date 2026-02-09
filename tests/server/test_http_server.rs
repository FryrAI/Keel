// Tests for HTTP server endpoints (Spec 010)
//
// Validates the REST API surface that `keel serve --http` exposes,
// including all command endpoints, CORS handling, and error responses.
//
// use keel_server::http::{HttpServer, HttpConfig};
// use std::net::TcpListener;

#[test]
#[ignore = "Not yet implemented"]
fn test_http_compile_endpoint() {
    // GIVEN an HTTP server running on localhost with a mapped project
    // WHEN a POST request is sent to /api/compile with a file path in the body
    // THEN the response is 200 OK with a JSON CompileResult payload
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_discover_endpoint() {
    // GIVEN an HTTP server running on localhost with a mapped project
    // WHEN a GET request is sent to /api/discover/{hash}
    // THEN the response is 200 OK with a JSON DiscoverResult payload
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_map_endpoint() {
    // GIVEN an HTTP server running on localhost with a mapped project
    // WHEN a POST request is sent to /api/map
    // THEN the response is 200 OK with updated graph statistics
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_explain_endpoint() {
    // GIVEN an HTTP server running on localhost with a mapped project
    // WHEN a GET request is sent to /api/explain/{code}/{hash}
    // THEN the response is 200 OK with a JSON ExplainResult payload
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_where_endpoint() {
    // GIVEN an HTTP server running on localhost with a mapped project
    // WHEN a GET request is sent to /api/where/{hash}
    // THEN the response is 200 OK with a JSON object containing file path and line number
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_cors_headers_present() {
    // GIVEN an HTTP server configured with CORS enabled
    // WHEN a preflight OPTIONS request is sent to any API endpoint
    // THEN the response includes Access-Control-Allow-Origin and related CORS headers
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_error_response_format() {
    // GIVEN an HTTP server running on localhost
    // WHEN a request is sent to a valid endpoint with invalid parameters
    // THEN the response is a 4xx status with a JSON error body containing code and message
}

#[test]
#[ignore = "Not yet implemented"]
fn test_http_unknown_endpoint_returns_404() {
    // GIVEN an HTTP server running on localhost
    // WHEN a GET request is sent to /api/nonexistent
    // THEN the response is 404 Not Found with a JSON error body
}
