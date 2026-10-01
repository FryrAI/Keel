//! Strict changed-path decoding and process-wide diagnostic deduplication.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use keel_parsers::walker::KeelIgnore;

static WARNED_PATHS: LazyLock<Mutex<HashSet<Vec<u8>>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Which side of the diff owns a path's ignore verdict.
#[derive(Clone, Copy)]
pub(super) enum PathRole {
    /// Existing paths take the walker's own verdict.
    Head,
    /// Previous paths take the hand model, independently of current contents.
    Base,
}

/// Reject unrepresentable paths without aliasing replacement-character siblings.
/// Warn once for each raw path that passes the source and ignore filters.
pub(super) fn decode_path<'a>(
    bytes: &'a [u8],
    ignore: &KeelIgnore,
    role: PathRole,
) -> Option<&'a str> {
    match std::str::from_utf8(bytes) {
        Ok(path) => Some(path),
        Err(_) => {
            if !warning_eligible(bytes, ignore, role)
                || !WARNED_PATHS
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(bytes.to_vec())
            {
                return None;
            }
            let escaped: String = bytes
                .iter()
                .flat_map(|b| std::ascii::escape_default(*b))
                .map(char::from)
                .collect();
            eprintln!("keel: skipping a changed path that is not valid UTF-8: {escaped}");
            None
        }
    }
}

#[cfg(unix)]
fn warning_eligible(bytes: &[u8], ignore: &KeelIgnore, role: PathRole) -> bool {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    let path = Path::new(OsStr::from_bytes(bytes));
    keel_parsers::treesitter::detect_language(path).is_some()
        && match role {
            PathRole::Head => !ignore.ignored_paths(&[path.to_path_buf()]).contains(path),
            PathRole::Base => !ignore.is_ignored(path),
        }
}

#[cfg(not(unix))]
fn warning_eligible(_bytes: &[u8], _ignore: &KeelIgnore, _role: PathRole) -> bool {
    true
}
