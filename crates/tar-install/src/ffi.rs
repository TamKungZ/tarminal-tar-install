use serde::Deserialize;
use serde_json::json;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use crate::paths::InstallScope;
use crate::recipe::{load_recipe, InstallInput};

type ApiResult = Result<serde_json::Value, String>;

#[derive(Debug, Deserialize)]
struct InstallArgs {
    archive: String,
    scope: Option<String>,
    app_id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    exec_path: Option<String>,
    command: Option<String>,
    icon: Option<String>,
    recipe: Option<String>,
    force: Option<bool>,
    probe_version: Option<bool>,
}

#[no_mangle]
pub extern "C" fn tar_install_inspect(archive: *const c_char) -> *mut c_char {
    respond(|| {
        let archive = read_string(archive)?;
        let inspection = crate::archive::inspect_archive(Path::new(&archive))
            .map_err(|err| format!("{err:#}"))?;
        serde_json::to_value(inspection).map_err(|err| err.to_string())
    })
}

#[no_mangle]
pub extern "C" fn tar_install_install(config_json: *const c_char) -> *mut c_char {
    respond(|| {
        let config_json = read_string(config_json)?;
        let args: InstallArgs = serde_json::from_str(&config_json).map_err(|err| err.to_string())?;
        let scope = parse_scope(args.scope.as_deref())?;
        let recipe = match args.recipe {
            Some(path) => Some(load_recipe(Path::new(&path)).map_err(|err| format!("{err:#}"))?),
            None => None,
        };
        let input = InstallInput {
            id: args.app_id,
            name: args.name,
            version: args.version,
            probe_version: args.probe_version,
            exec: args.exec_path,
            command: args.command,
            icon: args.icon,
            recipe,
            force: args.force.unwrap_or(false),
            interactive_config: false,
        };
        let report = crate::install_archive(Path::new(&args.archive), scope, input)
            .map_err(|err| format!("{err:#}"))?;
        Ok(json!({
            "installed": report.installed,
            "plan": {
                "archive": report.plan.archive,
                "scope": report.plan.scope,
                "app_id": report.plan.app_id,
                "app_name": report.plan.app_name,
                "version": report.plan.version,
                "probe_version": report.plan.probe_version,
                "exec_path_inside_app": report.plan.exec_path_inside_app,
                "command_name": report.plan.command_name,
                "icon_path_inside_app": report.plan.icon_path_inside_app,
                "targets": report.plan.targets,
                "categories": report.plan.categories,
                "terminal": report.plan.terminal,
                "notes": report.plan.notes,
            }
        }))
    })
}

#[no_mangle]
pub extern "C" fn tar_install_list(scope: *const c_char) -> *mut c_char {
    respond(|| {
        let scope = read_optional_string(scope)?;
        let apps = crate::install::list_apps(parse_scope(scope.as_deref())?)
            .map_err(|err| format!("{err:#}"))?;
        serde_json::to_value(apps).map_err(|err| err.to_string())
    })
}

#[no_mangle]
pub extern "C" fn tar_install_remove(app_id: *const c_char, scope: *const c_char) -> *mut c_char {
    respond(|| {
        let app_id = read_string(app_id)?;
        let scope = read_optional_string(scope)?;
        let report = crate::remove_app(parse_scope(scope.as_deref())?, &app_id)
            .map_err(|err| format!("{err:#}"))?;
        Ok(json!({
            "id": report.id,
            "removed_paths": report.removed_paths,
        }))
    })
}

#[no_mangle]
pub extern "C" fn tar_install_doctor(app_id: *const c_char, scope: *const c_char) -> *mut c_char {
    respond(|| {
        let app_id = read_string(app_id)?;
        let scope = read_optional_string(scope)?;
        let lines = crate::install::doctor_app(parse_scope(scope.as_deref())?, &app_id)
            .map_err(|err| format!("{err:#}"))?;
        serde_json::to_value(lines).map_err(|err| err.to_string())
    })
}

#[no_mangle]
pub extern "C" fn tar_install_free_string(value: *mut c_char) {
    if !value.is_null() {
        unsafe {
            let _ = CString::from_raw(value);
        }
    }
}

fn respond(action: impl FnOnce() -> ApiResult) -> *mut c_char {
    let payload = match action() {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    into_c_string(payload.to_string())
}

fn read_string(value: *const c_char) -> Result<String, String> {
    if value.is_null() {
        return Err("received null string pointer".to_string());
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map(|value| value.to_string())
        .map_err(|err| err.to_string())
}

fn read_optional_string(value: *const c_char) -> Result<Option<String>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        read_string(value).map(Some)
    }
}

fn parse_scope(value: Option<&str>) -> Result<InstallScope, String> {
    match value.unwrap_or("user") {
        "user" => Ok(InstallScope::User),
        "system" => Ok(InstallScope::System),
        other => Err(format!("scope must be 'user' or 'system', got {other:?}")),
    }
}

fn into_c_string(value: String) -> *mut c_char {
    let sanitized = value.replace('\0', "\\u0000");
    CString::new(sanitized)
        .expect("sanitized JSON must not contain nul bytes")
        .into_raw()
}
