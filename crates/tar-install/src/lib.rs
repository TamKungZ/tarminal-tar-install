//! tar-install
//!
//! Library core for installing Linux app tarballs into a desktop-friendly shape:
//! app directory, command wrapper, `.desktop` entry, icon, state DB, and safe removal.

pub mod archive;
pub mod desktop;
pub mod ffi;
pub mod filename;
pub mod install;
pub mod paths;
pub mod recipe;
pub mod state;
pub mod version;

pub use archive::{ArchiveEntry, ArchiveInspection, ExecutableCandidate};
pub use filename::FilenameGuess;
pub use install::{
    doctor_app, install_archive, install_archive_with_progress, remove_app, InstallPlan, InstallProgress,
    InstallReport, RemoveReport,
};
pub use paths::{InstallScope, InstallTargets};
pub use recipe::{AppRecipe, InstallInput, RecipeDesktop};
pub use version::{detect_installed_version, parse_version_from_text, VersionProbeResult};
