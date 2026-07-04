use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeDesktop {
    pub name: Option<String>,
    pub generic_name: Option<String>,
    pub categories: Option<Vec<String>>,
    pub terminal: Option<bool>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppRecipe {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub exec: Option<String>,
    pub command: Option<String>,
    pub icon: Option<String>,
    pub desktop: Option<RecipeDesktop>,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::BTreeMap<String, String>>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallInput {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub exec: Option<String>,
    pub command: Option<String>,
    pub icon: Option<String>,
    pub recipe: Option<AppRecipe>,
    pub force: bool,
    pub interactive_config: bool,
}

pub fn load_recipe(path: &Path) -> Result<AppRecipe> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read recipe: {}", path.display()))?;
    let recipe: AppRecipe = serde_yaml::from_str(&text)
        .with_context(|| format!("failed to parse YAML recipe: {}", path.display()))?;
    Ok(recipe)
}

pub fn sanitize_id(value: &str) -> String {
    let lowered = value.trim().to_ascii_lowercase();
    let mut out = String::new();
    let mut last_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub fn sanitize_command(value: &str) -> String {
    let lowered = value.trim().to_ascii_lowercase();
    let mut out = String::new();
    let mut last_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub fn display_name_from_id(id: &str) -> String {
    id.split(['-', '_', '.'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
