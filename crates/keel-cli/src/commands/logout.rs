//! `keel logout` — remove stored credentials.

use crate::auth;

/// Run the logout command.
pub fn run(verbose: bool) -> i32 {
    match auth::load_credentials() {
        Some(_) => {
            auth::clear_credentials();
            eprintln!("logged out.");
            if verbose {
                if let Some(home) = auth::keel_home() {
                    eprintln!("removed credentials from {}", home.display());
                }
            }
            0
        }
        None => {
            eprintln!("not currently logged in.");
            0
        }
    }
}
