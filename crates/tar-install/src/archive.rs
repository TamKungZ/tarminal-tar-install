use crate::filename::FilenameGuess;
use anyhow::{anyhow, Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use xz2::read::XzDecoder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub executable: bool,
    pub size: u64,
    pub unsafe_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutableCandidate {
    pub path: PathBuf,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInspection {
    pub archive_path: PathBuf,
    pub filename_guess: FilenameGuess,
    pub safe: bool,
    pub entries_count: usize,
    pub common_root: Option<PathBuf>,
    pub executable_candidates: Vec<ExecutableCandidate>,
    pub icon_candidates: Vec<PathBuf>,
    pub desktop_candidates: Vec<PathBuf>,
    pub manifest_candidates: Vec<PathBuf>,
    pub unsafe_entries: Vec<ArchiveEntry>,
    pub notes: Vec<String>,
}

pub fn open_tar_reader(path: &Path) -> Result<Box<dyn Read>> {
    let file = File::open(path).with_context(|| format!("failed to open archive: {}", path.display()))?;
    let reader = BufReader::new(file);
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        Ok(Box::new(XzDecoder::new(reader)))
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Ok(Box::new(GzDecoder::new(reader)))
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        Ok(Box::new(BzDecoder::new(reader)))
    } else if name.ends_with(".tar") {
        Ok(Box::new(reader))
    } else {
        Err(anyhow!("unsupported archive extension; supported: .tar.xz, .txz, .tar.gz, .tgz, .tar.bz2, .tbz2, .tar"))
    }
}

pub fn inspect_archive(path: &Path) -> Result<ArchiveInspection> {
    let guess = crate::filename::guess_from_filename(path);
    let reader = open_tar_reader(path)?;
    let mut archive = Archive::new(reader);
    let mut entries = Vec::new();

    for entry in archive.entries().context("failed to read tar entries")? {
        let entry = entry.context("failed to read tar entry")?;
        let header = entry.header();
        let entry_type = header.entry_type();
        let raw_path = entry.path().context("failed to read tar entry path")?.to_path_buf();
        let mode = header.mode().unwrap_or(0);
        let size = header.size().unwrap_or(0);
        let unsafe_reason = unsafe_path_reason(&raw_path);
        entries.push(ArchiveEntry {
            path: raw_path,
            is_file: entry_type.is_file(),
            is_dir: entry_type.is_dir(),
            is_symlink: entry_type.is_symlink(),
            executable: (mode & 0o111) != 0,
            size,
            unsafe_reason,
        });
    }

    let unsafe_entries: Vec<_> = entries.iter().filter(|e| e.unsafe_reason.is_some()).cloned().collect();
    let safe = unsafe_entries.is_empty();
    let common_root = common_root(&entries);
    let executable_candidates = executable_candidates(&entries, &guess);
    let icon_candidates = entries.iter()
        .filter(|e| e.is_file && is_icon_path(&e.path))
        .map(|e| e.path.clone())
        .collect();
    let desktop_candidates = entries.iter()
        .filter(|e| e.is_file && e.path.extension() == Some(OsStr::new("desktop")))
        .map(|e| e.path.clone())
        .collect();
    let manifest_candidates = entries.iter()
        .filter(|e| e.is_file && is_manifest_path(&e.path))
        .map(|e| e.path.clone())
        .collect();

    let mut notes = guess.notes.clone();
    if !safe {
        notes.push("archive contains unsafe paths and must not be extracted directly".to_string());
    }
    if executable_candidates.is_empty() {
        notes.push("no executable candidate was confidently detected".to_string());
    }

    Ok(ArchiveInspection {
        archive_path: path.to_path_buf(),
        filename_guess: guess,
        safe,
        entries_count: entries.len(),
        common_root,
        executable_candidates,
        icon_candidates,
        desktop_candidates,
        manifest_candidates,
        unsafe_entries,
        notes,
    })
}

pub fn unsafe_path_reason(path: &Path) -> Option<String> {
    if path.is_absolute() {
        return Some("absolute path".to_string());
    }
    for comp in path.components() {
        match comp {
            Component::ParentDir => return Some("path traversal using ..".to_string()),
            Component::RootDir | Component::Prefix(_) => return Some("root/prefix path".to_string()),
            _ => {}
        }
    }
    None
}

fn common_root(entries: &[ArchiveEntry]) -> Option<PathBuf> {
    let mut roots = BTreeSet::new();
    for e in entries {
        if let Some(first) = e.path.components().next() {
            if let Component::Normal(s) = first {
                roots.insert(PathBuf::from(s));
            }
        }
    }
    if roots.len() == 1 { roots.into_iter().next() } else { None }
}

fn executable_candidates(entries: &[ArchiveEntry], guess: &FilenameGuess) -> Vec<ExecutableCandidate> {
    let app = guess.app.clone().unwrap_or_default();
    let arch = guess.architecture.clone().unwrap_or_default();
    let mut candidates = Vec::new();

    for e in entries {
        if !e.is_file || !e.executable || e.unsafe_reason.is_some() {
            continue;
        }
        let file_name = e.path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        if looks_like_library_or_helper(file_name) {
            continue;
        }
        let score = score_executable(file_name, &app, &arch, &e.path);
        if score > 0 {
            candidates.push(ExecutableCandidate {
                path: e.path.clone(),
                score,
                reason: explain_score(file_name, &app, &arch, score),
            });
        }
    }
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    candidates
}

fn score_executable(file_name: &str, app: &str, arch: &str, path: &Path) -> i32 {
    let lower = file_name.to_ascii_lowercase();
    let app_lower = app.to_ascii_lowercase();
    let app_us = app_lower.replace('-', "_");
    let app_dash = app_lower.replace('_', "-");
    let mut score = 1;

    if !app_lower.is_empty() {
        if lower == app_lower || lower == app_us || lower == app_dash {
            score += 100;
        }
        if !arch.is_empty() && (lower == format!("{}-{}", app_dash, arch) || lower == format!("{}_{}", app_us, arch)) {
            score += 85;
        }
        if lower.starts_with(&app_lower) || lower.starts_with(&app_us) || lower.starts_with(&app_dash) {
            score += 45;
        }
        if path.iter().any(|p| p.to_string_lossy().eq_ignore_ascii_case("bin")) {
            score += 15;
        }
    }

    if lower.ends_with(".sh") || lower.ends_with(".run") {
        score += 5;
    }
    score
}

fn explain_score(file_name: &str, app: &str, arch: &str, score: i32) -> String {
    if !app.is_empty() && file_name.eq_ignore_ascii_case(app) {
        "exact filename matches guessed app name".to_string()
    } else if !app.is_empty() && !arch.is_empty() && file_name.to_ascii_lowercase().contains(&arch.to_ascii_lowercase()) {
        "filename contains guessed app and architecture pattern".to_string()
    } else if score >= 45 {
        "filename starts with guessed app name".to_string()
    } else {
        "executable file".to_string()
    }
}

fn looks_like_library_or_helper(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".so")
        || lower.contains(".so.")
        || lower.ends_with(".dll")
        || lower.ends_with(".dylib")
        || lower.ends_with(".a")
        || lower == "crashpad_handler"
        || lower == "chrome-sandbox"
}

fn is_icon_path(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "svg" | "xpm") {
        return false;
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    name.contains("icon") || name.contains("logo") || path.iter().any(|p| p.to_string_lossy().eq_ignore_ascii_case("icons"))
}

fn is_manifest_path(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    matches!(name.as_str(), "tarapp.yml" | "tarapp.yaml" | ".tarapp.yml" | ".tarapp.yaml" | "manifest.yml" | "manifest.yaml")
}

pub fn read_text_entry(path: &Path, entry_path: &Path, max_bytes: u64) -> Result<String> {
    let reader = open_tar_reader(path)?;
    let mut archive = Archive::new(reader);
    for entry in archive.entries().context("failed to read tar entries")? {
        let mut entry = entry.context("failed to read tar entry")?;
        let raw_path = entry.path().context("failed to read tar entry path")?.to_path_buf();
        if raw_path == entry_path {
            let size = entry.header().size().unwrap_or(0);
            if size > max_bytes {
                return Err(anyhow!("entry is too large to read as text: {}", raw_path.display()));
            }
            let mut text = String::new();
            entry.read_to_string(&mut text).with_context(|| format!("failed to read text entry: {}", raw_path.display()))?;
            return Ok(text);
        }
    }
    Err(anyhow!("entry not found: {}", entry_path.display()))
}
