use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallScope {
    User,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallTargets {
    pub scope: InstallScope,
    pub app_id: String,
    pub command_name: String,
    pub app_dir: PathBuf,
    pub command_path: PathBuf,
    pub desktop_path: PathBuf,
    pub icon_dir: PathBuf,
    pub state_path: PathBuf,
}

pub fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set; cannot resolve user install directories")
}

fn xdg_data_home() -> Result<PathBuf> {
    Ok(env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".local/share")))
}

fn xdg_state_home() -> Result<PathBuf> {
    Ok(env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".local/state")))
}

pub fn targets(scope: InstallScope, app_id: &str, command_name: &str) -> Result<InstallTargets> {
    let cleaned_id = crate::recipe::sanitize_id(app_id);
    let cleaned_cmd = crate::recipe::sanitize_command(command_name);

    let t = match scope {
        InstallScope::User => {
            let data = xdg_data_home()?;
            let state = xdg_state_home()?;
            InstallTargets {
                scope,
                app_id: cleaned_id.clone(),
                command_name: cleaned_cmd.clone(),
                app_dir: data.join("tarapp/apps").join(&cleaned_id),
                command_path: home_dir()?.join(".local/bin").join(&cleaned_cmd),
                desktop_path: data.join("applications").join(format!("{}.desktop", cleaned_id)),
                icon_dir: data.join("icons/hicolor/256x256/apps"),
                state_path: state.join("tarapp/apps.json"),
            }
        }
        InstallScope::System => InstallTargets {
            scope,
            app_id: cleaned_id.clone(),
            command_name: cleaned_cmd.clone(),
            app_dir: PathBuf::from("/opt").join(&cleaned_id),
            command_path: PathBuf::from("/usr/local/bin").join(&cleaned_cmd),
            desktop_path: PathBuf::from("/usr/share/applications").join(format!("{}.desktop", cleaned_id)),
            icon_dir: PathBuf::from("/usr/share/icons/hicolor/256x256/apps"),
            state_path: PathBuf::from("/var/lib/tarapp/apps.json"),
        },
    };

    Ok(t)
}
