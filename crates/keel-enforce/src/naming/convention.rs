//! Naming-convention inference and name generation for `keel name`.

/// Detect naming convention from sibling function names.
pub(super) fn detect_convention(names: &[&str]) -> NamingConvention {
    if names.is_empty() {
        return NamingConvention::SnakeCase { prefix: None };
    }

    let snake_count = names.iter().filter(|n| n.contains('_')).count();
    let camel_count = names
        .iter()
        .filter(|n| !n.contains('_') && n.chars().any(|c| c.is_uppercase()))
        .count();
    let prefix = detect_common_prefix(names);

    if snake_count >= camel_count {
        NamingConvention::SnakeCase { prefix }
    } else {
        NamingConvention::CamelCase { prefix }
    }
}

/// Detect a prefix shared by at least half of the sibling names.
pub(super) fn detect_common_prefix(names: &[&str]) -> Option<String> {
    if names.len() < 2 {
        return None;
    }
    let prefixes: Vec<&str> = names.iter().filter_map(|n| n.split('_').next()).collect();
    let first = prefixes.first()?;
    let matching = prefixes.iter().filter(|p| *p == first).count();
    (matching * 2 >= names.len() && !first.is_empty()).then(|| format!("{first}_"))
}

/// Generate a suggested name from description keywords and convention.
pub(super) fn generate_name(desc_words: &[String], convention: &NamingConvention) -> String {
    let filtered: Vec<&str> = desc_words.iter().take(4).map(String::as_str).collect();

    match convention {
        NamingConvention::SnakeCase { prefix } => {
            format!(
                "{}{}",
                prefix.as_deref().unwrap_or_default(),
                filtered.join("_")
            )
        }
        NamingConvention::CamelCase { prefix } => {
            let base: String = filtered
                .iter()
                .enumerate()
                .map(|(i, word)| match (i, word.chars().next()) {
                    (0, _) => word.to_string(),
                    (_, Some(first)) => {
                        first.to_uppercase().to_string() + &word[first.len_utf8()..]
                    }
                    (_, None) => String::new(),
                })
                .collect();
            format!("{}{}", prefix.as_deref().unwrap_or_default(), base)
        }
    }
}

#[derive(Debug)]
pub(super) enum NamingConvention {
    SnakeCase { prefix: Option<String> },
    CamelCase { prefix: Option<String> },
}

impl std::fmt::Display for NamingConvention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NamingConvention::SnakeCase { prefix } => {
                write!(f, "snake_case")?;
                if let Some(prefix) = prefix {
                    write!(f, ", prefix: {prefix}")?;
                }
            }
            NamingConvention::CamelCase { prefix } => {
                write!(f, "camelCase")?;
                if let Some(prefix) = prefix {
                    write!(f, ", prefix: {prefix}")?;
                }
            }
        }
        Ok(())
    }
}
