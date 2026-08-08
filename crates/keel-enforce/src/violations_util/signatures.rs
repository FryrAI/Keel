/// Maximum characters a parenthesized span may cover before it is treated as
/// unbalanced prose rather than an argument list. Bounds the plan scanner,
/// which runs over free text where an opening paren may never close.
pub(crate) const MAX_CLAIM_SPAN: usize = 600;

/// A signature (or a plan's call claim) reduced to the two things keel
/// compares: how many arguments it takes, and whether it declares a return.
pub(crate) struct ParsedSig {
    /// Parameter count with any receiver removed.
    pub(crate) arity: usize,
    /// Whether an explicit `-> T` follows the parameter list.
    pub(crate) has_return: bool,
    /// Whether a leading `self`/`cls`/`this` receiver was removed from the
    /// count — the signal that a call site *may* legally spell the receiver as
    /// its first argument (`Base.__init__(self, x)`, `Rc::clone(&x)`).
    pub(crate) has_receiver: bool,
}

/// True for characters that may appear inside an identifier.
pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True when the `'` at `chars[i]` opens a character literal (`'x'`, `'\n'`)
/// rather than starting a Rust lifetime (`'a`, `'static`). A literal closes
/// within two characters (one payload char, or a backslash escape); a lifetime
/// is `'` + identifier with no closing quote at all. Treating a lifetime as a
/// string opener swallowed everything up to the *next* tick — in
/// `(node: Node<'a>, source: &'a [u8])` that includes the top-level comma, so
/// the arity came out one short and E005 mis-fired on every caller.
pub(crate) fn tick_opens_literal(chars: &[char], i: usize) -> bool {
    matches!(chars.get(i + 1), Some('\\')) || matches!(chars.get(i + 2), Some('\''))
}

/// Index of the `)` closing the `(` at `open`, or `None` when the span is
/// unbalanced, longer than `MAX_CLAIM_SPAN`, or crosses a blank line.
pub(crate) fn match_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    let mut newlines = 0usize;
    for (offset, &ch) in chars[open..].iter().enumerate() {
        if offset > MAX_CLAIM_SPAN {
            return None;
        }
        if let Some(q) = in_str {
            if ch == q && prev != '\\' {
                in_str = None;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' | '`' => in_str = Some(ch),
            '\'' if tick_opens_literal(chars, open + offset) => in_str = Some(ch),
            '\n' => {
                newlines += 1;
                if newlines > 6 {
                    return None;
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
        prev = ch;
    }
    None
}

/// Split an argument list on top-level commas, tracking `()`, `[]`, `{}`,
/// generic `<>` (only when the `<` follows an identifier, so `a < b` is not a
/// bracket) and string literals. A `'` opens a literal only when it closes as
/// one ([`tick_opens_literal`]) — a Rust lifetime tick must not swallow the
/// rest of the list.
pub(crate) fn split_top_level(args: &str) -> Vec<String> {
    let chars: Vec<char> = args.chars().collect();
    let mut parts = Vec::new();
    let (mut round, mut square, mut curly, mut angle) = (0i32, 0i32, 0i32, 0i32);
    let mut in_str: Option<char> = None;
    let mut cur = String::new();
    let mut prev = '\0';
    for (i, &ch) in chars.iter().enumerate() {
        if let Some(q) = in_str {
            cur.push(ch);
            if ch == q && prev != '\\' {
                in_str = None;
            }
            prev = ch;
            continue;
        }
        match ch {
            '"' | '`' => {
                in_str = Some(ch);
                cur.push(ch);
            }
            '\'' if tick_opens_literal(&chars, i) => {
                in_str = Some(ch);
                cur.push(ch);
            }
            '(' => {
                round += 1;
                cur.push(ch);
            }
            ')' => {
                round -= 1;
                cur.push(ch);
            }
            '[' => {
                square += 1;
                cur.push(ch);
            }
            ']' => {
                square -= 1;
                cur.push(ch);
            }
            '{' => {
                curly += 1;
                cur.push(ch);
            }
            '}' => {
                curly -= 1;
                cur.push(ch);
            }
            '<' if is_ident_char(prev) => {
                angle += 1;
                cur.push(ch);
            }
            '>' if angle > 0 && prev != '-' && prev != '=' => {
                angle -= 1;
                cur.push(ch);
            }
            ',' if round == 0 && square == 0 && curly == 0 && angle == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
        prev = ch;
    }
    parts.push(cur);
    parts
}

/// Drop a leading receiver parameter (`self`, `&self`, `&mut self`, `cls`,
/// `this`): it is never written at the call site, so counting it would make
/// every method call look like it is one argument short. Returns whether a
/// receiver was actually removed.
pub(crate) fn strip_receiver(parts: &mut Vec<String>) -> bool {
    let is_receiver = parts.first().is_some_and(|first| {
        let t = first
            .trim()
            .trim_start_matches('&')
            .trim()
            .trim_start_matches("mut ")
            .trim();
        let head = t
            .split(|c: char| c == ':' || c.is_whitespace())
            .next()
            .unwrap_or("");
        matches!(head, "self" | "cls" | "this")
    });
    if is_receiver {
        parts.remove(0);
    }
    is_receiver
}

/// Countable argument count, or `None` when the list is variadic (`*args`,
/// `...rest`), defaulted (`=`), optional (`?`) or elided (`...`) — all cases
/// where "the plan says N" and "the code takes N" are not comparable.
pub(crate) fn countable_arity(parts: &[String]) -> Option<usize> {
    let trimmed: Vec<&str> = parts.iter().map(|p| p.trim()).collect();
    if trimmed.len() == 1 && trimmed[0].is_empty() {
        return Some(0);
    }
    for p in &trimmed {
        if p.is_empty()
            || p.starts_with('*')
            || p.starts_with("...")
            || p.starts_with('…')
            || p.contains('=')
            || p.contains('?')
        {
            return None;
        }
    }
    Some(trimmed.len())
}

/// Reduce a signature (`name(params) -> ret`) to arity and return presence.
/// `None` when there is no parameter list or the parameters are not countable.
pub(crate) fn parse_signature(sig: &str) -> Option<ParsedSig> {
    let chars: Vec<char> = sig.chars().collect();
    let open = chars.iter().position(|&c| c == '(')?;
    let close = match_paren(&chars, open)?;
    let args: String = chars[open + 1..close].iter().collect();
    let mut parts = split_top_level(&args);
    let has_receiver = strip_receiver(&mut parts);
    let arity = countable_arity(&parts)?;
    let tail: String = chars[close + 1..].iter().collect();
    Some(ParsedSig {
        arity,
        has_return: tail.contains("->"),
        has_receiver,
    })
}

/// True when a qualified call name's receiver segment names a *type* rather
/// than a value — `Base.__init__`, `Rc::clone`, `Self::new` — the one call
/// shape where the first written argument may legally be the receiver itself.
///
/// The receiver is everything before the last `.`/`::`; only its final path
/// segment is judged (so `mod::Type::method` looks at `Type`). Type-like means
/// an uppercase initial (the universal type convention in all four languages)
/// or the literal `Self`. A bare unqualified name has no receiver and is never
/// type-like.
pub(crate) fn receiver_is_type_like(call_name: &str) -> bool {
    let Some(cut) = call_name.rfind("::").max(call_name.rfind('.')) else {
        return false;
    };
    let segment = call_name[..cut].rsplit(['.', ':']).next().unwrap_or("");
    segment == "Self" || segment.starts_with(|c: char| c.is_uppercase())
}

/// Strip all whitespace so signatures can be compared ignoring pure
/// reformatting. A simple "collapse runs of whitespace to one space" would
/// still miscompare the most common rustfmt/prettier wrap style — one
/// parameter per line, with the `(`/`)` glued to a newline that has no
/// corresponding space in the single-line form:
/// ```text
/// fn foo(x: i32, y: i32) -> bool
/// fn foo(
///     x: i32,
///     y: i32,
/// ) -> bool
/// ```
/// Collapsing would leave `foo( x: i32...` vs `foo(x: i32...`, still
/// unequal. Removing whitespace entirely instead of collapsing it handles
/// this correctly — the token stream is what actually defines the
/// signature, and whitespace never distinguishes two different signatures.
pub fn normalize_signature(sig: &str) -> String {
    sig.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_signature_counts_params() {
        // Zero, one, many; nested generic/fn-pointer commas are one param.
        assert_eq!(parse_signature("fn foo()").unwrap().arity, 0);
        assert_eq!(parse_signature("fn foo(a: i32, b: str)").unwrap().arity, 2);
        assert_eq!(parse_signature("def bar(x, y, z)").unwrap().arity, 3);
        assert_eq!(
            parse_signature("fn f(m: HashMap<String, i32>, n: u8)")
                .unwrap()
                .arity,
            2
        );
        assert_eq!(
            parse_signature("fn f(cb: fn(a: i32, b: i32) -> i32)")
                .unwrap()
                .arity,
            1
        );
        assert!(parse_signature("fn foo").is_none());
    }

    #[test]
    fn test_parse_signature_receiver_stripped_and_reported() {
        // The receiver is never written at an ordinary call site, so it is not
        // counted — but its presence is reported so E005 can tolerate the
        // explicit-receiver call shape (`Base.__init__(self, x)`).
        let sig = parse_signature("fn method(&self, x: i32)").unwrap();
        assert_eq!(sig.arity, 1);
        assert!(sig.has_receiver);
        let sig = parse_signature("def method(self, x)").unwrap();
        assert_eq!(sig.arity, 1);
        assert!(sig.has_receiver);
        let free = parse_signature("fn free(x: i32)").unwrap();
        assert!(!free.has_receiver);
    }

    #[test]
    fn test_parse_signature_rust_lifetimes_do_not_swallow_commas() {
        // A lifetime tick used to open the "string literal" state, swallowing
        // everything up to the next tick — including top-level commas. The
        // repo's own `node_text(node: Node<'a>, source: &'a [u8])` counted as
        // ONE parameter, so E005 flagged all 19 correct two-argument callers.
        let sig =
            parse_signature("node_text(node: tree_sitter::Node<'a>, source: &'a [u8]) -> &'a str")
                .unwrap();
        assert_eq!(sig.arity, 2);
        assert!(sig.has_return);
        // A lone lifetime (no second tick anywhere) must not unbalance the
        // paren scan either.
        let sig = parse_signature("first(x: &'a str)").unwrap();
        assert_eq!(sig.arity, 1);
        // Real character literals still open (and close) as literals.
        let parts = split_top_level("'a', 'b'");
        assert_eq!(parts.len(), 2);
        // An escaped-quote char literal keeps its interior comma protected.
        let parts = split_top_level("f(',', x), y");
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn test_parse_signature_uncountable_lists_are_none() {
        // Variadic/defaulted/optional lists are not comparable to a call
        // site's argument count — E005 must skip them, not guess.
        assert!(parse_signature("def f(a, b=1)").is_none());
        assert!(parse_signature("def f(*args)").is_none());
        assert!(parse_signature("function f(a: number, b?: number)").is_none());
    }

    #[test]
    fn test_receiver_is_type_like() {
        assert!(receiver_is_type_like("Base.__init__"));
        assert!(receiver_is_type_like("Rc::clone"));
        assert!(receiver_is_type_like("Self::new"));
        assert!(receiver_is_type_like("mod::Type::method"));
        // Value receivers, packages, bare names: not type-like.
        assert!(!receiver_is_type_like("registry.register"));
        assert!(!receiver_is_type_like("self.render"));
        assert!(!receiver_is_type_like("fmt.Println"));
        assert!(!receiver_is_type_like("plain_call"));
    }

    #[test]
    fn test_normalize_signature_whitespace_only_reformat_matches() {
        // rustfmt's common one-param-per-line wrap style — the `(`/`)`
        // glue directly to a newline that has no corresponding space in
        // the single-line form. (A trailing comma, if rustfmt adds one,
        // is a real content difference, not whitespace — out of scope
        // for whitespace normalization, and correctly still flagged.)
        let a = "fn foo(x: i32, y: i32) -> bool";
        let b = "fn foo(\n    x: i32,\n    y: i32\n) -> bool";
        assert_eq!(normalize_signature(a), normalize_signature(b));

        // prettier-style wrap without a trailing comma
        let c = "function foo(\n  x: number,\n  y: number\n): boolean";
        let d = "function foo(x: number, y: number): boolean";
        assert_eq!(normalize_signature(c), normalize_signature(d));

        // Extra/irregular spacing
        assert_eq!(
            normalize_signature("fn  foo( x:i32 )"),
            normalize_signature("fn foo(x:i32)")
        );

        // Leading/trailing whitespace
        assert_eq!(
            normalize_signature("  fn foo()  "),
            normalize_signature("fn foo()")
        );
    }

    #[test]
    fn test_normalize_signature_real_change_differs() {
        // Added parameter is a real signature change, not just reformatting
        assert_ne!(
            normalize_signature("fn foo(x: i32)"),
            normalize_signature("fn foo(x: i32, y: i32)")
        );
        // Type change is a real signature change
        assert_ne!(
            normalize_signature("fn foo(x: i32)"),
            normalize_signature("fn foo(x: i64)")
        );
        // Renamed function is a real signature change
        assert_ne!(
            normalize_signature("fn foo()"),
            normalize_signature("fn bar()")
        );
    }
}
