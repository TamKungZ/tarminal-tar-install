use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use dialoguer::Input;
use std::path::PathBuf;
use tar_install::archive::inspect_archive;
use tar_install::install::{doctor_app, install_archive, list_apps, make_plan, remove_app};
use tar_install::paths::InstallScope;
use tar_install::recipe::{load_recipe, InstallInput};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScopeArg {
    User,
    System,
}

impl From<ScopeArg> for InstallScope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::User => InstallScope::User,
            ScopeArg::System => InstallScope::System,
        }
    }
}

#[derive(Parser)]
#[command(name = "tarminal")]
#[command(about = "Install Linux app tarballs into proper desktop-app locations")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect a tarball and show guessed app metadata.
    Inspect {
        archive: PathBuf,
        /// Output JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Install a tarball.
    Install {
        archive: PathBuf,
        /// User install uses ~/.local; system install uses /opt and /usr/local/bin.
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
        /// Shortcut for --scope system.
        #[arg(long)]
        system: bool,
        /// External community recipe YAML.
        #[arg(long)]
        recipe: Option<PathBuf>,
        /// App id, e.g. com.example.myapp or myapp.
        #[arg(long)]
        id: Option<String>,
        /// Display name for menu.
        #[arg(long)]
        name: Option<String>,
        /// Version override.
        #[arg(long)]
        version: Option<String>,
        /// Executable path inside the app directory after root stripping.
        #[arg(long = "exec")]
        exec_path: Option<String>,
        /// Command name to create in ~/.local/bin or /usr/local/bin.
        #[arg(long)]
        command: Option<String>,
        /// Icon path inside the app directory after root stripping.
        #[arg(long)]
        icon: Option<String>,
        /// Ask for values interactively after heuristics.
        #[arg(long)]
        config: bool,
        /// Overwrite existing app directory.
        #[arg(long)]
        force: bool,
    },
    /// List apps installed by Tarminal.
    List {
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
    /// Remove an app installed by Tarminal.
    Remove {
        id: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
    /// Check installed files for an app.
    Doctor {
        id: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect { archive, json } => {
            let inspection = inspect_archive(&archive)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&inspection)?);
            } else {
                print_inspection(&inspection);
            }
        }
        Commands::Install {
            archive,
            scope,
            system,
            recipe,
            id,
            name,
            version,
            exec_path,
            command,
            icon,
            config,
            force,
        } => {
            let scope = if system { InstallScope::System } else { scope.into() };
            let loaded_recipe = match recipe {
                Some(path) => Some(load_recipe(&path)?),
                None => None,
            };
            let mut input = InstallInput {
                id,
                name,
                version,
                exec: exec_path,
                command,
                icon,
                recipe: loaded_recipe,
                force,
                interactive_config: config,
            };

            if config {
                input = interactive_config(&archive, scope, input)?;
            }

            let report = install_archive(&archive, scope, input)?;
            println!("Installed {}", report.installed.name);
            println!("  id:       {}", report.installed.id);
            println!("  command:  {}", report.installed.command_path.display());
            println!("  desktop:  {}", report.installed.desktop_path.display());
            println!("  app dir:  {}", report.installed.install_dir.display());
        }
        Commands::List { scope } => {
            let apps = list_apps(scope.into())?;
            if apps.is_empty() {
                println!("No apps installed in this scope.");
            } else {
                for app in apps {
                    let version = app.version.unwrap_or_else(|| "-".to_string());
                    println!("{}  {}  {}", app.id, version, app.command_name);
                }
            }
        }
        Commands::Remove { id, scope } => {
            let report = remove_app(scope.into(), &id)?;
            println!("Removed {}", report.id);
            for path in report.removed_paths {
                println!("  {}", path.display());
            }
        }
        Commands::Doctor { id, scope } => {
            for line in doctor_app(scope.into(), &id)? {
                println!("{}", line);
            }
        }
    }
    Ok(())
}

fn interactive_config(archive: &PathBuf, scope: InstallScope, mut input: InstallInput) -> Result<InstallInput> {
    // Build a non-final plan only to collect good defaults. If it fails, fallback to raw inspect output.
    let (default_id, default_name, default_version, default_exec, default_command, default_icon) = match make_plan(archive, scope, &input) {
        Ok((plan, _)) => (
            plan.app_id,
            plan.app_name,
            plan.version.unwrap_or_default(),
            plan.exec_path_inside_app.to_string_lossy().to_string(),
            plan.command_name,
            plan.icon_path_inside_app.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        ),
        Err(_) => {
            let inspection = inspect_archive(archive)?;
            let app = inspection.filename_guess.app.unwrap_or_else(|| "myapp".to_string());
            let exec = inspection.executable_candidates.first()
                .map(|c| c.path.to_string_lossy().to_string())
                .unwrap_or_default();
            let icon = inspection.icon_candidates.first()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            (app.clone(), app.clone(), inspection.filename_guess.version.unwrap_or_default(), exec, app, icon)
        }
    };

    let id: String = Input::new().with_prompt("App id").default(input.id.clone().unwrap_or(default_id)).interact_text()?;
    let name: String = Input::new().with_prompt("Menu name").default(input.name.clone().unwrap_or(default_name)).interact_text()?;
    let version: String = Input::new().with_prompt("Version (empty allowed)").default(input.version.clone().unwrap_or(default_version)).allow_empty(true).interact_text()?;
    let exec: String = Input::new().with_prompt("Executable path inside app").default(input.exec.clone().unwrap_or(default_exec)).interact_text()?;
    let command: String = Input::new().with_prompt("Command name").default(input.command.clone().unwrap_or(default_command)).interact_text()?;
    let icon: String = Input::new().with_prompt("Icon path inside app (empty allowed)").default(input.icon.clone().unwrap_or(default_icon)).allow_empty(true).interact_text()?;

    input.id = Some(id);
    input.name = Some(name);
    input.version = if version.trim().is_empty() { None } else { Some(version) };
    input.exec = Some(exec);
    input.command = Some(command);
    input.icon = if icon.trim().is_empty() { None } else { Some(icon) };
    Ok(input)
}

fn print_inspection(inspection: &tar_install::ArchiveInspection) {
    println!("Archive: {}", inspection.archive_path.display());
    println!("Safe: {}", if inspection.safe { "yes" } else { "no" });
    println!("Entries: {}", inspection.entries_count);
    println!("Common root: {}", inspection.common_root.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "-".to_string()));
    println!("Filename guess:");
    println!("  app:     {}", inspection.filename_guess.app.as_deref().unwrap_or("-"));
    println!("  version: {}", inspection.filename_guess.version.as_deref().unwrap_or("-"));
    println!("  os:      {}", inspection.filename_guess.os.as_deref().unwrap_or("-"));
    println!("  arch:    {}", inspection.filename_guess.architecture.as_deref().unwrap_or("-"));
    println!("  confidence: {:.2}", inspection.filename_guess.confidence);

    println!("Executable candidates:");
    for candidate in inspection.executable_candidates.iter().take(8) {
        println!("  [{}] {} ({})", candidate.score, candidate.path.display(), candidate.reason);
    }
    if inspection.executable_candidates.is_empty() {
        println!("  -");
    }

    println!("Icon candidates:");
    for icon in inspection.icon_candidates.iter().take(8) {
        println!("  {}", icon.display());
    }
    if inspection.icon_candidates.is_empty() {
        println!("  -");
    }

    if !inspection.manifest_candidates.is_empty() {
        println!("Manifest candidates:");
        for path in &inspection.manifest_candidates {
            println!("  {}", path.display());
        }
    }

    if !inspection.notes.is_empty() {
        println!("Notes:");
        for note in &inspection.notes {
            println!("  - {}", note);
        }
    }
}
