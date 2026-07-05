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
    "x64", "x86_64", "amd64", "x86", "386", "i386", "i486", "i586", "i686", "arm64", "aarch64",
    "arm", "armv6", "armv6l", "armv7", "armv7l", "armv8", "armhf", "armel", "ppc", "ppc64",
    "ppc64le", "s390x", "mips", "mipsel", "mips64", "mips64el", "riscv64", "riscv32", "sparc64",
    "loongarch64", "universal", "universal2", "wasm32", "noarch",
];

// Canonical OS name -> every filename token that should map to it. This is the
// single source of truth: normalize_os() is generated from this table, so
// there is nowhere else that needs to stay in sync when a new alias is added.
const OS_TOKENS: &[(&str, &[&str])] = &[
    (
        "linux",
        &[
            "linux",
            "gnu-linux",
            "linux64",
            "linux-x64",
            "linux-x86_64",
            "linux-amd64",
            "ubuntu",
            "debian",
            "fedora",
            "rhel",
            "centos",
            "opensuse",
            "suse",
            // musl is a libc, not an OS, but in practice it only ever shows up
            // paired with Linux targets (e.g. x86_64-unknown-linux-musl), so
            // treating it as a Linux marker is a useful approximation.
            "musl",
        ],
    ),
    ("macos", &["macos", "darwin", "osx", "mac"]),
    (
        "windows",
        &["windows", "win", "win32", "win64", "mingw", "msvc"],
    ),
    ("freebsd", &["freebsd"]),
    ("openbsd", &["openbsd"]),
    ("netbsd", &["netbsd"]),
    ("solaris", &["solaris", "sunos", "illumos"]),
    ("aix", &["aix"]),
    ("android", &["android"]),
    ("ios", &["ios", "iphoneos"]),
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
    let tokens = tokenize_filename_stem(&stem);

    let mut version = None;
    let mut version_index = None;
    let mut os = None;
    let mut os_index = None;
    let mut arch = None;
    let mut arch_index = None;
    let mut notes = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();

        if os.is_none() {
            if let Some(normalized) = normalize_os(&lower) {
                os = Some(normalized);
                os_index = Some(i);
            }
        }

        if arch.is_none() {
            if ARCH_TOKENS.contains(&lower.as_str()) {
                arch = Some(normalize_arch(&lower));
                arch_index = Some(i);
            }
        }
    }

    for (i, token) in tokens.iter().enumerate() {
        if is_version_start(token) {
            let mut parts = Vec::new();
            parts.push(token.trim_start_matches(['v', 'V']).to_string());

            let mut j = i + 1;
            while j < tokens.len() {
                let lower = tokens[j].to_ascii_lowercase();
                if normalize_os(&lower).is_some() || ARCH_TOKENS.contains(&lower.as_str()) {
                    break;
                }

                // If a version starts before OS/arch, suffix tokens like
                // "dev", "beta", "906+gabcdef" are usually part of the version.
                // Avoid treating weak single-number arch fragments such as "64" as versions;
                // they never match is_version_start().
                if looks_like_version_suffix(&tokens[j]) {
                    parts.push(tokens[j].clone());
                    j += 1;
                } else {
                    break;
                }
            }

            version = Some(parts.join("-"));
            version_index = Some(i);
            break;
        }
    }

    let cutoff = [version_index, os_index, arch_index]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(tokens.len());

    let app = if cutoff > 0 {
        Some(tokens[..cutoff].join("-"))
    } else {
        None
    };

    let mut confidence = 0.0;
    if app.is_some() {
        confidence += 0.45;
    }
    if version.is_some() {
        confidence += 0.2;
    }
    if os.is_some() {
        confidence += 0.15;
    }
    if arch.is_some() {
        confidence += 0.15;
    }

    if app.is_none() {
        notes.push("filename did not contain a reliable app name".to_string());
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

fn tokenize_filename_stem(stem: &str) -> Vec<String> {
    let dash_space_re = Regex::new(r"[-\s]+").unwrap();
    let mut tokens = Vec::new();

    for part in dash_space_re.split(stem).filter(|s| !s.is_empty()) {
        tokens.extend(expand_part(part));
    }

    tokens
}

/// Whether `value`, taken as a whole, is already a recognized field
/// (an exact arch token, a recognized OS token, or a version string).
/// Such tokens are never worth splitting further.
fn is_whole_known_field(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ARCH_TOKENS.contains(&lower.as_str()) || normalize_os(&lower).is_some() || is_version_start(value)
}

/// Splits a single dash-delimited `part` into its constituent tokens,
/// handling the two other separators that show up in the wild: underscores
/// (e.g. Go-style "myapp_1.2.3_linux_amd64") and dots (e.g.
/// "myapp.1.2.3.windows.x64"). Falls back to returning `part` unchanged when
/// splitting wouldn't reveal any recognizable field, so we don't mangle
/// legitimately dotted/underscored app names.
fn expand_part(part: &str) -> Vec<String> {
    if is_whole_known_field(part) {
        return vec![part.to_string()];
    }

    if part.contains('_') && !part.contains('.') {
        return split_on_underscore(part);
    }

    if part.contains('.') {
        if let Some(split) = split_dotted(part) {
            return split;
        }
    }

    vec![part.to_string()]
}

/// Splits on underscores, merging "x86" + "64" back into "x86_64" since that
/// arch token itself contains an underscore.
fn split_on_underscore(part: &str) -> Vec<String> {
    let pieces: Vec<&str> = part.split('_').filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        if pieces[i].eq_ignore_ascii_case("x86") && pieces.get(i + 1).map(|s| s.eq_ignore_ascii_case("64")).unwrap_or(false) {
            out.push("x86_64".to_string());
            i += 2;
            continue;
        }
        out.push(pieces[i].to_string());
        i += 1;
    }
    out
}

/// Attempts to split a dot-joined `part` (e.g. "myapp.1.2.3.windows.x64")
/// into separate tokens. Returns `None` when there's no evidence a split is
/// warranted, so callers can keep an ordinary dotted name (like "my.app")
/// intact as a single token instead of shredding it.
///
/// Two strategies are tried:
/// 1. Find a version-like substring (e.g. "1.2.3") inside the string; split
///    around it, keeping the version's own dots intact, and recurse into the
///    leading/trailing chunks (splitting them on '.' and, if needed, '_').
/// 2. If no version substring is found, split purely on '.' and check
///    whether at least one resulting piece is a recognized OS/arch token
///    (e.g. "app.linux.x64" with no version at all).
fn split_dotted(part: &str) -> Option<Vec<String>> {
    let version_re =
        Regex::new(r"(?i)v?\d+(?:\.\d+){1,3}(?:[-+][A-Za-z0-9._+-]+)?").unwrap();

    if let Some(m) = version_re.find(part) {
        let pre = part[..m.start()].trim_matches(|c| c == '.' || c == '-');
        let post = part[m.end()..].trim_matches(|c| c == '.' || c == '-');

        let mut out = Vec::new();
        for piece in pre.split('.').filter(|s| !s.is_empty()) {
            out.extend(expand_part(piece));
        }
        out.push(m.as_str().to_string());
        for piece in post.split('.').filter(|s| !s.is_empty()) {
            out.extend(expand_part(piece));
        }
        return Some(out);
    }

    let pieces: Vec<&str> = part.split('.').filter(|s| !s.is_empty()).collect();
    let has_os_or_arch_piece = pieces.iter().any(|p| {
        let lower = p.to_ascii_lowercase();
        ARCH_TOKENS.contains(&lower.as_str()) || normalize_os(&lower).is_some()
    });
    if pieces.len() > 1 && has_os_or_arch_piece {
        let mut out = Vec::new();
        for piece in pieces {
            out.extend(expand_part(piece));
        }
        return Some(out);
    }

    None
}

fn is_version_start(token: &str) -> bool {
    // Require at least one dot so weak numeric fragments like "64" or "86"
    // are not treated as versions. Case-insensitive so both "v1.2.3" and
    // "V1.2.3" (e.g. "App-V1.2.3-win-x64.zip") are recognized.
    let version_re =
        Regex::new(r"(?i)^v?\d+\.\d+(?:\.\d+){0,3}(?:[-+][A-Za-z0-9._-]+)?$").unwrap();
    version_re.is_match(token)
}

fn looks_like_version_suffix(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();

    if lower.is_empty() {
        return false;
    }

    if normalize_os(&lower).is_some() || ARCH_TOKENS.contains(&lower.as_str()) {
        return false;
    }

    // Avoid turning common filename/packaging words into version suffixes.
    // Note: real OS/arch names are already excluded above via normalize_os()
    // and ARCH_TOKENS, so this list only needs generic packaging vocabulary
    // that isn't an OS or arch name (unlike "linux"/"macos"/"windows", which
    // normalize_os() already catches).
    if matches!(
        lower.as_str(),
        "portable"
            | "installer"
            | "setup"
            | "standalone"
            | "full"
            | "lite"
            | "release"
            | "debug"
            | "final"
    ) {
        return false;
    }

    let suffix_re = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._+]*$").unwrap();
    suffix_re.is_match(token)
}

pub fn normalize_arch(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "x64" | "x86_64" | "amd64" => "x86_64".to_string(),
        "arm64" | "aarch64" => "aarch64".to_string(),
        "386" | "i386" | "i486" | "i586" | "i686" | "x86" => "x86".to_string(),
        v => v.to_string(),
    }
}

pub fn normalize_os(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    OS_TOKENS
        .iter()
        .find(|(_, aliases)| aliases.contains(&lower.as_str()))
        .map(|(canonical, _)| canonical.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blender_version_without_splitting_dots() {
        let guess = guess_from_filename(Path::new("blender-5.1.2-linux-x64.tar.xz"));
        assert_eq!(guess.app.as_deref(), Some("blender"));
        assert_eq!(guess.version.as_deref(), Some("5.1.2"));
        assert_eq!(guess.os.as_deref(), Some("linux"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parses_macos_arch_without_false_version() {
        let guess = guess_from_filename(Path::new("nvim-macos-x86_64.tar.gz"));
        assert_eq!(guess.app.as_deref(), Some("nvim"));
        assert_eq!(guess.version.as_deref(), None);
        assert_eq!(guess.os.as_deref(), Some("macos"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parses_dev_version_suffix() {
        let guess = guess_from_filename(Path::new("nvim-v0.13.0-dev-906+g44baf03f2a-linux-x86_64.tar.gz"));
        assert_eq!(guess.app.as_deref(), Some("nvim"));
        assert_eq!(guess.version.as_deref(), Some("0.13.0-dev-906+g44baf03f2a"));
        assert_eq!(guess.os.as_deref(), Some("linux"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn detects_version_with_uppercase_v_prefix() {
        let guess = guess_from_filename(Path::new("App-V1.2.3-windows-x64.zip"));
        assert_eq!(guess.app.as_deref(), Some("App"));
        assert_eq!(guess.version.as_deref(), Some("1.2.3"));
        assert_eq!(guess.os.as_deref(), Some("windows"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parses_dot_separated_filename_with_no_dashes() {
        let guess = guess_from_filename(Path::new("myapp.2.5.0.windows.x64.zip"));
        assert_eq!(guess.app.as_deref(), Some("myapp"));
        assert_eq!(guess.version.as_deref(), Some("2.5.0"));
        assert_eq!(guess.os.as_deref(), Some("windows"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn does_not_shred_a_plain_dotted_name_without_any_signal() {
        // No version/OS/arch tokens anywhere, so the dots are assumed to be
        // part of the app name rather than field separators.
        let guess = guess_from_filename(Path::new("my.cool.app.zip"));
        assert_eq!(guess.app.as_deref(), Some("my.cool.app"));
        assert_eq!(guess.version, None);
        assert_eq!(guess.os, None);
        assert_eq!(guess.architecture, None);
    }

    #[test]
    fn recognizes_expanded_architecture_tokens() {
        for (filename, expected_arch) in [
            ("myapp-1.0.0-linux-riscv64.tar.gz", "riscv64"),
            ("myapp-1.0.0-linux-ppc64le.tar.gz", "ppc64le"),
            ("myapp-1.0.0-linux-s390x.tar.gz", "s390x"),
            ("myapp-1.4.0-macos-universal2.tar.gz", "universal2"),
            ("myapp-0.9.0-wasm32-unknown-unknown.tar.gz", "wasm32"),
            ("app-1.0-linux-i686.tar.gz", "x86"),
        ] {
            let guess = guess_from_filename(Path::new(filename));
            assert_eq!(
                guess.architecture.as_deref(),
                Some(expected_arch),
                "arch mismatch for {filename}"
            );
        }
    }

    #[test]
    fn recognizes_expanded_os_tokens() {
        for (filename, expected_os) in [
            ("myapp-2.0-freebsd-amd64.tar.gz", "freebsd"),
            ("myapp-3.2.1-android-arm64.zip", "android"),
            ("myapp-3.2.1-ios-arm64.zip", "ios"),
            (
                "ripgrep-13.0.0-x86_64-unknown-linux-musl.tar.gz",
                "linux",
            ),
        ] {
            let guess = guess_from_filename(Path::new(filename));
            assert_eq!(
                guess.os.as_deref(),
                Some(expected_os),
                "os mismatch for {filename}"
            );
        }
    }

    #[test]
    fn parses_rust_style_target_triples() {
        let msvc = guess_from_filename(Path::new(
            "ripgrep-13.0.0-x86_64-pc-windows-msvc.zip",
        ));
        assert_eq!(msvc.app.as_deref(), Some("ripgrep"));
        assert_eq!(msvc.version.as_deref(), Some("13.0.0"));
        assert_eq!(msvc.os.as_deref(), Some("windows"));
        assert_eq!(msvc.architecture.as_deref(), Some("x86_64"));

        let darwin = guess_from_filename(Path::new(
            "ripgrep-13.0.0-aarch64-apple-darwin.tar.gz",
        ));
        assert_eq!(darwin.app.as_deref(), Some("ripgrep"));
        assert_eq!(darwin.os.as_deref(), Some("macos"));
        assert_eq!(darwin.architecture.as_deref(), Some("aarch64"));
    }

    #[test]
    fn installer_words_do_not_pollute_the_version() {
        let guess = guess_from_filename(Path::new("app-1.2.3-setup.exe"));
        assert_eq!(guess.version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn handles_mixed_dot_and_underscore_separators() {
        let guess = guess_from_filename(Path::new("my_app.1.2.3.linux_x86_64.tar.gz"));
        assert_eq!(guess.app.as_deref(), Some("my-app"));
        assert_eq!(guess.version.as_deref(), Some("1.2.3"));
        assert_eq!(guess.os.as_deref(), Some("linux"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn handles_go_style_underscore_separators() {
        let guess = guess_from_filename(Path::new("myapp_1.2.3_darwin_amd64.tar.gz"));
        assert_eq!(guess.app.as_deref(), Some("myapp"));
        assert_eq!(guess.version.as_deref(), Some("1.2.3"));
        assert_eq!(guess.os.as_deref(), Some("macos"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn version_only_filename_has_no_app_name() {
        let guess = guess_from_filename(Path::new("1.29.0-linux-amd64.tar.gz"));
        assert_eq!(guess.app, None);
        assert_eq!(guess.version.as_deref(), Some("1.29.0"));
        assert_eq!(guess.os.as_deref(), Some("linux"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }

    #[test]
    fn handles_space_separated_filenames() {
        let guess = guess_from_filename(Path::new("My App 1.2.3 Linux x64.zip"));
        assert_eq!(guess.app.as_deref(), Some("My-App"));
        assert_eq!(guess.version.as_deref(), Some("1.2.3"));
        assert_eq!(guess.os.as_deref(), Some("linux"));
        assert_eq!(guess.architecture.as_deref(), Some("x86_64"));
    }
}