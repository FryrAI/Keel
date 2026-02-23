use super::*;

#[test]
fn credentials_not_expired() {
    let future = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let creds = Credentials {
        access_token: "tok".into(),
        refresh_token: "ref".into(),
        expires_at: future,
    };
    assert!(!creds.is_expired());
}

#[test]
fn credentials_expired() {
    let creds = Credentials {
        access_token: "tok".into(),
        refresh_token: "ref".into(),
        expires_at: 0,
    };
    assert!(creds.is_expired());
}

#[test]
fn credentials_roundtrip_json() {
    let creds = Credentials {
        access_token: "access_abc".into(),
        refresh_token: "refresh_xyz".into(),
        expires_at: 1700000000,
    };
    let json = serde_json::to_string(&creds).unwrap();
    let parsed: Credentials = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.access_token, "access_abc");
    assert_eq!(parsed.refresh_token, "refresh_xyz");
    assert_eq!(parsed.expires_at, 1700000000);
}

#[test]
fn keel_home_returns_some() {
    // HOME is always set in test environments
    let home = keel_home();
    assert!(home.is_some());
    let path = home.unwrap();
    assert!(path.ends_with(".keel"));
}

#[test]
fn save_and_load_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    // Override HOME to use temp dir
    let original = std::env::var("HOME").ok();
    std::env::set_var("HOME", tmp.path());

    let creds = Credentials {
        access_token: "test_token".into(),
        refresh_token: "test_refresh".into(),
        expires_at: 9999999999,
    };
    save_credentials(&creds).unwrap();

    let loaded = load_credentials().expect("should load credentials");
    assert_eq!(loaded.access_token, "test_token");
    assert_eq!(loaded.refresh_token, "test_refresh");
    assert_eq!(loaded.expires_at, 9999999999);

    clear_credentials();
    assert!(load_credentials().is_none());

    // Restore HOME
    if let Some(h) = original {
        std::env::set_var("HOME", h);
    }
}
