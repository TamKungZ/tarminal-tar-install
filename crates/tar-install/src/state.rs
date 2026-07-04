use crate::paths::InstallScope;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub scope: InstallScope,
    pub install_dir: PathBuf,
    pub command_name: String,
    pub command_path: PathBuf,
    pub desktop_path: PathBuf,
    pub icon_paths: Vec<PathBuf>,
    pub source_archive: Option<PathBuf>,
    pub source_sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDb {
    pub apps: BTreeMap<String, InstalledApp>,
}

pub fn load_state(path: &Path) -> Result<StateDb> {
    if !path.exists() {
        return Ok(StateDb::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("failed to read state DB: {}", path.display()))?;
    let db = serde_json::from_str(&text).with_context(|| format!("failed to parse state DB: {}", path.display()))?;
    Ok(db)
}

pub fn save_state(path: &Path, db: &StateDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create state dir: {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(db)?;
    fs::write(path, text).with_context(|| format!("failed to write state DB: {}", path.display()))?;
    Ok(())
}
