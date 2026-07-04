use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct DesktopEntryInput<'a> {
    pub name: &'a str,
    pub generic_name: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub exec_path: &'a Path,
    pub icon_name: &'a str,
    pub categories: &'a [String],
    pub terminal: bool,
}

pub fn write_desktop_entry(path: &Path, input: &DesktopEntryInput<'_>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create desktop dir: {}", parent.display()))?;
    }

    let categories = if input.categories.is_empty() {
        "Utility;".to_string()
    } else {
        let mut joined = input.categories.join(";");
        if !joined.ends_with(';') { joined.push(';'); }
        joined
    };

    let mut text = String::new();
    text.push_str("[Desktop Entry]\n");
    text.push_str("Type=Application\n");
    text.push_str(&format!("Name={}\n", escape_value(input.name)));
    if let Some(generic) = input.generic_name {
        text.push_str(&format!("GenericName={}\n", escape_value(generic)));
    }
    if let Some(comment) = input.comment {
        text.push_str(&format!("Comment={}\n", escape_value(comment)));
    }
    text.push_str(&format!("Exec={} %U\n", quote_exec(input.exec_path)));
    text.push_str(&format!("Icon={}\n", escape_value(input.icon_name)));
    text.push_str(&format!("Terminal={}\n", if input.terminal { "true" } else { "false" }));
    text.push_str(&format!("Categories={}\n", categories));
    text.push_str("StartupNotify=true\n");

    fs::write(path, text).with_context(|| format!("failed to write desktop entry: {}", path.display()))?;
    Ok(())
}

fn escape_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn quote_exec(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.contains(' ') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}
