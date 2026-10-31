//! Small persisted settings (JSON in Application Support). Exclusions for now.

use std::path::PathBuf;

use mymacstorage_core::AppError;
use serde::{Deserialize, Serialize};

/// Folders in `~/Library/Application Support` before and after the app was renamed.
const LEGACY_DATA_DIR: &str = "DuoBolt";
pub(crate) const DATA_DIR: &str = "MyMacStorage";

/// One-time move of the data folder from the app's old name, so settings, the activity log
/// and undo history survive the rename. Never overwrites: if the new folder already exists,
/// the old one is left alone. Runs before anything opens files in the folder.
pub fn migrate_legacy_data_dir() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let support = PathBuf::from(home).join("Library/Application Support");
    if let Err(e) = migrate_dir(&support.join(LEGACY_DATA_DIR), &support.join(DATA_DIR)) {
        tracing::warn!("could not migrate legacy data folder: {e}");
    }
}

fn migrate_dir(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<bool> {
    // symlink_metadata: never follow a symlink planted at the old location.
    let old_is_dir = std::fs::symlink_metadata(old).is_ok_and(|m| m.is_dir());
    if !old_is_dir || std::fs::symlink_metadata(new).is_ok() {
        return Ok(false);
    }
    std::fs::rename(old, new)?;
    Ok(true)
}

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
    Some(
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(DATA_DIR)
            .join("settings.json"),
    )
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

    #[test]
    fn legacy_data_dir_moves_once_and_never_overwrites() {
        let base = std::env::temp_dir().join(format!("mms-migrate-{}", std::process::id()));
        let (old, new) = (base.join("old"), base.join("new"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("activity.sqlite"), b"log").unwrap();

        assert!(migrate_dir(&old, &new).unwrap());
        assert_eq!(std::fs::read(new.join("activity.sqlite")).unwrap(), b"log");
        assert!(!old.exists());

        // Old folder reappears while the new one exists: leave both alone.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("activity.sqlite"), b"stale").unwrap();
        assert!(!migrate_dir(&old, &new).unwrap());
        assert_eq!(std::fs::read(new.join("activity.sqlite")).unwrap(), b"log");

        // Nothing to migrate.
        let _ = std::fs::remove_dir_all(&old);
        assert!(!migrate_dir(&old, &base.join("other")).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }
}
