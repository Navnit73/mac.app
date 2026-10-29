//! Small persisted settings (JSON in Application Support). Exclusions for now.

use std::path::PathBuf;

use duobolt_core::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Absolute paths the user excluded from scans.
    pub exclusions: Vec<String>,
}

impl Settings {
    /// Glob patterns for the scanner: each path itself and everything below it.
    pub fn exclusion_globs(&self) -> Vec<String> {
        self.exclusions
            .iter()
            .flat_map(|p| {
                let esc = escape_glob(p);
                [esc.clone(), format!("{esc}/**")]
            })
            .collect()
    }
}

/// Escape glob metacharacters so a literal path matches only itself.
fn escape_glob(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for c in p.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/DuoBolt/settings.json"))
}

pub fn load() -> Settings {
    file()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), AppError> {
    let f = file().ok_or_else(|| AppError::Internal("HOME is not set".into()))?;
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(s).map_err(|e| AppError::Internal(e.to_string()))?;
    // Write then rename so a crash never leaves a half-written file.
    let tmp = f.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, f)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_cover_path_and_children_and_escape_metacharacters() {
        let s = Settings {
            exclusions: vec!["/Users/me/a[1]".into()],
        };
        assert_eq!(
            s.exclusion_globs(),
            ["/Users/me/a\\[1\\]", "/Users/me/a\\[1\\]/**"]
        );
    }
}
