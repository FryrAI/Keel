mod file_kinds;
mod identity;
mod naming;
mod signatures;

pub use file_kinds::{distinct_compilation_units, is_bench_file, is_stub_file, is_test_file};
pub use identity::{bind_to_node, definition_hash_salted, definition_hashes, node_hash_matches};
pub use naming::extract_prefix;
pub use signatures::normalize_signature;
pub(crate) use signatures::{
    countable_arity, is_ident_char, match_paren, parse_signature, receiver_is_type_like,
    split_top_level, strip_receiver, ParsedSig,
};
