//! Tolerant configuration for opt-in expression homes.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer, Serialize};

/// A named set of literal expressions permitted only in designated paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeRule {
    pub name: String,
    pub patterns: Vec<String>,
    #[serde(default, deserialize_with = "string_or_array")]
    pub home: Vec<String>,
    #[serde(default, deserialize_with = "string_or_array")]
    pub scope: Vec<String>,
}

/// Severity of newly introduced expressions outside their home.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HomeSeverity {
    #[default]
    Warning,
    Error,
}

impl<'de> Deserialize<'de> for HomeSeverity {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(match value.as_str() {
            Some("error") => Self::Error,
            Some("warning") => Self::Warning,
            _ => {
                warn_once(
                    format!("homes-severity:{value}"),
                    &format!("unknown enforce.homes value {value}; using warning"),
                );
                Self::Warning
            }
        })
    }
}

fn string_or_array<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Paths {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Paths::deserialize(d)? {
        Paths::One(s) => vec![s],
        Paths::Many(v) => v,
    })
}

/// Compile path globs with separators kept literal (`*` never spans `/`).
pub fn path_globs(paths: &[String]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for path in paths {
        builder.add(GlobBuilder::new(path).literal_separator(true).build()?);
    }
    builder.build()
}

fn warn_once(key: String, message: &str) {
    // The CLI loads config again for telemetry. Report each malformed rule
    // once per process rather than duplicating the diagnostic on that reload.
    static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    if WARNED
        .get_or_init(Mutex::default)
        .lock()
        .unwrap()
        .insert(key)
    {
        eprintln!("keel: warning: {message}");
    }
}

/// Deserialize rules individually, retaining valid rules and unrelated config.
pub fn deserialize_homes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<HomeRule>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let Some(rules) = value.as_array() else {
        warn_once(
            value.to_string(),
            "skipping homes: expected an array of rules",
        );
        return Ok(Vec::new());
    };
    let mut names = HashSet::new();
    let mut out = Vec::new();
    for (i, value) in rules.iter().enumerate() {
        let label = value
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("#{}", i + 1));
        let rule = serde_json::from_value::<HomeRule>(value.clone()).and_then(|mut rule| {
            for path in rule.home.iter_mut().chain(&mut rule.scope) {
                let components = path.split('/').collect::<Vec<_>>();
                if components.contains(&"..") {
                    return Err(serde::de::Error::custom(
                        "parent component '..' is not permitted in a home/scope glob",
                    ));
                }
                *path = components
                    .into_iter()
                    .filter(|part| !part.is_empty() && *part != ".")
                    .collect::<Vec<_>>()
                    .join("/");
            }
            if rule.home.iter().any(String::is_empty) {
                return Err(serde::de::Error::custom(
                    "repo root is not a permitted home",
                ));
            }
            if rule.scope.iter().any(String::is_empty) {
                rule.scope.clear();
            }
            Ok(rule)
        });
        let reason = match &rule {
            Err(e) => Some(e.to_string()),
            Ok(r) if r.name.trim().is_empty() => Some("empty name".into()),
            Ok(r) if r.patterns.is_empty() || r.patterns.iter().any(|p| p.is_empty()) => {
                Some("missing or empty pattern".into())
            }
            Ok(r) => path_globs(&r.home)
                .and_then(|_| path_globs(&r.scope))
                .err()
                .map(|e| e.to_string()),
        };
        if let Some(reason) = reason {
            warn_once(
                format!("{value}:{i}:{reason}"),
                &format!("skipping home rule {label:?}: {reason}"),
            );
            continue;
        }
        let rule = rule.expect("validated rule");
        if !names.insert(rule.name.clone()) {
            warn_once(
                format!("{value}:{i}:duplicate"),
                &format!("skipping home rule {label:?}: duplicate name"),
            );
            continue;
        }
        out.push(rule);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "config_homes_tests.rs"]
mod tests;
