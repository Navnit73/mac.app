//! Build script. Besides Tauri's own step, it passes the app API settings to the compiler.
//!
//! `MMS_APP_API_KEY` and `MMS_UPDATE_CHANNEL` come from the environment or, if unset there,
//! from the repo's `.env` file (gitignored, never committed). See docs/app-api.md.

use std::path::Path;

const VARS: [&str; 2] = ["MMS_APP_API_KEY", "MMS_UPDATE_CHANNEL"];

fn main() {
    let env_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    println!("cargo:rerun-if-changed={}", env_file.display());
    let file = std::fs::read_to_string(&env_file).unwrap_or_default();

    for var in VARS {
        println!("cargo:rerun-if-env-changed={var}");
        if std::env::var_os(var).is_some() {
            continue; // Cargo already passes it through to `option_env!`.
        }
        if let Some(value) = read_var(&file, var) {
            println!("cargo:rustc-env={var}={value}");
        }
    }

    tauri_build::build()
}

/// `KEY=value` lines; `#` comments, `export ` prefixes and surrounding quotes are allowed.
fn read_var(file: &str, key: &str) -> Option<String> {
    file.lines().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (k, v) = line.split_once('=')?;
        if k.trim() != key || line.starts_with('#') {
            return None;
        }
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        (!v.is_empty()).then(|| v.to_string())
    })
}
