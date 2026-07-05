use anyhow::{Context, Result};
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_METADATA_BYTES: u64 = 512 * 1024;
const MAX_OUTPUT_BYTES: u64 = 128 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionProbeResult {
    pub version: String,
    pub source: String,
}

/// Detect version after the archive has been copied into the final app dir.
///
/// Order:
/// 1. Metadata files: product-info.json, package.json, VERSION, etc.
/// 2. Runtime command probes: --version, -version, -v, version, -V, then no args.
///
/// Important: runtime probes execute code from the extracted tarball. Only call this
/// after the user has decided to install the package, and expose a way to disable it.
pub fn detect_installed_version(
    app_dir: &Path,
    exec_path_inside_app: &Path,
    command_name: &str,
    app_id: &str,
    app_name: &str,
) -> Result<Option<VersionProbeResult>> {
    if let Some(found) = detect_from_metadata(app_dir)? {
        return Ok(Some(found));
    }

    let exec_abs = app_dir.join(exec_path_inside_app);
    detect_from_command(&exec_abs, app_dir, command_name, app_id, app_name)
}

fn detect_from_metadata(app_dir: &Path) -> Result<Option<VersionProbeResult>> {
    let json_candidates = [
        PathBuf::from("product-info.json"),          // JetBrains IDEs
        PathBuf::from("package.json"),               // Node / Electron apps
        PathBuf::from("resources/app/package.json"), // Electron apps
        PathBuf::from("app/package.json"),
    ];

    for rel in json_candidates {
        let path = app_dir.join(&rel);
        if !path.is_file() {
            continue;
        }

        let text = read_small_text(&path)?;
        if text.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            if let Some(version) = json
                .get("version")
                .and_then(Value::as_str)
                .and_then(clean_version)
            {
                return Ok(Some(VersionProbeResult {
                    version,
                    source: rel.display().to_string(),
                }));
            }

            // Some products expose buildNumber but not version. Use it only as a fallback.
            if let Some(version) = json
                .get("buildNumber")
                .and_then(Value::as_str)
                .and_then(clean_version)
            {
                return Ok(Some(VersionProbeResult {
                    version,
                    source: format!("{}:buildNumber", rel.display()),
                }));
            }
        }
    }

    let text_candidates = [
        PathBuf::from("VERSION"),
        PathBuf::from("VERSION.txt"),
        PathBuf::from("version.txt"),
        PathBuf::from("build.txt"),
        PathBuf::from("RELEASE"),
    ];

    for rel in text_candidates {
        let path = app_dir.join(&rel);
        if !path.is_file() {
            continue;
        }

        let text = read_small_text(&path)?;
        if let Some(version) = parse_version_from_text(&text, "", "", "") {
            return Ok(Some(VersionProbeResult {
                version,
                source: rel.display().to_string(),
            }));
        }
    }

    Ok(None)
}

fn detect_from_command(
    exec_abs: &Path,
    app_dir: &Path,
    command_name: &str,
    app_id: &str,
    app_name: &str,
) -> Result<Option<VersionProbeResult>> {
    if !exec_abs.is_file() {
        return Ok(None);
    }

    let probes: &[&[&str]] = &[
        &["--version"],
        &["-version"],
        &["-v"],
        &["version"],
        &["-V"],
        &[],
    ];

    for args in probes {
        let output = match run_probe(exec_abs, app_dir, args, PROBE_TIMEOUT) {
            Ok(output) => output,
            Err(_) => continue,
        };

        if let Some(version) = parse_version_from_text(&output, command_name, app_id, app_name) {
            let rendered_args = if args.is_empty() {
                "<no args>".to_string()
            } else {
                args.join(" ")
            };

            return Ok(Some(VersionProbeResult {
                version,
                source: format!("{} {}", exec_abs.display(), rendered_args),
            }));
        }
    }

    Ok(None)
}

fn run_probe(exec_abs: &Path, app_dir: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(exec_abs)
        .args(args)
        .current_dir(app_dir)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run version probe: {}", exec_abs.display()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_handle = stdout.map(|stream| thread::spawn(move || read_limited(stream, MAX_OUTPUT_BYTES)));
    let err_handle = stderr.map(|stream| thread::spawn(move || read_limited(stream, MAX_OUTPUT_BYTES)));

    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }

        thread::sleep(Duration::from_millis(40));
    }

    let mut output = String::new();

    if let Some(handle) = out_handle {
        output.push_str(&handle.join().unwrap_or_default());
    }

    if let Some(handle) = err_handle {
        if !output.ends_with('\n') && !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&handle.join().unwrap_or_default());
    }

    Ok(output)
}

fn read_limited<R: Read>(stream: R, limit: u64) -> String {
    let mut text = String::new();
    let mut limited = stream.take(limit);
    let _ = limited.read_to_string(&mut text);
    text
}

fn read_small_text(path: &Path) -> Result<String> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_METADATA_BYTES {
        return Ok(String::new());
    }

    fs::read_to_string(path).with_context(|| format!("failed to read metadata file: {}", path.display()))
}

fn clean_version(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches(['\'', '"']);
    if trimmed.is_empty() {
        return None;
    }

    parse_version_from_text(trimmed, "", "", "")
        .or_else(|| Some(trimmed.trim_start_matches(['v', 'V']).to_string()))
}

pub fn parse_version_from_text(
    text: &str,
    command_name: &str,
    app_id: &str,
    app_name: &str,
) -> Option<String> {
    let version_re = Regex::new(
        r#"(?ix)
        \b
        (?:version\s*)?
        ["']?
        v?
        (\d{1,4}(?:\.\d{1,8}){1,5}(?:[-+~._][0-9A-Za-z][0-9A-Za-z._+-]*)?)
        ["']?
        "#,
    )
    .unwrap();

    let needles = [command_name, app_id, app_name]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let mut best: Option<(i32, String)> = None;

    for (line_index, line) in text.lines().take(80).enumerate() {
        let lower = line.to_ascii_lowercase();

        for caps in version_re.captures_iter(line) {
            let Some(raw) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };

            if looks_like_false_positive(raw, &lower) {
                continue;
            }

            let mut score = 1000 - line_index as i32;

            if line_index == 0 {
                score += 120;
            }
            if lower.contains("version") {
                score += 80;
            }
            if needles.iter().any(|needle| lower.contains(needle)) {
                score += 80;
            }
            if lower.contains("build date")
                || lower.contains("build time")
                || lower.contains("commit")
                || lower.contains("runtime")
                || lower.contains("luajit")
            {
                score -= 160;
            }

            let version = raw.trim_start_matches(['v', 'V']).to_string();

            match &best {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best = Some((score, version)),
            }
        }
    }

    best.map(|(_, version)| version)
}

fn looks_like_false_positive(version: &str, line_lower: &str) -> bool {
    let lower = version.to_ascii_lowercase();

    if line_lower.contains("copyright") || line_lower.contains("license") {
        return true;
    }

    // Usually timestamps/build ids, unless they are dev/hash suffix versions.
    if !lower.contains('-') && !lower.contains('+') {
        if lower.split('.').any(|part| part.len() > 8) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nvim_dev_version() {
        let text = "NVIM v0.13.0-dev-915+g3b8c19ea46\nLuaJIT 2.1.1782726002";
        assert_eq!(
            parse_version_from_text(text, "nvim", "nvim", "Neovim").as_deref(),
            Some("0.13.0-dev-915+g3b8c19ea46")
        );
    }

    #[test]
    fn parses_blender_version() {
        let text = "Blender 5.1.2\n\tbuild date: 2026-05-19";
        assert_eq!(
            parse_version_from_text(text, "blender", "blender", "Blender").as_deref(),
            Some("5.1.2")
        );
    }

    #[test]
    fn parses_java_version() {
        let text = "openjdk version \"21.0.11\" 2026-04-21 LTS";
        assert_eq!(
            parse_version_from_text(text, "java", "java", "Java").as_deref(),
            Some("21.0.11")
        );
    }

    #[test]
    fn parses_jetbrains_style_version() {
        let text = "2026.1.4";
        assert_eq!(
            parse_version_from_text(text, "idea", "idea", "IntelliJ IDEA").as_deref(),
            Some("2026.1.4")
        );
    }
}
