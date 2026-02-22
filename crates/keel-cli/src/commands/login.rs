//! `keel login` — authenticate with keel cloud via device flow.

use std::io::Read;

use crate::auth;
use super::json_helpers::{extract_json_string, extract_json_number};

const API_BASE: &str = "https://api.keel.engineer";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the device-flow login sequence.
pub fn run(verbose: bool) -> i32 {
    // Check if already logged in
    if let Some(creds) = auth::load_credentials() {
        if !creds.is_expired() {
            eprintln!("already logged in. Use `keel logout` first to switch accounts.");
            return 0;
        }
        if verbose {
            eprintln!("existing token expired, re-authenticating...");
        }
    }

    // Step 1: Request device code
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build(),
    );

    let url = format!("{API_BASE}/auth/device/code");
    let resp = match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
        .send(b"{}".as_slice())
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to initiate login: {e}");
            return 2;
        }
    };

    let mut body = String::new();
    if resp.into_body().into_reader().read_to_string(&mut body).is_err() {
        eprintln!("error: failed to read login response");
        return 2;
    }

    let device_code = match extract_json_string(&body, "device_code") {
        Some(v) => v,
        None => {
            eprintln!("error: unexpected response from auth server");
            if verbose {
                eprintln!("response: {body}");
            }
            return 2;
        }
    };
    let user_code = extract_json_string(&body, "user_code").unwrap_or_default();
    let verification_uri = extract_json_string(&body, "verification_uri")
        .unwrap_or_else(|| format!("{API_BASE}/device"));
    let interval: u64 = extract_json_number(&body, "interval").unwrap_or(5);
    let expires_in: u64 = extract_json_number(&body, "expires_in").unwrap_or(900);

    // Step 2: Open browser + show user code
    eprintln!("opening browser to authenticate...");
    eprintln!();
    eprintln!("  your code: {user_code}");
    eprintln!("  url: {verification_uri}");
    eprintln!();

    if let Err(e) = webbrowser::open(&verification_uri) {
        eprintln!("could not open browser: {e}");
        eprintln!("please open the URL above manually.");
    }

    eprintln!("waiting for authorization...");

    // Step 3: Poll for token
    let poll_url = format!("{API_BASE}/auth/device/token");
    let poll_body = format!(r#"{{"device_code":"{device_code}"}}"#);
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(expires_in);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(interval));

        if std::time::Instant::now() > deadline {
            eprintln!("error: login timed out. Please try again.");
            return 2;
        }

        let resp = match agent
            .post(&poll_url)
            .header("Content-Type", "application/json")
            .header("User-Agent", &format!("keel/{CURRENT_VERSION}"))
            .send(poll_body.as_bytes())
        {
            Ok(r) => r,
            Err(_) => continue, // Network blip, retry
        };

        let mut resp_body = String::new();
        if resp.into_body().into_reader().read_to_string(&mut resp_body).is_err() {
            continue;
        }

        // Check for pending status
        if resp_body.contains("authorization_pending") || resp_body.contains("slow_down") {
            continue;
        }

        // Check for token
        if let Some(access_token) = extract_json_string(&resp_body, "access_token") {
            let refresh_token =
                extract_json_string(&resp_body, "refresh_token").unwrap_or_default();
            let expires_at =
                extract_json_number(&resp_body, "expires_at").unwrap_or_else(|| {
                    // Fallback: current time + 1 hour
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        + 3600
                });

            let creds = auth::Credentials {
                access_token,
                refresh_token,
                expires_at,
            };

            if let Err(e) = auth::save_credentials(&creds) {
                eprintln!("error: {e}");
                return 2;
            }

            eprintln!("logged in successfully.");
            return 0;
        }

        // Unexpected response — check for error
        if let Some(err) = extract_json_string(&resp_body, "error") {
            eprintln!("error: {err}");
            return 2;
        }
    }
}

