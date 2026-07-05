use crate::archive::{inspect_archive, open_tar_reader, read_text_entry, unsafe_path_reason, ArchiveInspection};
use crate::desktop::{write_desktop_entry, DesktopEntryInput};
use crate::filename::{normalize_arch, normalize_os};
use crate::paths::{self, InstallScope, InstallTargets};
use crate::recipe::{display_name_from_id, sanitize_command, sanitize_id, AppRecipe, InstallInput};
use crate::state::{load_state, save_state, InstalledApp};
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::{self as unix_fs, PermissionsExt};
use std::path::{Path, PathBuf};
use tar::Archive;
use tempfile::tempdir;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub archive: PathBuf,
    pub scope: InstallScope,
    pub app_id: String,
    pub app_name: String,
    pub version: Option<String>,
    pub probe_version: bool,
    pub exec_path_inside_app: PathBuf,
    pub command_name: String,
    pub icon_path_inside_app: Option<PathBuf>,
    pub targets: InstallTargets,
    pub categories: Vec<String>,
    pub terminal: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InstallReport {
    pub plan: InstallPlan,
    pub installed: InstalledApp,
}

#[derive(Debug, Clone)]
pub struct RemoveReport {
    pub id: String,
    pub removed_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum InstallProgress {
    Planning,
    Extracting { current: u64, total: u64, path: PathBuf },
    Copying { current: u64, total: u64, path: PathBuf },
    Integrating { step: &'static str },
    Finished,
}

pub fn make_plan(archive_path: &Path, scope: InstallScope, input: &InstallInput) -> Result<(InstallPlan, ArchiveInspection)> {
    let inspection = inspect_archive(archive_path)?;
    if !inspection.safe {
        bail!("archive contains unsafe paths; run `tarminal inspect` for details");
    }
    if !input.force {
        if let Some(reason) = incompatible_platform_reason(&inspection.filename_guess) {
            bail!("{} (use --force to override)", reason);
        }
    }

    let embedded_recipe = if input.recipe.is_none() {
        inspection.manifest_candidates.first()
            .and_then(|p| read_text_entry(archive_path, p, 64 * 1024).ok())
            .and_then(|text| serde_yaml::from_str::<AppRecipe>(&text).ok())
    } else {
        None
    };
    let recipe = input.recipe.as_ref().or(embedded_recipe.as_ref());

    let guessed_app = inspection.filename_guess.app.clone();
    let app_id = input.id.clone()
        .or_else(|| recipe.and_then(|r| r.id.clone()))
        .or_else(|| guessed_app.clone())
        .map(|s| sanitize_id(&s))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("cannot determine app id; use --id or --config"))?;

    let app_name = input.name.clone()
        .or_else(|| recipe.and_then(|r| r.name.clone()))
        .unwrap_or_else(|| display_name_from_id(&app_id));

    let version = input.version.clone()
        .or_else(|| recipe.and_then(|r| r.version.clone()))
        .or_else(|| inspection.filename_guess.version.clone());

    let probe_version = input.probe_version
        .or_else(|| recipe.and_then(|r| r.probe_version))
        .unwrap_or(true);

    let exec_from_input = input.exec.clone().or_else(|| recipe.and_then(|r| r.exec.clone()));
    let exec_path_inside_app = if let Some(exec) = exec_from_input {
        PathBuf::from(exec)
    } else {
        inspection.executable_candidates.first()
            .map(|c| strip_common_root(&c.path, inspection.common_root.as_deref()))
            .ok_or_else(|| anyhow!("cannot determine executable; use --exec or --config"))?
    };

    let command_name = input.command.clone()
        .or_else(|| recipe.and_then(|r| r.command.clone()))
        .unwrap_or_else(|| sanitize_command(&app_id));

    let icon_path_inside_app = input.icon.clone()
        .or_else(|| recipe.and_then(|r| r.icon.clone()))
        .map(PathBuf::from)
        .or_else(|| inspection.icon_candidates.first().map(|p| strip_common_root(p, inspection.common_root.as_deref())));

    let targets = paths::targets(scope, &app_id, &command_name)?;
    let categories = recipe.and_then(|r| r.desktop.as_ref()).and_then(|d| d.categories.clone()).unwrap_or_else(|| vec!["Utility".to_string()]);
    let terminal = recipe.and_then(|r| r.desktop.as_ref()).and_then(|d| d.terminal).unwrap_or(false);

    Ok((InstallPlan {
        archive: archive_path.to_path_buf(),
        scope,
        app_id,
        app_name,
        version,
        probe_version,
        exec_path_inside_app,
        command_name,
        icon_path_inside_app,
        targets,
        categories,
        terminal,
        notes: inspection.notes.clone(),
    }, inspection))
}

pub fn install_archive(archive_path: &Path, scope: InstallScope, input: InstallInput) -> Result<InstallReport> {
    install_archive_with_progress(archive_path, scope, input, None)
}

pub fn install_archive_with_progress(
    archive_path: &Path,
    scope: InstallScope,
    input: InstallInput,
    progress: Option<&dyn Fn(InstallProgress)>,
) -> Result<InstallReport> {
    emit_progress(progress, InstallProgress::Planning);

    let (mut plan, inspection) = make_plan(archive_path, scope, &input)?;

    if plan.targets.app_dir.exists() && !input.force {
        bail!("install directory already exists: {} (use --force to overwrite)", plan.targets.app_dir.display());
    }

    let staging = tempdir().context("failed to create temporary extraction directory")?;
    safe_extract_to(archive_path, staging.path(), progress, inspection.entries_count as u64)?;

    let source_root = if let Some(root) = inspection.common_root.as_ref() {
        let candidate = staging.path().join(root);
        if candidate.exists() { candidate } else { staging.path().to_path_buf() }
    } else {
        staging.path().to_path_buf()
    };

    if plan.targets.app_dir.exists() {
        emit_progress(progress, InstallProgress::Integrating { step: "removing previous install" });
        fs::remove_dir_all(&plan.targets.app_dir)
            .with_context(|| format!("failed to remove existing install dir: {}", plan.targets.app_dir.display()))?;
    }
    if let Some(parent) = plan.targets.app_dir.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create app parent dir: {}", parent.display()))?;
    }
    copy_dir_all(&source_root, &plan.targets.app_dir, progress)?;

    let exec_abs = plan.targets.app_dir.join(&plan.exec_path_inside_app);
    if !exec_abs.exists() {
        bail!("resolved executable does not exist after extraction: {}", exec_abs.display());
    }
    ensure_executable(&exec_abs)?;

    if plan.version.is_none() && plan.probe_version {
        emit_progress(progress, InstallProgress::Integrating { step: "detecting version" });

        match crate::version::detect_installed_version(
            &plan.targets.app_dir,
            &plan.exec_path_inside_app,
            &plan.command_name,
            &plan.app_id,
            &plan.app_name,
        ) {
            Ok(Some(found)) => {
                plan.notes.push(format!("version detected from {}", found.source));
                plan.version = Some(found.version);
            }
            Ok(None) => {
                plan.notes.push("version could not be detected from metadata or command probes".to_string());
            }
            Err(err) => {
                plan.notes.push(format!("version probe failed: {err:#}"));
            }
        }
    }

    emit_progress(progress, InstallProgress::Integrating { step: "writing command wrapper" });
    write_wrapper(&plan.targets.command_path, &plan.targets.app_dir, &exec_abs)?;

    emit_progress(progress, InstallProgress::Integrating { step: "installing icon" });
    let icon_paths = if let Some(icon_inside) = plan.icon_path_inside_app.as_ref() {
        install_icon(&plan, icon_inside).unwrap_or_default()
    } else {
        Vec::new()
    };

    emit_progress(progress, InstallProgress::Integrating { step: "writing desktop entry" });
    write_desktop_entry(&plan.targets.desktop_path, &DesktopEntryInput {
        name: &plan.app_name,
        generic_name: None,
        comment: Some("Installed from a Linux tarball by Tarminal"),
        exec_path: &plan.targets.command_path,
        icon_name: &plan.app_id,
        categories: &plan.categories,
        terminal: plan.terminal,
    })?;

    emit_progress(progress, InstallProgress::Integrating { step: "saving state" });
    let sha256 = sha256_file(archive_path).ok();
    let installed = InstalledApp {
        id: plan.app_id.clone(),
        name: plan.app_name.clone(),
        version: plan.version.clone(),
        scope: plan.scope,
        install_dir: plan.targets.app_dir.clone(),
        command_name: plan.command_name.clone(),
        command_path: plan.targets.command_path.clone(),
        desktop_path: plan.targets.desktop_path.clone(),
        icon_paths,
        source_archive: Some(archive_path.to_path_buf()),
        source_sha256: sha256,
    };

    let mut db = load_state(&plan.targets.state_path)?;
    db.apps.insert(installed.id.clone(), installed.clone());
    save_state(&plan.targets.state_path, &db)?;

    emit_progress(progress, InstallProgress::Finished);
    Ok(InstallReport { plan, installed })
}

pub fn remove_app(scope: InstallScope, app_id: &str) -> Result<RemoveReport> {
    let id = sanitize_id(app_id);
    let dummy = paths::targets(scope, &id, &id)?;
    let mut db = load_state(&dummy.state_path)?;
    let app = db.apps.remove(&id).ok_or_else(|| anyhow!("app is not installed in {:?} scope: {}", scope, id))?;

    let mut removed = Vec::new();
    remove_path(&app.install_dir, &mut removed)?;
    remove_path(&app.command_path, &mut removed)?;
    remove_path(&app.desktop_path, &mut removed)?;
    for icon in &app.icon_paths {
        remove_path(icon, &mut removed)?;
    }
    save_state(&dummy.state_path, &db)?;
    Ok(RemoveReport { id, removed_paths: removed })
}

pub fn doctor_app(scope: InstallScope, app_id: &str) -> Result<Vec<String>> {
    let id = sanitize_id(app_id);
    let dummy = paths::targets(scope, &id, &id)?;
    let db = load_state(&dummy.state_path)?;
    let app = db.apps.get(&id).ok_or_else(|| anyhow!("app is not installed in {:?} scope: {}", scope, id))?;
    let mut lines = Vec::new();
    lines.push(format!("id: {}", app.id));
    lines.push(format!("name: {}", app.name));
    lines.push(format!("install dir: {} [{}]", app.install_dir.display(), exists_text(&app.install_dir)));
    lines.push(format!("command: {} [{}]", app.command_path.display(), exists_text(&app.command_path)));
    lines.push(format!("desktop: {} [{}]", app.desktop_path.display(), exists_text(&app.desktop_path)));
    for icon in &app.icon_paths {
        lines.push(format!("icon: {} [{}]", icon.display(), exists_text(icon)));
    }
    Ok(lines)
}

pub fn list_apps(scope: InstallScope) -> Result<Vec<InstalledApp>> {
    let dummy = paths::targets(scope, "dummy", "dummy")?;
    let db = load_state(&dummy.state_path)?;
    Ok(db.apps.values().cloned().collect())
}

fn emit_progress(progress: Option<&dyn Fn(InstallProgress)>, event: InstallProgress) {
    if let Some(callback) = progress {
        callback(event);
    }
}

fn incompatible_platform_reason(guess: &crate::filename::FilenameGuess) -> Option<String> {
    if let Some(archive_os) = guess.os.as_deref() {
        let current_os = normalize_os(std::env::consts::OS)
            .unwrap_or_else(|| std::env::consts::OS.to_string());

        if archive_os != current_os {
            return Some(format!(
                "archive appears to be for {}, but this system is {}",
                archive_os, current_os
            ));
        }
    }

    if let Some(archive_arch) = guess.architecture.as_deref() {
        let current_arch = normalize_arch(std::env::consts::ARCH);

        if archive_arch != current_arch {
            return Some(format!(
                "archive appears to be for {} architecture, but this system is {}",
                archive_arch, current_arch
            ));
        }
    }

    None
}

fn strip_common_root(path: &Path, root: Option<&Path>) -> PathBuf {
    if let Some(root) = root {
        path.strip_prefix(root).unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    }
}

fn safe_extract_to(
    archive_path: &Path,
    dest: &Path,
    progress: Option<&dyn Fn(InstallProgress)>,
    total_entries: u64,
) -> Result<()> {
    let reader = open_tar_reader(archive_path)?;
    let mut archive = Archive::new(reader);
    let mut current = 0_u64;

    for entry in archive.entries().context("failed to read tar entries")? {
        let mut entry = entry.context("failed to read tar entry")?;
        let entry_type = entry.header().entry_type();
        let raw_path = entry.path().context("failed to read tar entry path")?.to_path_buf();
        current += 1;

        emit_progress(progress, InstallProgress::Extracting {
            current,
            total: total_entries,
            path: raw_path.clone(),
        });

        if let Some(reason) = unsafe_path_reason(&raw_path) {
            bail!("unsafe path in archive: {} ({})", raw_path.display(), reason);
        }

        let out_path = dest.join(&raw_path);

        if entry_type.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else if entry_type.is_file() {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&out_path)
                .with_context(|| format!("failed to create extracted file: {}", out_path.display()))?;
            std::io::copy(&mut entry, &mut out)?;
            let mode = entry.header().mode().unwrap_or(0o644);
            fs::set_permissions(&out_path, fs::Permissions::from_mode(mode & 0o777))?;
        } else if entry_type.is_symlink() {
            let link_target = entry
                .link_name()
                .context("failed to read symlink target")?
                .ok_or_else(|| anyhow!("symlink entry has no target: {}", raw_path.display()))?
                .into_owned();

            validate_safe_symlink(&raw_path, &link_target)?;

            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            if fs::symlink_metadata(&out_path).is_ok() {
                bail!("refusing to overwrite existing path with symlink: {}", raw_path.display());
            }

            unix_fs::symlink(&link_target, &out_path)
                .with_context(|| format!("failed to create symlink: {} -> {}", raw_path.display(), link_target.display()))?;
        } else if entry_type.is_hard_link() {
            bail!("hard link entries are not supported yet: {}", raw_path.display());
        }
    }

    Ok(())
}

fn validate_safe_symlink(link_path: &Path, target: &Path) -> Result<()> {
    if target.is_absolute() {
        bail!(
            "unsafe symlink target in archive: {} -> {} (absolute target)",
            link_path.display(),
            target.display()
        );
    }

    let base = link_path.parent().unwrap_or_else(|| Path::new(""));
    let resolved = normalize_relative_path(&base.join(target)).ok_or_else(|| {
        anyhow!(
            "unsafe symlink target in archive: {} -> {} (escapes archive root)",
            link_path.display(),
            target.display()
        )
    })?;

    if resolved.as_os_str().is_empty() {
        bail!(
            "unsafe symlink target in archive: {} -> {} (empty target)",
            link_path.display(),
            target.display()
        );
    }

    Ok(())
}

fn normalize_relative_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return None,
        }
    }

    Some(normalized)
}

fn copy_dir_all(src: &Path, dst: &Path, progress: Option<&dyn Fn(InstallProgress)>) -> Result<()> {
    fs::create_dir_all(dst)?;

    let mut entries = Vec::new();
    for entry in WalkDir::new(src).follow_links(false) {
        entries.push(entry?);
    }

    let total = entries.iter().filter(|entry| entry.path() != src).count() as u64;
    let mut current = 0_u64;

    for entry in entries {
        let rel = entry.path().strip_prefix(src)?;
        if rel.as_os_str().is_empty() {
            continue;
        }

        current += 1;
        emit_progress(progress, InstallProgress::Copying {
            current,
            total,
            path: rel.to_path_buf(),
        });

        let to = dst.join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&to)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &to)
                .with_context(|| format!("failed to copy {} to {}", entry.path().display(), to.display()))?;
            let perms = fs::metadata(entry.path())?.permissions();
            fs::set_permissions(&to, perms)?;
        } else if entry.file_type().is_symlink() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }

            let target = fs::read_link(entry.path())
                .with_context(|| format!("failed to read symlink: {}", entry.path().display()))?;

            if fs::symlink_metadata(&to).is_ok() {
                fs::remove_file(&to)
                    .with_context(|| format!("failed to replace existing symlink target: {}", to.display()))?;
            }

            unix_fs::symlink(&target, &to)
                .with_context(|| format!("failed to copy symlink {} -> {}", to.display(), target.display()))?;
        }
    }

    Ok(())
}

fn ensure_executable(path: &Path) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    let mode = perms.mode();
    if (mode & 0o111) == 0 {
        perms.set_mode(mode | 0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn write_wrapper(path: &Path, app_dir: &Path, exec_abs: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create command dir: {}", parent.display()))?;
    }
    let content = format!(
        "#!/usr/bin/env bash\nset -e\nAPPDIR={}\ncd \"$APPDIR\"\nexec {} \"$@\"\n",
        shell_quote(&app_dir.to_string_lossy()),
        shell_quote(&exec_abs.to_string_lossy()),
    );
    fs::write(path, content).with_context(|| format!("failed to write wrapper: {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

fn install_icon(plan: &InstallPlan, icon_inside: &Path) -> Result<Vec<PathBuf>> {
    let src = plan.targets.app_dir.join(icon_inside);
    if !src.exists() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(&plan.targets.icon_dir)?;
    let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("png");
    let dest = plan.targets.icon_dir.join(format!("{}.{}", plan.app_id, ext));
    fs::copy(&src, &dest).with_context(|| format!("failed to install icon: {}", dest.display()))?;
    Ok(vec![dest])
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn remove_path(path: &Path, removed: &mut Vec<PathBuf>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    removed.push(path.to_path_buf());
    Ok(())
}

fn exists_text(path: &Path) -> &'static str {
    if path.exists() { "ok" } else { "missing" }
}