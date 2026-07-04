use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilenameGuess {
    pub raw_stem: String,
    pub app: Option<String>,
    pub version: Option<String>,
    pub os: Option<String>,
    pub architecture: Option<String>,
    pub confidence: f32,
    pub notes: Vec<String>,
}

const ARCH_TOKENS: &[&str] = &[
    "x64", "x86_64", "amd64", "x86", "i386", "i686", "arm64", "aarch64", "armv7", "armhf",
];
const OS_TOKENS: &[&str] = &[
    "linux", "gnu-linux", "linux64", "linux-x64", "linux-x86_64", "linux-amd64", "ubuntu", "debian",
];

pub fn strip_archive_extensions(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for ext in [
        ".tar.xz", ".tar.gz", ".tar.bz2", ".tar.zst", ".tgz", ".txz", ".tbz2", ".tar",
    ] {
        if lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string()
}

pub fn guess_from_filename(path: &Path) -> FilenameGuess {
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
    let stem = strip_archive_extensions(filename);
    let token_re = Regex::new(r"[-_ .]+" ).unwrap();
    let version_re = Regex::new(r"^v?\d+(?:\.\d+){0,3}(?:[-+][A-Za-z0-9._-]+)?$").unwrap();

    let tokens: Vec<String> = token_re
        .split(&stem)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut version = None;
    let mut os = None;
    let mut arch = None;
    let mut cutoff = tokens.len();
    let mut notes = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();
        if version.is_none() && version_re.is_match(&lower) {
            version = Some(token.trim_start_matches('v').to_string());
            cutoff = cutoff.min(i);
            continue;
        }
        if os.is_none() && OS_TOKENS.contains(&lower.as_str()) {
            os = Some(lower.clone());
            cutoff = cutoff.min(i);
            continue;
        }
        if arch.is_none() && ARCH_TOKENS.contains(&lower.as_str()) {
            arch = Some(normalize_arch(&lower));
            cutoff = cutoff.min(i);
            continue;
        }
    }

    // Handle combined tail like app-linux-x64 where split already finds linux and x64.
    let app = if cutoff > 0 {
        Some(tokens[..cutoff].join("-"))
    } else {
        None
    };

    let mut confidence = 0.0;
    if app.is_some() { confidence += 0.45; }
    if version.is_some() { confidence += 0.2; }
    if os.is_some() { confidence += 0.15; }
    if arch.is_some() { confidence += 0.15; }
    if app.is_none() {
        notes.push("filename did not match <app>-<version>-<os>-<architecture> well".to_string());
    }
    if version.is_none() {
        notes.push("version was not found in filename".to_string());
    }
    if os.is_none() {
        notes.push("OS was not found in filename".to_string());
    }
    if arch.is_none() {
        notes.push("architecture was not found in filename".to_string());
    }

    FilenameGuess {
        raw_stem: stem,
        app,
        version,
        os,
        architecture: arch,
        confidence,
        notes,
    }
}

pub fn normalize_arch(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "x64" | "x86_64" | "amd64" => "x86_64".to_string(),
        "arm64" | "aarch64" => "aarch64".to_string(),
        v => v.to_string(),
    }
}
