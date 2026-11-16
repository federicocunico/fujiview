//! Persistent app state: the user preferences file and one automatic project
//! file per opened folder, both under the app data directory so the photo
//! folders are never written to.

use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

/// Writes JSON next to the target and swaps it in, keeping a `.bak` copy of
/// the previous version so a crash mid-write never loses the file.
pub fn write_json_atomic(target: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = target.parent().ok_or("Percorso non valido")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let file_name = target
        .file_name()
        .ok_or("Percorso non valido")?
        .to_string_lossy()
        .into_owned();
    let temp = parent.join(format!("{file_name}.tmp"));
    let backup = parent.join(format!("{file_name}.bak"));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if !target.exists() {
        return fs::rename(temp, target).map_err(|e| e.to_string());
    }
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    fs::rename(target, &backup).map_err(|e| e.to_string())?;
    match fs::rename(&temp, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::rename(&backup, target);
            Err(error.to_string())
        }
    }
}

/// Reads a JSON file, falling back to its `.bak` copy when the main file is
/// missing or corrupt. `None` when neither exists.
fn read_json(target: &Path) -> Result<Option<serde_json::Value>, String> {
    let backup = target.with_file_name(format!(
        "{}.bak",
        target.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut last_error = None;
    for candidate in [target, backup.as_path()] {
        match fs::read(candidate) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(value) => return Ok(Some(value)),
                Err(error) => last_error = Some(format!("File non valido: {error}")),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    last_error.map_or(Ok(None), Err)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("settings.json"))
}

/// Stable FNV-1a hash (std's hasher is not guaranteed stable across releases).
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// File name of the automatic project for a folder: readable folder name plus
/// a hash of the normalized path, so "D:\Foto\x\" and "d:/foto/x" match.
pub fn session_file_name(root: &str) -> String {
    let normalized = root
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    let name: String = Path::new(&normalized)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    format!(
        "{}-{:016x}.fujiview",
        if name.is_empty() { "root" } else { &name },
        fnv1a(&normalized)
    )
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<serde_json::Value, String> {
    // A corrupt settings file must never block the app: start from defaults.
    Ok(read_json(&settings_path(&app)?)
        .ok()
        .flatten()
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({})))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("Preferenze non valide".into());
    }
    write_json_atomic(&settings_path(&app)?, &settings)
}

/// Path of the automatic project file for a folder (it may not exist yet).
#[tauri::command]
pub fn session_project_path(app: AppHandle, root: String) -> Result<String, String> {
    Ok(data_dir(&app)?
        .join("projects")
        .join(session_file_name(&root))
        .to_string_lossy()
        .into_owned())
}

/// Loads a project file, using its backup if the main copy is damaged.
/// `None` when the file does not exist.
pub fn load_project_file(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let Some(value) = read_json(path)? else {
        return Ok(None);
    };
    if value.get("schemaVersion").and_then(|item| item.as_u64()) != Some(1) {
        return Err("Versione progetto non supportata".into());
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_file_name_ignores_case_slashes_and_trailing_separator() {
        let a = session_file_name("D:\\Foto\\2026 Matrimonio\\RAW\\FujiView varianti");
        let b = session_file_name("d:/foto/2026 matrimonio/raw/fujiview varianti/");
        assert_eq!(a, b);
        assert!(a.starts_with("fujiview_varianti-"));
        assert!(a.ends_with(".fujiview"));
        assert_ne!(a, session_file_name("D:\\Foto\\altro\\FujiView varianti"));
    }

    #[test]
    fn atomic_write_keeps_backup_and_read_falls_back_to_it() {
        let dir = std::env::temp_dir().join(format!("fujiview-persist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let target = dir.join("p.fujiview");
        write_json_atomic(&target, &serde_json::json!({"schemaVersion": 1, "n": 1})).unwrap();
        write_json_atomic(&target, &serde_json::json!({"schemaVersion": 1, "n": 2})).unwrap();
        assert_eq!(load_project_file(&target).unwrap().unwrap()["n"], 2);
        fs::write(&target, b"{broken").unwrap();
        assert_eq!(load_project_file(&target).unwrap().unwrap()["n"], 1);
        assert!(load_project_file(&dir.join("missing.fujiview"))
            .unwrap()
            .is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
