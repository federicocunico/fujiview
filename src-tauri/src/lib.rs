use fast_image_resize as fir;
use image::{codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView, ImageEncoder};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use tokio::sync::Semaphore;
use xmp_toolkit::{xmp_ns, OpenFileOptions, XmpFile, XmpMeta, XmpValue};

const CACHE_LIMIT_BYTES: i64 = 20 * 1024 * 1024 * 1024;
const CACHE_VERSION: &str = "preview-v1";

struct AppState {
    decode_slots: Semaphore,
    cache_db: Mutex<Option<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Variant {
    preset: String,
    path: String,
    file_name: String,
    extension: String,
    bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexedShot {
    key: String,
    display_name: String,
    variants: Vec<Variant>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryIndex {
    root: String,
    presets: Vec<String>,
    guide_preset: String,
    shots: Vec<IndexedShot>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResult {
    cache_path: String,
    width: u32,
    height: u32,
    cached: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportItem {
    source_path: String,
    output_name: String,
    rating: i32,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExportReport {
    exported: usize,
    skipped: usize,
    errors: Vec<String>,
}

fn image_extension(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    matches!(ext.as_str(), "jpg" | "jpeg" | "tif" | "tiff").then_some(ext)
}

fn scan_library_impl(root: &Path) -> Result<LibraryIndex, String> {
    if !root.is_dir() {
        return Err("La cartella sorgente non esiste o non è accessibile".into());
    }
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let mut preset_dirs = Vec::new();
    for entry in fs::read_dir(&canonical).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            preset_dirs.push((
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            ));
        }
    }
    preset_dirs.sort_by(|a, b| natord::compare_ignore_case(&a.0, &b.0));
    if preset_dirs.is_empty() {
        return Err("Non sono state trovate sottocartelle preset".into());
    }

    let mut grouped: BTreeMap<String, (String, Vec<Variant>)> = BTreeMap::new();
    let mut preset_keys: BTreeMap<String, BTreeSet<String>> = preset_dirs
        .iter()
        .map(|(preset, _)| (preset.clone(), BTreeSet::new()))
        .collect();
    let mut warnings = Vec::new();
    for (preset, directory) in &preset_dirs {
        let entries = match fs::read_dir(directory) {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("{}: {}", directory.display(), error));
                continue;
            }
        };
        for item in entries {
            let entry = match item {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(extension) = image_extension(&path) else {
                continue;
            };
            let Some(stem) = path
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
            else {
                continue;
            };
            let key = stem.to_lowercase();
            let bytes = entry.metadata().map(|value| value.len()).unwrap_or(0);
            let variant = Variant {
                preset: preset.clone(),
                path: path.to_string_lossy().into_owned(),
                file_name: entry.file_name().to_string_lossy().into_owned(),
                extension,
                bytes,
            };
            let group = grouped
                .entry(key.clone())
                .or_insert_with(|| (stem, Vec::new()));
            if group.1.iter().any(|existing| existing.preset == *preset) {
                warnings.push(format!(
                    "Duplicato ignorato in {}: {}",
                    preset, variant.file_name
                ));
            } else {
                group.1.push(variant);
                if let Some(keys) = preset_keys.get_mut(preset) {
                    keys.insert(key);
                }
            }
        }
    }

    // The largest preset is the guide: only its filenames define the catalog.
    // Because preset_dirs is naturally sorted and we only replace on a strict
    // increase, ties are deterministic.
    let mut guide_preset = preset_dirs[0].0.clone();
    let mut guide_count = preset_keys.get(&guide_preset).map_or(0, BTreeSet::len);
    for (preset, _) in preset_dirs.iter().skip(1) {
        let count = preset_keys.get(preset).map_or(0, BTreeSet::len);
        if count > guide_count {
            guide_preset = preset.clone();
            guide_count = count;
        }
    }
    let guide_keys = preset_keys.get(&guide_preset).cloned().unwrap_or_default();
    let mut shots: Vec<_> = grouped
        .into_iter()
        .filter(|(key, _)| guide_keys.contains(key))
        .map(|(key, (fallback_name, mut variants))| {
            variants.sort_by(|a, b| natord::compare_ignore_case(&a.preset, &b.preset));
            let display_name = variants
                .iter()
                .find(|variant| variant.preset == guide_preset)
                .and_then(|variant| Path::new(&variant.path).file_stem())
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or(fallback_name);
            IndexedShot {
                key,
                display_name,
                variants,
            }
        })
        .collect();
    shots.sort_by(|a, b| natord::compare_ignore_case(&a.display_name, &b.display_name));
    Ok(LibraryIndex {
        root: canonical.to_string_lossy().into_owned(),
        presets: preset_dirs.into_iter().map(|item| item.0).collect(),
        guide_preset,
        shots,
        warnings,
    })
}

#[tauri::command]
async fn scan_library(root: String) -> Result<LibraryIndex, String> {
    tauri::async_runtime::spawn_blocking(move || scan_library_impl(Path::new(&root)))
        .await
        .map_err(|e| e.to_string())?
}

fn exif_orientation(path: &Path) -> u32 {
    let Ok(file) = fs::File::open(path) else {
        return 1;
    };
    let mut reader = BufReader::new(file);
    exif::Reader::new()
        .read_from_container(&mut reader)
        .ok()
        .and_then(|data| {
            data.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })
        .unwrap_or(1)
}

fn orient(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

fn preview_key(path: &Path, max_edge: u32) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let modified = metadata
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(blake3::hash(
        format!(
            "{}|{}|{}|{}|{}",
            CACHE_VERSION,
            path.to_string_lossy(),
            metadata.len(),
            modified,
            max_edge
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string())
}

fn build_preview(source: &Path, destination: &Path, max_edge: u32) -> Result<(u32, u32), String> {
    let decoded = image::ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let decoded = orient(decoded, exif_orientation(source));
    let (width, height) = decoded.dimensions();
    let ratio = (max_edge as f64 / width.max(height) as f64).min(1.0);
    let out_width = ((width as f64 * ratio).round() as u32).max(1);
    let out_height = ((height as f64 * ratio).round() as u32).max(1);
    let rgb = decoded.into_rgb8();
    let src = fir::images::Image::from_vec_u8(width, height, rgb.into_raw(), fir::PixelType::U8x3)
        .map_err(|e| e.to_string())?;
    let mut dst = fir::images::Image::new(out_width, out_height, fir::PixelType::U8x3);
    let mut resizer = fir::Resizer::new();
    resizer
        .resize(
            &src,
            &mut dst,
            Some(
                &fir::ResizeOptions::new()
                    .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Lanczos3)),
            ),
        )
        .map_err(|e| e.to_string())?;
    let temp = destination.with_extension("tmp");
    let file = fs::File::create(&temp).map_err(|e| e.to_string())?;
    JpegEncoder::new_with_quality(BufWriter::new(file), 90)
        .write_image(
            dst.buffer(),
            out_width,
            out_height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    fs::rename(&temp, destination).map_err(|e| e.to_string())?;
    Ok((out_width, out_height))
}

fn touch_cache(state: &AppState, key: &str, path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let Ok(mut guard) = state.cache_db.lock() else {
        return;
    };
    let Some(db) = guard.as_mut() else {
        return;
    };
    let _ = db.execute("INSERT INTO cache_entries(key,path,bytes,last_access) VALUES(?1,?2,?3,?4) ON CONFLICT(key) DO UPDATE SET last_access=?4,bytes=?3,path=?2", params![key, path.to_string_lossy(), metadata.len() as i64, now]);
    let total: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(bytes),0) FROM cache_entries",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if total <= CACHE_LIMIT_BYTES {
        return;
    }
    let mut reclaimed = 0i64;
    let mut stale = Vec::new();
    if let Ok(mut statement) =
        db.prepare("SELECT key,path,bytes FROM cache_entries ORDER BY last_access ASC")
    {
        if let Ok(rows) = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        }) {
            for row in rows.flatten() {
                stale.push(row);
                if total - reclaimed <= CACHE_LIMIT_BYTES {
                    break;
                }
                reclaimed += stale.last().unwrap().2;
            }
        }
    }
    for (stale_key, stale_path, _) in stale {
        let _ = fs::remove_file(stale_path);
        let _ = db.execute("DELETE FROM cache_entries WHERE key=?1", [&stale_key]);
    }
}

#[tauri::command]
async fn get_preview(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    max_edge: u32,
) -> Result<PreviewResult, String> {
    let max_edge = max_edge.clamp(384, 8192);
    let source = PathBuf::from(path);
    let key = preview_key(&source, max_edge)?;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("previews");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let destination = cache_dir.join(format!("{key}.jpg"));
    if destination.exists() {
        let (width, height) = image::image_dimensions(&destination).map_err(|e| e.to_string())?;
        touch_cache(&state, &key, &destination);
        return Ok(PreviewResult {
            cache_path: destination.to_string_lossy().into_owned(),
            width,
            height,
            cached: true,
        });
    }
    let _permit = state
        .decode_slots
        .acquire()
        .await
        .map_err(|e| e.to_string())?;
    let source_copy = source.clone();
    let destination_copy = destination.clone();
    let (width, height) = tauri::async_runtime::spawn_blocking(move || {
        build_preview(&source_copy, &destination_copy, max_edge)
    })
    .await
    .map_err(|e| e.to_string())??;
    touch_cache(&state, &key, &destination);
    Ok(PreviewResult {
        cache_path: destination.to_string_lossy().into_owned(),
        width,
        height,
        cached: false,
    })
}

#[tauri::command]
fn save_project(path: String, project: serde_json::Value) -> Result<(), String> {
    if project
        .get("schemaVersion")
        .and_then(|value| value.as_u64())
        != Some(1)
    {
        return Err("Versione progetto non supportata".into());
    }
    let target = PathBuf::from(path);
    let parent = target.parent().ok_or("Percorso progetto non valido")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = target.with_extension("fujiview.tmp");
    let bytes = serde_json::to_vec_pretty(&project).map_err(|e| e.to_string())?;
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if !target.exists() {
        return fs::rename(temp, target).map_err(|e| e.to_string());
    }
    let backup = target.with_extension("fujiview.bak");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&target, &backup).map_err(|e| e.to_string())?;
    match fs::rename(&temp, &target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::rename(&backup, &target);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
fn load_project(path: String) -> Result<serde_json::Value, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Progetto non valido: {e}"))?;
    if value.get("schemaVersion").and_then(|item| item.as_u64()) != Some(1) {
        return Err("Versione progetto non supportata".into());
    }
    Ok(value)
}

fn embed_rating(path: &Path, rating: i32) -> Result<(), String> {
    let mut file = XmpFile::new().map_err(|e| e.to_string())?;
    file.open_file(
        path,
        OpenFileOptions::default().for_update().use_smart_handler(),
    )
    .map_err(|e| e.to_string())?;
    let mut meta = match file.xmp() {
        Some(value) => value,
        None => XmpMeta::new().map_err(|e| e.to_string())?,
    };
    meta.set_property_i32(xmp_ns::XMP, "Rating", &XmpValue::new(rating.clamp(0, 5)))
        .map_err(|e| e.to_string())?;
    file.put_xmp(&meta).map_err(|e| e.to_string())?;
    file.try_close().map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_winners(
    destination: String,
    items: Vec<ExportItem>,
    overwrite: bool,
) -> Result<ExportReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let destination = PathBuf::from(destination);
        fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        let canonical_destination = destination.canonicalize().map_err(|e| e.to_string())?;
        let mut report = ExportReport::default();
        for item in items {
            let source = PathBuf::from(&item.source_path);
            if !source.is_file() || image_extension(&source).is_none() {
                report
                    .errors
                    .push(format!("Sorgente non valida: {}", item.source_path));
                continue;
            }
            let safe_name = Path::new(&item.output_name)
                .file_name()
                .map(|v| v.to_owned())
                .unwrap_or_default();
            let target = canonical_destination.join(safe_name);
            if target.exists() && !overwrite {
                report.skipped += 1;
                continue;
            }
            let output_path = Path::new(&item.output_name);
            let stem = output_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("image");
            let extension = output_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("jpg");
            let temporary =
                canonical_destination.join(format!(".{stem}.fujiview-export.{extension}"));
            let _ = fs::remove_file(&temporary);
            if let Err(error) = fs::copy(&source, &temporary) {
                report
                    .errors
                    .push(format!("{}: {}", item.output_name, error));
                continue;
            }
            if let Err(error) = embed_rating(&temporary, item.rating) {
                let _ = fs::remove_file(&temporary);
                report
                    .errors
                    .push(format!("{} (XMP): {}", item.output_name, error));
                continue;
            }
            let backup = canonical_destination.join(format!(".{stem}.fujiview-old.{extension}"));
            let had_target = target.exists();
            if had_target {
                let _ = fs::remove_file(&backup);
                if let Err(error) = fs::rename(&target, &backup) {
                    let _ = fs::remove_file(&temporary);
                    report
                        .errors
                        .push(format!("{}: {}", item.output_name, error));
                    continue;
                }
            }
            if let Err(error) = fs::rename(&temporary, &target) {
                if had_target {
                    let _ = fs::rename(&backup, &target);
                }
                let _ = fs::remove_file(&temporary);
                report
                    .errors
                    .push(format!("{}: {}", item.output_name, error));
                continue;
            }
            if had_target {
                let _ = fs::remove_file(&backup);
            }
            report.exported += 1;
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_extensions_and_case_by_stem() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("Classic");
        let b = temp.path().join("Velvia");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("DSC001.JPG"), b"x").unwrap();
        fs::write(b.join("dsc001.tif"), b"x").unwrap();
        let result = scan_library_impl(temp.path()).unwrap();
        assert_eq!(result.shots.len(), 1);
        assert_eq!(result.shots[0].variants.len(), 2);
    }
    #[test]
    fn ignores_root_files_and_nested_directories() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("A");
        fs::create_dir_all(preset.join("nested")).unwrap();
        fs::write(temp.path().join("root.jpg"), b"x").unwrap();
        fs::write(preset.join("nested/hidden.jpg"), b"x").unwrap();
        let result = scan_library_impl(temp.path()).unwrap();
        assert!(result.shots.is_empty());
    }
    #[test]
    fn ignores_fujifilm_raw_and_sidecar_files() {
        let temp = tempfile::tempdir().unwrap();
        let preset = temp.path().join("RAW");
        fs::create_dir_all(&preset).unwrap();
        fs::write(preset.join("DSCF0001.RAF"), b"raw").unwrap();
        fs::write(preset.join("DSCF0001.FP2"), b"sidecar").unwrap();
        fs::write(preset.join("DSCF0001.FP3"), b"sidecar").unwrap();
        fs::write(preset.join("DSCF0002.jpg"), b"jpeg").unwrap();

        let result = scan_library_impl(temp.path()).unwrap();
        assert_eq!(result.shots.len(), 1);
        assert_eq!(result.shots[0].display_name, "DSCF0002");
        assert_eq!(result.shots[0].variants.len(), 1);
    }
    #[test]
    fn largest_preset_is_the_guide_and_shots_use_natural_order() {
        let temp = tempfile::tempdir().unwrap();
        let smaller = temp.path().join("Preset 2");
        let guide = temp.path().join("Preset 10");
        fs::create_dir_all(&smaller).unwrap();
        fs::create_dir_all(&guide).unwrap();
        for name in ["DSCF2.jpg", "DSCF10.jpg", "ONLY_SMALLER.jpg"] {
            fs::write(smaller.join(name), b"jpeg").unwrap();
        }
        for name in ["DSCF1.jpg", "DSCF2.jpg", "DSCF3.jpg", "DSCF10.jpg"] {
            fs::write(guide.join(name), b"jpeg").unwrap();
        }

        let result = scan_library_impl(temp.path()).unwrap();
        assert_eq!(result.guide_preset, "Preset 10");
        assert_eq!(
            result
                .shots
                .iter()
                .map(|shot| shot.display_name.as_str())
                .collect::<Vec<_>>(),
            ["DSCF1", "DSCF2", "DSCF3", "DSCF10"]
        );
        assert!(result.shots.iter().all(|shot| shot.key != "only_smaller"));
    }
    #[test]
    fn creates_a_bounded_preview() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.jpg");
        let target = temp.path().join("preview.jpg");
        let pixels = vec![127u8; 100 * 50 * 3];
        JpegEncoder::new_with_quality(fs::File::create(&source).unwrap(), 95)
            .write_image(&pixels, 100, 50, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(build_preview(&source, &target, 40).unwrap(), (40, 20));
        assert_eq!(image::image_dimensions(target).unwrap(), (40, 20));
    }
    #[test]
    fn xmp_rating_keeps_image_pixels_readable() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("rated.jpg");
        let pixels = vec![90u8; 16 * 16 * 3];
        JpegEncoder::new_with_quality(fs::File::create(&source).unwrap(), 95)
            .write_image(&pixels, 16, 16, image::ExtendedColorType::Rgb8)
            .unwrap();
        let before = image::open(&source).unwrap().to_rgb8();
        embed_rating(&source, 4).unwrap();
        let after = image::open(&source).unwrap().to_rgb8();
        assert_eq!(before.as_raw(), after.as_raw());
        let meta = XmpMeta::from_file(&source).unwrap();
        assert_eq!(meta.property_i32(xmp_ns::XMP, "Rating").unwrap().value, 4);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { decode_slots: Semaphore::new(2), cache_db: Mutex::new(None) })
        .setup(|app| {
            let cache_dir = app.path().app_cache_dir()?.join("previews");
            fs::create_dir_all(&cache_dir)?;
            let db = Connection::open(app.path().app_cache_dir()?.join("cache.db"))?;
            db.execute_batch("CREATE TABLE IF NOT EXISTS cache_entries(key TEXT PRIMARY KEY,path TEXT NOT NULL,bytes INTEGER NOT NULL,last_access INTEGER NOT NULL);")?;
            *app.state::<AppState>().cache_db.lock().unwrap() = Some(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![scan_library, get_preview, save_project, load_project, export_winners])
        .run(tauri::generate_context!())
        .expect("error while running FujiView");
}
