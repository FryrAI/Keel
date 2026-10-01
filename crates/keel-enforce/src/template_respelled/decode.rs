//! Semantic string decoding; unknown or invalid escapes are rejected rather
//! than compared as guessed runtime text.

/// Decode standard escapes, leaving raw and byte string text as written.
pub(super) fn decode(text: &str, lang: &str, raw: bool) -> Option<String> {
    // Physical source line endings are normalized before escape processing.
    let text = if lang == "go" && raw {
        text.replace('\r', "")
    } else if lang != "go" {
        text.replace("\r\n", "\n")
    } else {
        text.to_string()
    };
    if raw || !text.contains('\\') {
        return Some(text);
    }
    // Go hex/octal escapes represent bytes, not Unicode code points.
    let mut bytes = Vec::new();
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            '\\' | '\'' | '"' => out.push(escaped),
            '`' | '$' if lang == "typescript" => out.push(escaped),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' if lang == "rust" => out.push('\0'),
            'b' if lang != "rust" => out.push('\u{8}'),
            'f' if lang != "rust" => out.push('\u{c}'),
            'a' if matches!(lang, "go" | "python") => out.push('\u{7}'),
            'v' if lang != "rust" => out.push('\u{b}'),
            '\n' if lang != "go" => {
                if lang == "rust" {
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                }
            }
            '\r' if matches!(lang, "typescript" | "python") => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            'x' | 'u' | 'U' if escaped != 'U' || lang != "typescript" => {
                let mut hex = String::new();
                if escaped == 'u'
                    && chars.peek() == Some(&'{')
                    && matches!(lang, "rust" | "typescript")
                {
                    chars.next();
                    loop {
                        let ch = chars.next()?;
                        if ch == '}' {
                            break;
                        }
                        if ch != '_' || lang != "rust" {
                            hex.push(ch);
                        }
                    }
                } else {
                    let n = match escaped {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    for _ in 0..n {
                        hex.push(chars.next()?);
                    }
                }
                let value = u32::from_str_radix(&hex, 16).ok()?;
                if lang == "go" && escaped == 'x' {
                    bytes.extend_from_slice(out.as_bytes());
                    out.clear();
                    bytes.push(u8::try_from(value).ok()?);
                    continue;
                }
                // JS encodes supplementary characters as surrogate pairs.
                let value = if lang == "typescript" && (0xd800..=0xdbff).contains(&value) {
                    if chars.next()? != '\\' || chars.next()? != 'u' {
                        return None;
                    }
                    let low: String = chars.by_ref().take(4).collect();
                    let low = u32::from_str_radix(&low, 16).ok()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return None;
                    }
                    0x10000 + ((value - 0xd800) << 10) + low - 0xdc00
                } else {
                    value
                };
                out.push(char::from_u32(value)?);
            }
            c @ '0'..='7' if lang != "rust" => {
                let mut oct = String::from(c);
                for _ in 0..2 {
                    if chars.peek().is_some_and(|c| matches!(c, '0'..='7')) {
                        oct.push(chars.next()?);
                    } else {
                        break;
                    }
                }
                if lang == "go" && oct.len() != 3 {
                    return None;
                }
                let value = u32::from_str_radix(&oct, 8).ok()?;
                if lang == "go" {
                    bytes.extend_from_slice(out.as_bytes());
                    out.clear();
                    bytes.push(u8::try_from(value).ok()?);
                } else {
                    out.push(char::from_u32(value)?);
                }
            }
            c if lang == "python" && !matches!(c, 'N') => {
                out.push('\\');
                out.push(c);
            }
            c if lang == "typescript" => out.push(c),
            _ => return None,
        }
    }
    if lang == "go" {
        bytes.extend_from_slice(out.as_bytes());
        String::from_utf8(bytes).ok()
    } else {
        Some(out)
    }
}

/// Rust format strings and Python `.format` use doubled literal braces. The
/// Python f-string boundaries themselves come from interpolation AST nodes.
pub(super) fn brace_segments(text: &str) -> Option<Vec<String>> {
    let mut chars = text.chars().peekable();
    let mut segments = vec![String::new()];
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' if chars.peek() == Some(&c) => {
                chars.next();
                segments.last_mut()?.push(c);
            }
            '{' => {
                let mut depth = 1;
                while depth > 0 {
                    match chars.next()? {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                segments.push(String::new());
            }
            '}' => return None,
            _ => segments.last_mut()?.push(c),
        }
    }
    Some(segments)
}

/// Split Go printf verbs, retaining escaped percent signs as fixed text.
pub(super) fn printf_segments(text: &str) -> Option<Vec<String>> {
    let mut chars = text.chars().peekable();
    let mut segments = vec![String::new()];
    while let Some(c) = chars.next() {
        if c != '%' {
            segments.last_mut()?.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            segments.last_mut()?.push('%');
            continue;
        }
        loop {
            let c = chars.next()?;
            if c.is_ascii_alphabetic() {
                break;
            }
            if !matches!(c, '0'..='9' | '[' | ']' | '*' | '.' | '+' | '-' | '#' | ' ') {
                return None;
            }
        }
        segments.push(String::new());
    }
    Some(segments)
}
