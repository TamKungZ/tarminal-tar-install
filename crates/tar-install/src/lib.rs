//! tar-install
//!
//! Library core for installing Linux app tarballs into a desktop-friendly shape:
//! app directory, command wrapper, `.desktop` entry, icon, state DB, and safe removal.

pub mod archive;
pub mod desktop;
pub mod filename;
pub mod install;
pub mod paths;
pub mod recipe;
pub mod state;

pub use archive::{ArchiveEntry, ArchiveInspection, ExecutableCandidate};
pub use filename::FilenameGuess;
pub use install::{install_archive, remove_app, doctor_app, InstallPlan, InstallReport, RemoveReport};
pub use paths::{InstallScope, InstallTargets};
pub use recipe::{AppRecipe, InstallInput, RecipeDesktop};
