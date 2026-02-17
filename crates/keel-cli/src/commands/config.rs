//! `keel config` command — get or set configuration values.
//!
//! Supports dot-notation for nested keys:
//!   keel config                          # dump full config as JSON
//!   keel config tier                     # get current tier
//!   keel config tier team                # set tier to "team"
//!   keel config telemetry.enabled false  # disable telemetry

use std::fs;
use std::path::Path;

use keel_core::config::KeelConfig;
use keel_output::OutputFormatter;

pub fn run(
    _formatter: &dyn OutputFormatter,
    _verbose: bool,
    key: Option<String>,
    value: Option<String>,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel config: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel config: not initialized. Run `keel init` first.");
        return 2;
    }

    let config_path = keel_dir.join("keel.json");

    match (key, value) {
        (None, None) => dump_config(&config_path),
        (Some(k), None) => get_config(&config_path, &k),
        (Some(k), Some(v)) => set_config(&config_path, &k, &v),
        (None, Some(_)) => {
            eprintln!("keel config: value provided without key");
            2
        }
    }
}

fn dump_config(config_path: &Path) -> i32 {
    let config = KeelConfig::load(config_path.parent().unwrap_or(Path::new(".")));
    match serde_json::to_string_pretty(&config) {
        Ok(json) => {
            println!("{}", json);
            0
        }
        Err(e) => {
            eprintln!("keel config: failed to serialize: {}", e);
            2
        }
    }
}

fn get_config(config_path: &Path, key: &str) -> i32 {
    let config = KeelConfig::load(config_path.parent().unwrap_or(Path::new(".")));
    let json_value = match serde_json::to_value(&config) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("keel config: failed to serialize: {}", e);
            return 2;
        }
    };

    match resolve_dot_path(&json_value, key) {
        Some(v) => {
            let output = match v {
                serde_json::Value::String(s) => s.to_string(),
                other => other.to_string(),
            };
            println!("{}", output);
            0
        }
        None => {
            eprintln!("keel config: unknown key '{}'", key);
            1
        }
    }
}

fn set_config(config_path: &Path, key: &str, value: &str) -> i32 {
    let config = KeelConfig::load(config_path.parent().unwrap_or(Path::new(".")));
    let mut json_value = match serde_json::to_value(&config) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("keel config: failed to serialize: {}", e);
            return 2;
        }
    };

    // Parse value into appropriate JSON type
    let parsed_value = parse_value(value);

    if !set_dot_path(&mut json_value, key, parsed_value) {
        eprintln!("keel config: unknown key '{}'", key);
        return 1;
    }

    // Validate by deserializing back to KeelConfig
    let updated: KeelConfig = match serde_json::from_value(json_value.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("keel config: invalid value for '{}': {}", key, e);
            return 1;
        }
    };

    match fs::write(config_path, serde_json::to_string_pretty(&updated).unwrap()) {
        Ok(_) => {
            eprintln!("keel config: {} = {}", key, value);
            0
        }
        Err(e) => {
            eprintln!("keel config: failed to write config: {}", e);
            2
        }
    }
}

fn parse_value(value: &str) -> serde_json::Value {
    match value {
        "true" => serde_json::Value::Bool(true),
        "false" => serde_json::Value::Bool(false),
        "null" => serde_json::Value::Null,
        _ => {
            if let Ok(n) = value.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if let Ok(f) = value.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(value.to_string()))
            } else {
                serde_json::Value::String(value.to_string())
            }
        }
    }
}

fn resolve_dot_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn set_dot_path(value: &mut serde_json::Value, path: &str, new_value: serde_json::Value) -> bool {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return false;
    }

    let mut current = value;
    for segment in &segments[..segments.len() - 1] {
        current = match current.get_mut(*segment) {
            Some(v) => v,
            None => return false,
        };
    }

    let last = segments[segments.len() - 1];
    if current.get(last).is_some() {
        current[last] = new_value;
        true
    } else {
        false
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
