//! Feasibility probe for the Fujifilm connector (development only).
//!
//!   fuji_probe list
//!   fuji_probe info  [--save tests/fixtures/xt5_device_info.bin]
//!   fuji_probe banks [--save tests/fixtures/xt5_banks.json]
//!   fuji_probe convert <raf> <out.jpg> [--save-profile tests/fixtures/xt5_profile_asshot.bin]
//!   fuji_probe multi <raf> <out_dir>           two film simulations after one upload
//!   fuji_probe bank <raf> <k> <out_dir>        convert with custom bank k
//!   fuji_probe batch <raf_dir> <out_dir> [--overwrite]
//!
//! Add `--sim` to any command to use the simulated camera instead of USB.

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("ERRORE: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("fuji_probe supporta solo Windows");
}

#[cfg(windows)]
type Probe =
    fujiview_lib::camera::fuji::FujiCamera<Box<dyn fujiview_lib::camera::ptp::PtpTransport>>;

#[cfg(windows)]
fn open_camera(sim: bool) -> Result<Probe, Box<dyn std::error::Error>> {
    use fujiview_lib::camera::{fuji::FujiCamera, sim::SimulatedCamera, wpd};
    if sim {
        println!("Camera SIMULATA");
        return Ok(FujiCamera::new(Box::new(SimulatedCamera::new())));
    }
    let device = wpd::find_fujifilm()?.ok_or("nessuna camera Fujifilm trovata")?;
    println!("Device: {} ({})", device.friendly_name, device.id);
    Ok(FujiCamera::new(Box::new(wpd::WpdTransport::open(
        &device.id,
    )?)))
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use fujiview_lib::camera::banks::{apply_bank, as_shot, film_simulation_name, property_label};
    use fujiview_lib::camera::profile::cell;
    use fujiview_lib::camera::ptp::{op, prop, DataPhase, DeviceInfo, PtpTransport};
    use fujiview_lib::camera::wpd::{self, ComGuard, WpdTransport};
    use fujiview_lib::variants::{plan, runner, types::VariantRequest};
    use std::time::Instant;

    let _com = ComGuard::init()?;
    let all: Vec<String> = std::env::args().skip(1).collect();
    let sim = all.iter().any(|a| a == "--sim");
    let flag = |name: &str| {
        all.iter()
            .position(|a| a == name)
            .and_then(|i| all.get(i + 1))
            .cloned()
    };
    let mut positional = Vec::new();
    let mut skip = false;
    for arg in &all {
        if skip {
            skip = false;
        } else if arg.starts_with("--") {
            skip = matches!(arg.as_str(), "--save" | "--save-profile");
        } else {
            positional.push(arg.clone());
        }
    }
    let arg = |i: usize| {
        positional
            .get(i)
            .cloned()
            .ok_or_else(|| format!("argomento {i} mancante"))
    };
    let command = positional.first().cloned().unwrap_or_else(|| "list".into());
    let timed =
        |label: &str, start: Instant| println!("  {label}: {:.2} s", start.elapsed().as_secs_f64());

    match command.as_str() {
        "list" => {
            for device in wpd::list_devices()? {
                let tag = if device.is_fujifilm() { "FUJI" } else { "    " };
                println!(
                    "{tag} | {} | {} | {}\n    {}",
                    device.friendly_name, device.manufacturer, device.description, device.id
                );
            }
        }
        "info" => {
            let device = wpd::find_fujifilm()?.ok_or("nessuna camera Fujifilm trovata")?;
            println!("Device: {} ({})", device.friendly_name, device.id);
            let mut camera = WpdTransport::open(&device.id)?;
            match camera.supported_vendor_opcodes() {
                Ok(codes) => println!("Vendor opcodes inoltrati dal driver: {}", hex_list(&codes)),
                Err(error) => println!("Vendor opcodes non disponibili: {error}"),
            }
            let response = camera.execute_ok(op::GET_DEVICE_INFO, &[], DataPhase::Read)?;
            if let Some(path) = flag("--save") {
                std::fs::write(&path, &response.data)?;
                println!(
                    "DeviceInfo salvato in {path} ({} byte)",
                    response.data.len()
                );
            }
            let info = DeviceInfo::parse(&response.data)?;
            println!(
                "Produttore: {}\nModello: {}\nFirmware: {}\nSeriale: {}",
                info.manufacturer, info.model, info.device_version, info.serial_number
            );
            println!(
                "Vendor ext: 0x{:08X} v{} '{}'",
                info.vendor_extension_id, info.vendor_extension_version, info.vendor_extension_desc
            );
            println!("Modalita funzionale: 0x{:04X}", info.functional_mode);
            match camera.execute_ok(
                op::GET_DEVICE_PROP_VALUE,
                &[prop::USB_MODE as u32],
                DataPhase::Read,
            ) {
                Ok(r) => println!("Modalita USB (0xD16E): {:02X?} (6 = RAW CONV)", r.data),
                Err(e) => println!("Modalita USB (0xD16E): errore {e}"),
            }
            println!(
                "Operazioni ({}): {}",
                info.operations.len(),
                hex_list(&info.operations)
            );
            println!("Eventi ({}): {}", info.events.len(), hex_list(&info.events));
            println!(
                "Proprieta ({}): {}",
                info.properties.len(),
                hex_list(&info.properties)
            );
            let missing = info.missing_conversion_support();
            let banks_missing: Vec<u16> = (0xD18Cu16..=0xD1A4)
                .filter(|p| !info.supports_property(*p))
                .collect();
            println!(
                "Mancanti per la conversione: {}",
                if missing.is_empty() {
                    "nessuno".into()
                } else {
                    missing.join(", ")
                }
            );
            println!(
                "Proprieta banchi mancanti: {}",
                if banks_missing.is_empty() {
                    "nessuna".into()
                } else {
                    hex_list(&banks_missing)
                }
            );
        }
        "banks" => {
            let mut camera = open_camera(sim)?;
            let before = camera.get_prop(0xD18C)?;
            let banks = camera.read_banks()?;
            let after = camera.get_prop(0xD18C)?;
            for bank in &banks {
                println!("C{} '{}' — {}", bank.index, bank.name, bank.summary());
                for (code, value) in &bank.props {
                    println!("    {code:04X} = {value:<6} {}", property_label(*code));
                }
            }
            println!(
                "Banco selezionato prima {before:?} / dopo {after:?} → {}",
                if before == after {
                    "invariato"
                } else {
                    "CAMBIATO!"
                }
            );
            if let Some(path) = flag("--save") {
                std::fs::write(&path, serde_json::to_string_pretty(&banks)?)?;
                println!("Banchi salvati in {path}");
            }
        }
        "convert" => {
            let (raf, out) = (arg(1)?, arg(2)?);
            let mut camera = open_camera(sim)?;
            println!("Oggetti residui cancellati: {}", camera.clear_objects()?);
            let bytes = std::fs::read(&raf)?;
            let start = Instant::now();
            camera.upload_raf(&bytes)?;
            timed(&format!("upload {:.1} MB", bytes.len() as f64 / 1e6), start);
            let start = Instant::now();
            let profile = camera.read_profile()?;
            timed("lettura profilo", start);
            println!(
                "Profilo: {} byte, IOPCode {}, {} celle (dichiarate {})",
                profile.to_bytes().len(),
                profile.iop_code(),
                profile.cell_count(),
                profile.declared_params()
            );
            println!("Celle: {:?}", profile.cells());
            if let Some(path) = flag("--save-profile") {
                std::fs::write(&path, profile.to_bytes())?;
                println!("Profilo salvato in {path}");
            }
            let start = Instant::now();
            let jpeg = camera.convert(&as_shot(&profile)?)?;
            timed("conversione + download", start);
            write_out(&out, &jpeg)?;
        }
        "multi" => {
            let (raf, out) = (arg(1)?, arg(2)?);
            let mut camera = open_camera(sim)?;
            camera.clear_objects()?;
            camera.upload_raf(&std::fs::read(&raf)?)?;
            let base = camera.read_profile()?;
            for film in [11, 12] {
                let mut profile = as_shot(&base)?;
                profile.set(cell::FILM_SIMULATION, film)?;
                let start = Instant::now();
                let jpeg = camera.convert(&profile)?;
                timed(
                    &format!("{}", film_simulation_name(film).unwrap_or("?")),
                    start,
                );
                write_out(&format!("{out}/film_{film}.jpg"), &jpeg)?;
            }
        }
        "bank" => {
            let (raf, k, out) = (arg(1)?, arg(2)?.parse::<u8>()?, arg(3)?);
            let mut camera = open_camera(sim)?;
            let bank = camera
                .read_banks()?
                .into_iter()
                .find(|b| b.index == k)
                .ok_or("banco inesistente")?;
            camera.clear_objects()?;
            camera.upload_raf(&std::fs::read(&raf)?)?;
            let base = camera.read_profile()?;
            let applied = apply_bank(&base, &bank)?;
            println!("C{} '{}' — {}", bank.index, bank.name, bank.summary());
            for (i, (a, b)) in base.cells().iter().zip(applied.profile.cells()).enumerate() {
                if a != b {
                    println!("    cella {i:>2}: {a} → {b}");
                }
            }
            println!("Non applicate: {:?}", applied.unapplied);
            let jpeg = camera.convert(&applied.profile)?;
            let stem = std::path::Path::new(&raf)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            write_out(
                &format!(
                    "{out}/{}/{stem}.jpg",
                    fujiview_lib::camera::banks::folder_name(bank.index, &bank.name)
                ),
                &jpeg,
            )?;
        }
        "batch" => {
            let (source, out) = (arg(1)?, arg(2)?);
            let mut camera = open_camera(sim)?;
            let mut presets = vec![plan::PresetDef::as_shot()];
            presets.extend(camera.read_banks()?.into_iter().map(plan::PresetDef::bank));
            let request = VariantRequest {
                sources: vec![source],
                output_dir: Some(out),
                preset_ids: presets.iter().map(|p| p.id.clone()).collect(),
                overwrite: all.iter().any(|a| a == "--overwrite"),
            };
            let plan = plan::build_plan(&request, &presets)?;
            let summary = plan.summary();
            println!(
                "{} RAF × {} preset = {} (da fare {}, già fatte {})",
                summary.raf_count,
                summary.presets.len(),
                summary.total_conversions,
                summary.pending_conversions,
                summary.already_done
            );
            let start = Instant::now();
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let report = runner::run_plan(
                &mut camera,
                &plan,
                runner::RunOptions::default(),
                &cancel,
                &mut |p| {
                    if let (Some(raf), Some(preset)) = (&p.current_raf, &p.current_preset) {
                        println!(
                            "  [{}/{}] {raf} → {preset}",
                            p.completed + p.skipped + p.failed + 1,
                            p.total
                        );
                    }
                },
            );
            println!(
                "Esito: {:?}, convertite {}, saltate {}, errori {}",
                report.state,
                report.converted,
                report.skipped,
                report.failures.len()
            );
            for failure in &report.failures {
                println!(
                    "  ✗ {} {:?}: {}",
                    failure.raf, failure.preset, failure.reason
                );
            }
            if let Some(reason) = report.paused_reason {
                println!("Motivo: {reason}");
            }
            if report.converted > 0 {
                println!(
                    "Media: {:.2} s per variante",
                    start.elapsed().as_secs_f64() / report.converted as f64
                );
            }
        }
        "matrix" => {
            // Test cases built in memory from the camera's banks; the banks on
            // the camera are never written. cases.json:
            // [{"name": "...", "bank": "REGGIE" | null, "props": {"D198": 3}, "cells": {"7": 1}}]
            let (raf, cases, out) = (arg(1)?, arg(2)?, arg(3)?);
            let cases: Vec<serde_json::Value> =
                serde_json::from_str(&std::fs::read_to_string(&cases)?)?;
            let mut camera = open_camera(sim)?;
            let banks = camera.read_banks()?;
            camera.clear_objects()?;
            camera.upload_raf(&std::fs::read(&raf)?)?;
            let base = camera.read_profile()?;
            for case in &cases {
                let name = case["name"].as_str().ok_or("name mancante")?;
                let mut profile = as_shot(&base)?;
                let mut unapplied = Vec::new();
                if let Some(bank_name) = case["bank"].as_str() {
                    let mut bank = banks
                        .iter()
                        .find(|b| b.name == bank_name)
                        .ok_or(format!("banco {bank_name} inesistente"))?
                        .clone();
                    if let Some(props) = case["props"].as_object() {
                        for (code, value) in props {
                            let code = u16::from_str_radix(code.trim_start_matches("0x"), 16)?;
                            bank.props
                                .insert(code, value.as_i64().ok_or("valore")? as i32);
                        }
                    }
                    let applied = apply_bank(&base, &bank)?;
                    profile = applied.profile;
                    unapplied = applied.unapplied;
                }
                if let Some(cells) = case["cells"].as_object() {
                    for (index, value) in cells {
                        profile.set(index.parse()?, value.as_i64().ok_or("valore")? as i32)?;
                    }
                }
                let changed: Vec<String> = base
                    .cells()
                    .iter()
                    .zip(profile.cells())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(i, (a, b))| format!("{i}:{a}→{b}"))
                    .collect();
                let started = Instant::now();
                match camera.convert(&profile) {
                    Ok(jpeg) => {
                        write_out(&format!("{out}/{name}.jpg"), &jpeg)?;
                        println!(
                            "✓ {name} ({:.1} s) celle {changed:?} non applicate {unapplied:?}",
                            started.elapsed().as_secs_f64()
                        );
                    }
                    Err(e) => println!("✗ {name}: {e} — celle {changed:?}"),
                }
            }
        }
        other => return Err(format!("comando sconosciuto: {other}").into()),
    }
    Ok(())
}

#[cfg(windows)]
fn write_out(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    let path = std::path::Path::new(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    println!(
        "  → {} ({:.1} MB)",
        path.display(),
        bytes.len() as f64 / 1e6
    );
    Ok(())
}

#[cfg(windows)]
fn hex_list(codes: &[u16]) -> String {
    codes
        .iter()
        .map(|c| format!("{c:04X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
