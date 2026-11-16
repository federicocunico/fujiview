//! Shot metadata for the side panel: EXIF shooting data, GPS, file info and
//! the Fujifilm recipe (film simulation, DR, WB, tones…) from the MakerNote.
//! Code tables were checked against real X-T5 MakerNotes
//! (`tests/fixtures/xt5_makernote_matrix.json`).

use serde::Serialize;
use std::{fs, io::BufReader, path::Path};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaRow {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaSection {
    pub title: String,
    pub rows: Vec<MetaRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpsPoint {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    /// Short line, e.g. "Classic Chrome · DR200 · Auto".
    pub summary: String,
    pub rows: Vec<MetaRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadata {
    pub sections: Vec<MetaSection>,
    pub gps: Option<GpsPoint>,
    pub recipe: Option<Recipe>,
}

#[tauri::command]
pub async fn image_metadata(path: String) -> Result<ImageMetadata, String> {
    tauri::async_runtime::spawn_blocking(move || read_metadata(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

fn row(rows: &mut Vec<MetaRow>, label: &str, value: Option<String>) {
    if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
        rows.push(MetaRow {
            label: label.into(),
            value,
        });
    }
}

pub fn read_metadata(path: &Path) -> Result<ImageMetadata, String> {
    let file_meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let exif = fs::File::open(path).ok().and_then(|file| {
        exif::Reader::new()
            .read_from_container(&mut BufReader::new(file))
            .ok()
    });

    let mut sections = Vec::new();
    let mut gps = None;
    let mut recipe = None;
    let mut maker = MakerNote::default();

    if let Some(exif) = &exif {
        if let Some(exif::Value::Undefined(bytes, _)) =
            field(exif, exif::Tag::MakerNote).map(|f| &f.value)
        {
            maker = MakerNote::parse(bytes).unwrap_or_default();
        }

        let mut shot = Vec::new();
        row(&mut shot, "Data e ora", date_time(exif));
        row(
            &mut shot,
            "Tempo",
            rational(exif, exif::Tag::ExposureTime).map(format_exposure),
        );
        row(
            &mut shot,
            "Diaframma",
            rational(exif, exif::Tag::FNumber).map(|f| format!("f/{}", trim_float(f, 1))),
        );
        row(
            &mut shot,
            "ISO",
            uint(exif, exif::Tag::PhotographicSensitivity).map(|v| v.to_string()),
        );
        row(
            &mut shot,
            "Compensazione",
            srational(exif, exif::Tag::ExposureBiasValue).map(format_bias),
        );
        row(&mut shot, "Focale", focal_length(exif));
        row(
            &mut shot,
            "Programma",
            uint(exif, exif::Tag::ExposureProgram).and_then(exposure_program),
        );
        row(
            &mut shot,
            "Esposimetro",
            uint(exif, exif::Tag::MeteringMode).and_then(metering_mode),
        );
        row(
            &mut shot,
            "Flash",
            uint(exif, exif::Tag::Flash).map(|v| {
                if v & 1 == 1 {
                    "Scattato"
                } else {
                    "Non scattato"
                }
                .into()
            }),
        );
        row(
            &mut shot,
            "Otturatore",
            maker.get(0x1050).and_then(shutter_type),
        );
        row(
            &mut shot,
            "Messa a fuoco",
            maker.get(0x1021).and_then(focus_mode),
        );
        if !shot.is_empty() {
            sections.push(MetaSection {
                title: "Scatto".into(),
                rows: shot,
            });
        }

        let mut camera = Vec::new();
        row(&mut camera, "Fotocamera", camera_name(exif));
        row(&mut camera, "Obiettivo", lens_name(exif));
        row(&mut camera, "Firmware", text(exif, exif::Tag::Software));
        if !camera.is_empty() {
            sections.push(MetaSection {
                title: "Fotocamera".into(),
                rows: camera,
            });
        }

        gps = gps_point(exif);
        if let Some(point) = &gps {
            let mut place = vec![MetaRow {
                label: "Coordinate".into(),
                value: format!("{:.6}, {:.6}", point.latitude, point.longitude),
            }];
            row(
                &mut place,
                "Altitudine",
                point.altitude.map(|a| format!("{a:.0} m")),
            );
            sections.push(MetaSection {
                title: "Luogo".into(),
                rows: place,
            });
        }

        recipe = maker.recipe();
    }

    let mut file = Vec::new();
    row(
        &mut file,
        "Nome",
        path.file_name().map(|n| n.to_string_lossy().into_owned()),
    );
    let dims = exif.as_ref().and_then(|e| {
        Some((
            uint(e, exif::Tag::PixelXDimension)?,
            uint(e, exif::Tag::PixelYDimension)?,
        ))
    });
    row(
        &mut file,
        "Dimensioni",
        dims.map(|(w, h)| {
            format!("{w} × {h} px ({:.1} MP)", (w as f64 * h as f64) / 1e6).replace('.', ",")
        }),
    );
    row(
        &mut file,
        "Peso",
        Some(format!("{:.1} MB", file_meta.len() as f64 / 1_048_576.0).replace('.', ",")),
    );
    row(
        &mut file,
        "Cartella",
        path.parent().map(|p| p.display().to_string()),
    );
    sections.push(MetaSection {
        title: "File".into(),
        rows: file,
    });

    Ok(ImageMetadata {
        sections,
        gps,
        recipe,
    })
}

// ---------- EXIF helpers ----------

fn field(exif: &exif::Exif, tag: exif::Tag) -> Option<&exif::Field> {
    exif.get_field(tag, exif::In::PRIMARY)
}

fn text(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    match &field(exif, tag)?.value {
        exif::Value::Ascii(parts) => parts
            .first()
            .map(|p| {
                String::from_utf8_lossy(p)
                    .trim_matches(char::from(0))
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

fn uint(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    field(exif, tag)?.value.get_uint(0)
}

fn rational(exif: &exif::Exif, tag: exif::Tag) -> Option<f64> {
    match &field(exif, tag)?.value {
        exif::Value::Rational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
}

fn srational(exif: &exif::Exif, tag: exif::Tag) -> Option<f64> {
    match &field(exif, tag)?.value {
        exif::Value::SRational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        exif::Value::Rational(v) => v.first().filter(|r| r.denom != 0).map(|r| r.to_f64()),
        _ => None,
    }
}

fn trim_float(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    s.replace('.', ",")
}

/// "2026:11:12 15:30:01" (+ "+01:00") → "12/11/2026 15:30:01 (UTC+01:00)".
fn date_time(exif: &exif::Exif) -> Option<String> {
    let raw =
        text(exif, exif::Tag::DateTimeOriginal).or_else(|| text(exif, exif::Tag::DateTime))?;
    let (date, time) = raw.split_once(' ')?;
    let mut parts = date.split(':');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    let mut out = format!("{d}/{m}/{y} {time}");
    if let Some(offset) = text(exif, exif::Tag::OffsetTimeOriginal) {
        out.push_str(&format!(" (UTC{offset})"));
    }
    Some(out)
}

pub fn format_exposure(seconds: f64) -> String {
    if seconds <= 0.0 {
        return "—".into();
    }
    if seconds < 0.3 {
        format!("1/{} s", (1.0 / seconds).round() as u64)
    } else {
        format!("{} s", trim_float(seconds, 1))
    }
}

pub fn format_bias(ev: f64) -> String {
    if ev.abs() < 0.01 {
        "0 EV".into()
    } else {
        format!(
            "{}{} EV",
            if ev > 0.0 { "+" } else { "−" },
            trim_float(ev.abs(), 1)
        )
    }
}

fn focal_length(exif: &exif::Exif) -> Option<String> {
    let focal = rational(exif, exif::Tag::FocalLength)?;
    let mut out = format!("{} mm", trim_float(focal, 1));
    if let Some(eq) = uint(exif, exif::Tag::FocalLengthIn35mmFilm).filter(|v| *v > 0) {
        out.push_str(&format!(" ({eq} mm equiv.)"));
    }
    Some(out)
}

fn camera_name(exif: &exif::Exif) -> Option<String> {
    let model = text(exif, exif::Tag::Model)?;
    Some(match text(exif, exif::Tag::Make) {
        Some(make) if !model.to_lowercase().starts_with(&make.to_lowercase()) => {
            format!("{make} {model}")
        }
        _ => model,
    })
}

fn lens_name(exif: &exif::Exif) -> Option<String> {
    let model = text(exif, exif::Tag::LensModel)?;
    Some(match text(exif, exif::Tag::LensMake) {
        Some(make) if !model.to_lowercase().contains(&make.to_lowercase()) => {
            format!("{make} {model}")
        }
        _ => model,
    })
}

fn gps_point(exif: &exif::Exif) -> Option<GpsPoint> {
    let coord = |tag, reference| -> Option<f64> {
        let exif::Value::Rational(v) = &field(exif, tag)?.value else {
            return None;
        };
        if v.len() < 3 || v.iter().any(|r| r.denom == 0) {
            return None;
        }
        let value = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0;
        let negative = matches!(text(exif, reference).as_deref(), Some("S") | Some("W"));
        Some(if negative { -value } else { value })
    };
    let latitude = coord(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?;
    let longitude = coord(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?;
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }
    let altitude = rational(exif, exif::Tag::GPSAltitude).map(|a| {
        if uint(exif, exif::Tag::GPSAltitudeRef) == Some(1) {
            -a
        } else {
            a
        }
    });
    Some(GpsPoint {
        latitude,
        longitude,
        altitude,
    })
}

fn exposure_program(v: u32) -> Option<String> {
    Some(
        match v {
            1 => "Manuale",
            2 => "Programma",
            3 => "Priorità diaframma",
            4 => "Priorità tempi",
            _ => return None,
        }
        .into(),
    )
}

fn metering_mode(v: u32) -> Option<String> {
    Some(
        match v {
            1 => "Media",
            2 => "Media ponderata al centro",
            3 => "Spot",
            4 => "Multi-spot",
            5 => "Multi-zona",
            6 => "Parziale",
            _ => return None,
        }
        .into(),
    )
}

fn shutter_type(v: i64) -> Option<String> {
    Some(
        match v {
            0 => "Meccanico",
            1 => "Elettronico",
            2 => "Elettronico (posa lunga)",
            3 => "Prima tendina elettronica",
            _ => return None,
        }
        .into(),
    )
}

fn focus_mode(v: i64) -> Option<String> {
    Some(
        match v {
            0 => "Automatica",
            1 => "Manuale",
            _ => return None,
        }
        .into(),
    )
}

// ---------- Fujifilm MakerNote ----------

/// Integer tags of a FUJIFILM MakerNote ("FUJIFILM" + LE IFD offset; offsets
/// are relative to the MakerNote start).
#[derive(Debug, Default, Clone)]
pub struct MakerNote {
    tags: std::collections::BTreeMap<u16, i64>,
}

impl MakerNote {
    pub fn parse(mn: &[u8]) -> Option<Self> {
        if mn.len() < 12 || &mn[..8] != b"FUJIFILM" {
            return None;
        }
        let u16_at = |o: usize| mn.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| {
            mn.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let ifd = u32_at(8)? as usize;
        let count = u16_at(ifd)? as usize;
        let mut tags = std::collections::BTreeMap::new();
        for i in 0..count {
            let e = ifd + 2 + 12 * i;
            let (Some(tag), Some(typ), Some(n)) = (u16_at(e), u16_at(e + 2), u32_at(e + 4)) else {
                break;
            };
            if n != 1 {
                continue;
            }
            let value = match typ {
                1 => mn.get(e + 8).map(|b| *b as i64),
                3 => u16_at(e + 8).map(i64::from),
                8 => u16_at(e + 8).map(|v| v as i16 as i64),
                4 => u32_at(e + 8).map(i64::from),
                9 => u32_at(e + 8).map(|v| v as i32 as i64),
                _ => None,
            };
            if let Some(value) = value {
                tags.insert(tag, value);
            }
        }
        Some(Self { tags })
    }

    pub fn get(&self, tag: u16) -> Option<i64> {
        self.tags.get(&tag).copied()
    }

    pub fn recipe(&self) -> Option<Recipe> {
        let film = film_name(self.get(0x1401), self.get(0x1003));
        let mono = self.get(0x1401).is_none()
            && self.get(0x1003).is_some_and(|s| s >= 0x300 && s != 0x8000);
        let dr = self
            .get(0x1403)
            .filter(|v| *v > 0)
            .map(|v| format!("DR{v}"));
        let wb = self.get(0x1002).and_then(white_balance);
        let mut rows = Vec::new();
        row(&mut rows, "Simulazione", film.clone());
        row(&mut rows, "Gamma dinamica", dr.clone());
        row(&mut rows, "Bilanciamento bianco", wb.clone());
        row(&mut rows, "Luci", self.get(0x1041).map(tone));
        row(&mut rows, "Ombre", self.get(0x1040).map(tone));
        if !mono {
            row(&mut rows, "Colore", self.get(0x1003).and_then(color));
        }
        row(&mut rows, "Nitidezza", self.get(0x1001).and_then(sharpness));
        row(
            &mut rows,
            "Riduzione rumore",
            self.get(0x100E).and_then(noise_reduction),
        );
        row(
            &mut rows,
            "Chiarezza",
            self.get(0x100F).map(|v| signed(v as f64 / 1000.0)),
        );
        row(
            &mut rows,
            "Grana",
            grain(self.get(0x1047), self.get(0x104C)),
        );
        row(&mut rows, "Color Chrome", self.get(0x1048).and_then(effect));
        row(
            &mut rows,
            "Color Chrome FX Blu",
            self.get(0x104E).and_then(effect),
        );
        if rows.is_empty() {
            return None;
        }
        let summary = [film, dr, wb]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        Some(Recipe { summary, rows })
    }
}

/// FilmMode (0x1401) for colour; monochrome has no FilmMode and is encoded in
/// Saturation (0x1003).
pub fn film_name(film_mode: Option<i64>, saturation: Option<i64>) -> Option<String> {
    let name = match film_mode {
        Some(mode) => match mode {
            0x000 => "Provia",
            0x120 => "Astia",
            0x200 | 0x400 => "Velvia",
            0x500 => "Pro Neg. Std",
            0x501 => "Pro Neg. Hi",
            0x600 => "Classic Chrome",
            0x700 => "Eterna",
            0x800 => "Classic Neg.",
            0x900 => "Eterna Bleach Bypass",
            0xA00 => "Nostalgic Neg.",
            0xB00 => "Reala Ace",
            _ => return Some(format!("Simulazione 0x{mode:X}")),
        },
        None => match saturation? {
            0x300 => "Monocromatico",
            0x301 => "Monocromatico + R",
            0x302 => "Monocromatico + Ye",
            0x303 => "Monocromatico + G",
            0x310 => "Seppia",
            0x500 => "Acros",
            0x501 => "Acros + R",
            0x502 => "Acros + Ye",
            0x503 => "Acros + G",
            _ => return None,
        },
    };
    Some(name.into())
}

pub fn white_balance(v: i64) -> Option<String> {
    Some(
        match v {
            0x000 => "Auto",
            0x001 => "Auto (priorità bianco)",
            0x002 => "Auto (priorità ambiente)",
            0x100 => "Luce diurna",
            0x200 => "Ombra",
            0x300 => "Fluorescente 1",
            0x301 => "Fluorescente 2",
            0x302 => "Fluorescente 3",
            0x400 => "Incandescenza",
            0x500 => "Flash",
            0x600 => "Subacqueo",
            0xF00..=0xF0F => "Personalizzato",
            0xFF0 => "Temperatura colore (K)",
            _ => return None,
        }
        .into(),
    )
}

fn signed(v: f64) -> String {
    if v.abs() < 0.01 {
        "0".into()
    } else {
        format!(
            "{}{}",
            if v > 0.0 { "+" } else { "−" },
            trim_float(v.abs(), 1)
        )
    }
}

/// Highlight/shadow tone: −16 per step, so −64 = +4 and 16 = −1.
pub fn tone(v: i64) -> String {
    signed(-(v as f64) / 16.0)
}

pub fn color(v: i64) -> Option<String> {
    Some(
        match v {
            0x000 => "0",
            0x080 => "+1",
            0x100 => "+2",
            0x0C0 => "+3",
            0x0E0 => "+4",
            0x180 => "−1",
            0x200 => "−2",
            0x4C0 => "−3",
            0x4E0 => "−4",
            _ => return None,
        }
        .into(),
    )
}

pub fn sharpness(v: i64) -> Option<String> {
    Some(
        match v {
            0x00 => "−4",
            0x01 => "−3",
            0x02 => "−2",
            0x82 => "−1",
            0x03 => "0",
            0x84 => "+1",
            0x04 => "+2",
            0x05 => "+3",
            0x06 => "+4",
            _ => return None,
        }
        .into(),
    )
}

pub fn noise_reduction(v: i64) -> Option<String> {
    Some(
        match v {
            0x000 => "0",
            0x100 => "+2",
            0x180 => "+1",
            0x1C0 => "+3",
            0x1E0 => "+4",
            0x200 => "−2",
            0x280 => "−1",
            0x2C0 => "−3",
            0x2E0 => "−4",
            _ => return None,
        }
        .into(),
    )
}

fn effect(v: i64) -> Option<String> {
    Some(
        match v {
            0 => "Off",
            32 => "Debole",
            64 => "Forte",
            _ => return None,
        }
        .into(),
    )
}

fn grain(roughness: Option<i64>, size: Option<i64>) -> Option<String> {
    let strength = effect(roughness?)?;
    if strength == "Off" {
        return Some(strength);
    }
    let size = match size {
        Some(16) => " · piccola",
        Some(32) => " · grande",
        _ => "",
    };
    Some(format!("{strength}{size}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn makernote(tags: &[(u16, u16, u32)]) -> Vec<u8> {
        let mut mn = b"FUJIFILM".to_vec();
        mn.extend_from_slice(&12u32.to_le_bytes());
        mn.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        for (tag, typ, value) in tags {
            mn.extend_from_slice(&tag.to_le_bytes());
            mn.extend_from_slice(&typ.to_le_bytes());
            mn.extend_from_slice(&1u32.to_le_bytes());
            mn.extend_from_slice(&value.to_le_bytes());
        }
        mn
    }

    #[test]
    fn parses_makernote_and_builds_recipe() {
        let mn = makernote(&[
            (0x1401, 3, 0x600),
            (0x1403, 3, 200),
            (0x1002, 3, 0),
            (0x1041, 9, (-64i32) as u32),
            (0x1040, 9, 32),
            (0x1047, 4, 32),
            (0x104C, 4, 16),
        ]);
        let recipe = MakerNote::parse(&mn).unwrap().recipe().unwrap();
        assert_eq!(recipe.summary, "Classic Chrome · DR200 · Auto");
        let value = |label: &str| {
            recipe
                .rows
                .iter()
                .find(|r| r.label == label)
                .map(|r| r.value.clone())
        };
        assert_eq!(value("Luci").as_deref(), Some("+4"));
        assert_eq!(value("Ombre").as_deref(), Some("−2"));
        assert_eq!(value("Grana").as_deref(), Some("Debole · piccola"));
        assert!(MakerNote::parse(b"NOTFUJI").is_none());
    }

    /// Every case of the real X-T5 matrix decodes to the settings that produced it.
    #[test]
    fn decodes_real_xt5_makernotes() {
        let text = include_str!("../tests/fixtures/xt5_makernote_matrix.json");
        let matrix: Value = serde_json::from_str(text).unwrap();
        let wb_codes = [
            (2, 0x000),
            (4, 0x100),
            (6, 0x400),
            (0x8001, 0x300),
            (0x8002, 0x301),
            (0x8003, 0x302),
            (0x8006, 0x200),
            (0x8020, 0x001),
            (0x8021, 0x002),
        ];
        for case in matrix["cases"].as_array().unwrap() {
            let mn = &case["makernote"];
            let get = |tag: &str| mn[tag].as_i64();
            let props = &case["props"];
            if let Some(film) = props["D192"].as_i64() {
                let expected = crate::camera::banks::film_simulation_name(film as i32).unwrap();
                let got = film_name(get("0x1401"), get("0x1003")).unwrap();
                assert_eq!(
                    got.replace("Monocromatico", "Mono"),
                    expected.replace("Monocromatico", "Mono"),
                    "{}",
                    case["name"]
                );
            }
            if let Some(wb) = props["D199"].as_i64() {
                if let Some((_, code)) = wb_codes.iter().find(|(prop, _)| *prop == wb) {
                    assert_eq!(get("0x1002"), Some(*code), "{}", case["name"]);
                    assert!(white_balance(*code).is_some());
                }
            }
            for tag in ["0x1002", "0x1001", "0x100E"] {
                if let Some(v) = get(tag) {
                    let decoded = match tag {
                        "0x1002" => white_balance(v),
                        "0x1001" => sharpness(v),
                        _ => noise_reduction(v),
                    };
                    assert!(
                        decoded.is_some(),
                        "{} {tag}={v:#x} not decoded",
                        case["name"]
                    );
                }
            }
        }
        let case = |name: &str| {
            matrix["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == name)
                .unwrap()["makernote"]
                .clone()
        };
        let tones = case("tone_h40_s-20");
        assert_eq!(tone(tones["0x1041"].as_i64().unwrap()), "+4");
        assert_eq!(tone(tones["0x1040"].as_i64().unwrap()), "−2");
        assert_eq!(
            color(case("color-40_clar-50")["0x1003"].as_i64().unwrap()).as_deref(),
            Some("−4")
        );
        assert_eq!(
            sharpness(case("sharp_20")["0x1001"].as_i64().unwrap()).as_deref(),
            Some("+2")
        );
        assert_eq!(
            sharpness(case("sharp_-40")["0x1001"].as_i64().unwrap()).as_deref(),
            Some("−4")
        );
    }

    #[test]
    fn formats_exposure_values() {
        assert_eq!(format_exposure(1.0 / 250.0), "1/250 s");
        assert_eq!(format_exposure(2.0), "2 s");
        assert_eq!(format_exposure(0.5), "0,5 s");
        assert_eq!(format_bias(1.0 / 3.0), "+0,3 EV");
        assert_eq!(format_bias(-2.0 / 3.0), "−0,7 EV");
        assert_eq!(format_bias(0.0), "0 EV");
    }
}
