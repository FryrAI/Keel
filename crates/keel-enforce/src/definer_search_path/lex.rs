//! A PostgreSQL lexer, just deep enough to read routine DDL: comments are
//! dropped, and a string constant or dollar-quoted body is one opaque token.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// Unquoted identifier, keyword or number, folded to lower case.
    Word,
    /// `"Quoted"` identifier, verbatim.
    Quoted,
    /// String constant (`'…'`, `E'…'`, `$tag$…$tag$`), verbatim.
    Str,
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: Kind,
    pub text: String,
    pub offset: usize,
}

impl Token {
    pub fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text == word
    }

    pub fn is_punct(&self, punct: char) -> bool {
        self.kind == Kind::Punct && self.text.starts_with(punct)
    }

    /// The identifier this token spells, if it is one.
    pub fn ident(&self) -> Option<&str> {
        matches!(self.kind, Kind::Word | Kind::Quoted).then_some(self.text.as_str())
    }
}

pub(super) fn lex(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    // A byte-order mark is not part of the first statement.
    let mut at = if sql.starts_with('\u{feff}') { 3 } else { 0 };
    while at < bytes.len() {
        let start = at;
        let (kind, text) = if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        } else if bytes[at..].starts_with(b"--") || bytes[at] == b'\\' {
            // A comment, or a psql meta-command: neither is SQL.
            at = bytes[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |end| at + end + 1);
            continue;
        } else if bytes[at..].starts_with(b"/*") {
            at = block_comment_end(bytes, at);
            continue;
        } else if bytes[at] == b'\'' {
            let escapes = take_prefix(&mut tokens, at);
            let (text, end) = quoted(bytes, at, b'\'', escapes);
            at = end;
            (Kind::Str, text)
        } else if bytes[at] == b'"' {
            take_prefix(&mut tokens, at);
            let (text, end) = quoted(bytes, at, b'"', false);
            at = end;
            (Kind::Quoted, text)
        } else if let Some((text, end)) = dollar_quoted(sql, at) {
            at = end;
            (Kind::Str, text)
        } else if is_word_byte(bytes[at]) && bytes[at] != b'$' {
            while at < bytes.len() && is_word_byte(bytes[at]) {
                at += 1;
            }
            (Kind::Word, sql[start..at].to_ascii_lowercase())
        } else {
            at += 1;
            (Kind::Punct, char::from(bytes[start]).to_string())
        };
        tokens.push(Token {
            kind,
            text,
            offset: start,
        });
    }
    tokens
}

/// Drops an `E` or `U&` prefix glued to the quote at `at`: it belongs to the
/// constant, not to the statement. True for `E`, whose constant reads backslash
/// escapes (a quote among them; the other sequences are kept undecoded).
fn take_prefix(tokens: &mut Vec<Token>, at: usize) -> bool {
    let ends_at = |token: &Token, end: usize| token.offset + token.text.len() == end;
    if let [.., last] = tokens.as_slice() {
        if last.is_word("e") && ends_at(last, at) {
            tokens.pop();
            return true;
        }
    }
    if let [.., u, amp] = tokens.as_slice() {
        if u.is_word("u") && ends_at(u, amp.offset) && amp.is_punct('&') && ends_at(amp, at) {
            tokens.truncate(tokens.len() - 2);
        }
    }
    false
}

/// Identifier bytes; every non-ASCII byte counts, as PostgreSQL has it.
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || !byte.is_ascii()
}

/// The index after the comment opening at `start`; block comments nest.
fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let (mut depth, mut at) = (0usize, start);
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if bytes[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    bytes.len()
}

/// The content of the constant or identifier opening at `start`, and the index
/// after its closing quote. A doubled quote is one quote.
fn quoted(bytes: &[u8], start: usize, quote: u8, backslash: bool) -> (String, usize) {
    let mut text = Vec::new();
    let mut at = start + 1;
    while at < bytes.len() {
        if backslash && bytes[at] == b'\\' && at + 1 < bytes.len() {
            text.push(bytes[at + 1]);
            at += 2;
        } else if bytes[at] != quote {
            text.push(bytes[at]);
            at += 1;
        } else if bytes.get(at + 1) == Some(&quote) {
            text.push(quote);
            at += 2;
        } else {
            at += 1;
            break;
        }
    }
    (String::from_utf8_lossy(&text).into_owned(), at)
}

/// A `$tag$…$tag$` constant opening at `start`: its content and the index
/// after its closing tag. `$1` is a parameter, not a tag.
fn dollar_quoted(sql: &str, start: usize) -> Option<(String, usize)> {
    let bytes = sql.as_bytes();
    if bytes[start] != b'$' || bytes.get(start + 1)?.is_ascii_digit() {
        return None;
    }
    let tag_len = bytes[start + 1..]
        .iter()
        .position(|byte| !is_word_byte(*byte) || *byte == b'$')?;
    let open_end = start + tag_len + 2;
    if bytes[open_end - 1] != b'$' {
        return None;
    }
    let tag = &sql[start..open_end];
    let close = sql[open_end..]
        .find(tag)
        .map_or(sql.len(), |found| open_end + found);
    let end = (close + tag.len()).min(sql.len());
    Some((sql[open_end..close].to_string(), end))
}
