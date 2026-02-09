use xxhash_rust::xxh64::xxh64;

const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encode a u64 value as a base62 string (11 chars, zero-padded).
fn base62_encode(mut value: u64) -> String {
    if value == 0 {
        return "0".repeat(11);
    }
    let mut result = Vec::with_capacity(11);
    while value > 0 {
        let idx = (value % 62) as usize;
        result.push(BASE62_CHARS[idx]);
        value /= 62;
    }
    // Pad to 11 chars
    while result.len() < 11 {
        result.push(b'0');
    }
    result.reverse();
    String::from_utf8(result).expect("base62 chars are valid UTF-8")
}

/// Compute the keel hash for a function/class node.
///
/// hash = base62(xxhash64(canonical_signature + body_normalized + docstring))
///
/// - `canonical_signature`: normalized function declaration (name, params with types, return type)
/// - `body_normalized`: AST-based normalized function body (whitespace/comments stripped)
/// - `docstring`: the docstring content, or empty string if none
pub fn compute_hash(canonical_signature: &str, body_normalized: &str, docstring: &str) -> String {
    let mut input = String::with_capacity(
        canonical_signature.len() + body_normalized.len() + docstring.len() + 2,
    );
    input.push_str(canonical_signature);
    input.push('\0'); // separator
    input.push_str(body_normalized);
    input.push('\0'); // separator
    input.push_str(docstring);

    let hash_value = xxh64(input.as_bytes(), 0);
    base62_encode(hash_value)
}

/// Compute a disambiguated hash when a collision is detected.
/// Appends the file path to the input to create a unique hash.
pub fn compute_hash_disambiguated(
    canonical_signature: &str,
    body_normalized: &str,
    docstring: &str,
    file_path: &str,
) -> String {
    let mut input = String::with_capacity(
        canonical_signature.len() + body_normalized.len() + docstring.len() + file_path.len() + 3,
    );
    input.push_str(canonical_signature);
    input.push('\0');
    input.push_str(body_normalized);
    input.push('\0');
    input.push_str(docstring);
    input.push('\0');
    input.push_str(file_path);

    let hash_value = xxh64(input.as_bytes(), 0);
    base62_encode(hash_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_hash() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "Adds one");
        let h2 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "Adds one");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_length() {
        let h = compute_hash("fn bar()", "{}", "");
        assert_eq!(h.len(), 11);
    }

    #[test]
    fn test_hash_changes_with_signature() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "");
        let h2 = compute_hash("fn foo(x: i64) -> i64", "x + 1", "");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_changes_with_body() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "");
        let h2 = compute_hash("fn foo(x: i32) -> i32", "x + 2", "");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_changes_with_docstring() {
        let h1 = compute_hash("fn foo()", "{}", "Does X");
        let h2 = compute_hash("fn foo()", "{}", "Does Y");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_disambiguated_hash_differs() {
        let h1 = compute_hash("fn foo()", "{}", "");
        let h2 = compute_hash_disambiguated("fn foo()", "{}", "", "src/a.rs");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_base62_encoding() {
        let encoded = base62_encode(0);
        assert_eq!(encoded.len(), 11);
        assert!(encoded.chars().all(|c| c == '0'));

        let encoded = base62_encode(1);
        assert_eq!(encoded.len(), 11);
    }
}
