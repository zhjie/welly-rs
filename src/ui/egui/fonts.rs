use crate::backend::cell::Cell;
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};
use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::Instant;

#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

#[cfg(target_os = "windows")]
use winreg::RegKey;

pub const CHINESE_FONT_SIZE: f32 = 32.0;
pub const ENGLISH_FONT_SIZE: f32 = 26.0;
pub const CHINESE_LEFT_MARGIN: f32 = 2.0;
pub const CHINESE_TOP_MARGIN: f32 = 2.0;
pub const ENGLISH_LEFT_MARGIN: f32 = 2.0;
pub const ENGLISH_TOP_MARGIN: f32 = 2.0;

/// Font metrics measured from the actual loaded English font at [`ENGLISH_FONT_SIZE`].
/// Used to center capital letters optically in each cell instead of centering the em-square.
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
    /// Distance (logical px) from the top of the text bounding box to the baseline.
    pub ascent_px: f32,
    /// Height (logical px) of capital letters as measured from the 'H' glyph.
    pub cap_height_px: f32,
}

impl FontMetrics {
    /// Vertical y-offset (logical px) that places the optical center of capital letters
    /// at the vertical center of a cell with the given logical height.
    pub fn vertical_center_y_offset(&self, cell_height_logical: f32) -> f32 {
        cell_height_logical / 2.0 - self.ascent_px + self.cap_height_px / 2.0
    }
}

impl Default for FontMetrics {
    fn default() -> Self {
        // Chosen so that vertical_center_y_offset(CELL_HEIGHT) == (CELL_HEIGHT - ENGLISH_FONT_SIZE) / 2,
        // i.e. the same as the old em-center formula with no correction offset.
        Self {
            ascent_px: ENGLISH_FONT_SIZE * 0.75,
            cap_height_px: ENGLISH_FONT_SIZE * 0.50,
        }
    }
}

static ENGLISH_FONT_METRICS: OnceLock<FontMetrics> = OnceLock::new();

pub fn set_english_font_metrics(metrics: FontMetrics) {
    let _ = ENGLISH_FONT_METRICS.set(metrics);
}

pub fn english_font_metrics() -> FontMetrics {
    ENGLISH_FONT_METRICS.get().copied().unwrap_or_default()
}

/// Measure [`FontMetrics`] for the English font from its raw bytes.
/// Uses ab_glyph to read ascent and the bounding box of 'H' at [`ENGLISH_FONT_SIZE`].
pub fn measure_font_metrics(data: &[u8], index: u32) -> FontMetrics {
    let font = match FontRef::try_from_slice_and_index(data, index) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Could not parse font for metrics measurement: {e}");
            return FontMetrics::default();
        }
    };

    let scale = PxScale::from(ENGLISH_FONT_SIZE);
    let scaled = font.as_scaled(scale);
    let ascent_px = scaled.ascent();

    let cap_height_px = {
        let glyph_id = scaled.glyph_id('H');
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
        // outline_glyph() returns None for empty/missing glyphs; px_bounds() gives pixel rect.
        // Pen position is at the baseline (y=0). For 'H':
        //   bounds.min.y < 0  (top of letter, above baseline)
        //   bounds.max.y ≈ 0  (bottom of letter, at or slightly below baseline)
        // We want to center the ACTUAL glyph, so use the midpoint:
        //   glyph_center_from_baseline = (min.y + max.y) / 2
        // Then cap_height_px = -(min.y + max.y) so that the formula
        //   y_offset = cell/2 - ascent + cap_height_px/2  centers the glyph correctly.
        // Using just -min.y (the pure height) would overcenter downward when max.y > 0.
        font.outline_glyph(glyph)
            .map(|outlined| {
                let b = outlined.px_bounds();
                -(b.min.y + b.max.y)
            })
            .filter(|&v| v > 0.0)
            .unwrap_or(ascent_px * 0.7)
    };

    // y_offset = cell_height/2 - ascent_px + cap_height_px/2 (at CELL_HEIGHT = 35.0)
    let y_offset = 35.0_f32 / 2.0 - ascent_px + cap_height_px / 2.0;
    log::info!(
        "English font metrics: ascent={:.2}px cap_height={:.2}px → y_offset={:.2}px \
         (at 35px cell height, old em-center was 4.5px)",
        ascent_px,
        cap_height_px,
        y_offset,
    );
    FontMetrics {
        ascent_px,
        cap_height_px,
    }
}

pub const ENGLISH_FONT_NAME: &str = "welly-english";
pub const CHINESE_FONT_NAME: &str = "welly-chinese";

#[cfg(target_os = "windows")]
const WINDOWS_FONT_REGISTRY_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

pub const ENGLISH_FONT_CANDIDATES: &[FontCandidate] = &[
    FontCandidate {
        egui_name: ENGLISH_FONT_NAME,
        families: &["Monaco"],
    },
    FontCandidate {
        egui_name: ENGLISH_FONT_NAME,
        families: &["Consolas"],
    },
    FontCandidate {
        egui_name: ENGLISH_FONT_NAME,
        families: &["CaskaydiaMono"],
    },
];

pub const CHINESE_FONT_CANDIDATES: &[FontCandidate] = &[
    FontCandidate {
        egui_name: CHINESE_FONT_NAME,
        families: &["Heiti SC"],
    },
    FontCandidate {
        egui_name: CHINESE_FONT_NAME,
        families: &["SimHei"],
    },
    FontCandidate {
        egui_name: CHINESE_FONT_NAME,
        families: &["Noto Sans Mono CJK SC"],
    },
    FontCandidate {
        egui_name: CHINESE_FONT_NAME,
        families: &["Sarasa Mono SC"],
    },
];

#[derive(Clone, Copy)]
pub struct FontCandidate {
    pub egui_name: &'static str,
    pub families: &'static [&'static str],
}

pub struct LoadedFont {
    pub egui_name: &'static str,
    pub family_name: String,
    pub data: Vec<u8>,
    pub index: u32,
    pub metrics: FontMetrics,
}

pub fn font_for_cell(cell: &Cell) -> (&'static str, f32) {
    if cell.ch.is_ascii() {
        (ENGLISH_FONT_NAME, ENGLISH_FONT_SIZE)
    } else {
        (CHINESE_FONT_NAME, CHINESE_FONT_SIZE)
    }
}

pub fn configure_fonts(ctx: &egui::Context) {
    #[cfg(target_os = "windows")]
    {
        configure_fonts_without_blocking_first_frame(ctx);
    }

    #[cfg(not(target_os = "windows"))]
    let started_at = Instant::now();
    #[cfg(not(target_os = "windows"))]
    ctx.set_fonts(build_configured_font_definitions());
    #[cfg(not(target_os = "windows"))]
    log::info!("Configured fonts in {:?}", started_at.elapsed());
}

#[cfg(target_os = "windows")]
fn configure_fonts_without_blocking_first_frame(ctx: &egui::Context) {
    let started_at = Instant::now();
    let (fonts, needs_background_scan) = build_startup_font_definitions();
    ctx.set_fonts(fonts);
    log::info!("Configured startup fonts in {:?}", started_at.elapsed());

    if !needs_background_scan {
        return;
    }

    let background_ctx = ctx.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("font-loader".to_owned())
        .spawn(move || {
            let started_at = Instant::now();
            let fonts = build_configured_font_definitions();
            background_ctx.set_fonts(fonts);
            log::info!(
                "Completed background font discovery in {:?}",
                started_at.elapsed()
            );
            background_ctx.request_repaint();
        })
    {
        log::warn!("Failed to spawn background font loader: {}", error);
        let started_at = Instant::now();
        ctx.set_fonts(build_configured_font_definitions());
        log::info!(
            "Configured fonts synchronously in {:?}",
            started_at.elapsed()
        );
    }
}

fn build_configured_font_definitions() -> FontDefinitions {
    #[cfg(target_os = "windows")]
    {
        let mut english_font = load_font_candidate_fast(ENGLISH_FONT_CANDIDATES);
        let mut chinese_font = load_font_candidate_fast(CHINESE_FONT_CANDIDATES);

        if english_font.is_none() || chinese_font.is_none() {
            let font_db = load_system_font_db();
            if english_font.is_none() {
                english_font = load_font_candidate(&font_db, ENGLISH_FONT_CANDIDATES);
            }
            if chinese_font.is_none() {
                chinese_font = load_font_candidate(&font_db, CHINESE_FONT_CANDIDATES);
            }
        }

        build_font_definitions_from_loaded_fonts(english_font, chinese_font, true)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let font_db = load_system_font_db();
        let english_font = load_font_candidate(&font_db, ENGLISH_FONT_CANDIDATES);
        let chinese_font = load_font_candidate(&font_db, CHINESE_FONT_CANDIDATES);
        build_font_definitions_from_loaded_fonts(english_font, chinese_font, true)
    }
}

#[cfg(target_os = "windows")]
fn build_startup_font_definitions() -> (FontDefinitions, bool) {
    let english_font = load_font_candidate_fast(ENGLISH_FONT_CANDIDATES);
    let chinese_font = load_font_candidate_fast(CHINESE_FONT_CANDIDATES);
    let needs_background_scan = english_font.is_none() || chinese_font.is_none();

    (
        build_font_definitions_from_loaded_fonts(english_font, chinese_font, false),
        needs_background_scan,
    )
}

fn build_font_definitions_from_loaded_fonts(
    english_font: Option<LoadedFont>,
    chinese_font: Option<LoadedFont>,
    log_missing_fonts: bool,
) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let terminal_fallbacks = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let english_font_name = english_font.as_ref().map(|font| font.egui_name);
    let chinese_font_name = chinese_font.as_ref().map(|font| font.egui_name);

    if let Some(loaded) = english_font {
        log::info!("Using English font '{}'", loaded.family_name);
        set_english_font_metrics(loaded.metrics);
        insert_loaded_font(&mut fonts, loaded);
    } else if log_missing_fonts {
        log::warn!("No English candidate fonts found; egui will use fallback families");
    }

    if let Some(loaded) = chinese_font {
        log::info!("Using Chinese font '{}'", loaded.family_name);
        insert_loaded_font(&mut fonts, loaded);
    } else if log_missing_fonts {
        log::warn!("No Chinese candidate fonts found; egui will use fallback families");
    }

    register_font_families(
        &mut fonts,
        english_font_name,
        chinese_font_name,
        &terminal_fallbacks,
    );
    fonts
}

fn insert_loaded_font(fonts: &mut FontDefinitions, loaded: LoadedFont) {
    fonts.font_data.insert(
        loaded.egui_name.to_owned(),
        FontData {
            font: Cow::Owned(loaded.data),
            index: loaded.index,
            tweak: Default::default(),
        },
    );
}

fn register_font_families(
    fonts: &mut FontDefinitions,
    english_font_name: Option<&str>,
    chinese_font_name: Option<&str>,
    terminal_fallbacks: &[String],
) {
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    prepend_font_name(monospace, english_font_name);
    append_font_name(monospace, chinese_font_name);

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    prepend_font_name(proportional, english_font_name);
    append_font_name(proportional, chinese_font_name);

    fonts.families.insert(
        FontFamily::Name(ENGLISH_FONT_NAME.into()),
        family_chain(english_font_name, chinese_font_name, terminal_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Name(CHINESE_FONT_NAME.into()),
        family_chain(chinese_font_name, english_font_name, terminal_fallbacks),
    );
}

fn prepend_font_name(families: &mut Vec<String>, font_name: Option<&str>) {
    let Some(font_name) = font_name else {
        return;
    };

    if families.iter().any(|item| item == font_name) {
        return;
    }

    families.insert(0, font_name.to_owned());
}

fn append_font_name(families: &mut Vec<String>, font_name: Option<&str>) {
    let Some(font_name) = font_name else {
        return;
    };

    if families.iter().any(|item| item == font_name) {
        return;
    }

    families.push(font_name.to_owned());
}

fn family_chain(
    primary: Option<&str>,
    secondary: Option<&str>,
    fallbacks: &[String],
) -> Vec<String> {
    let mut chain = Vec::with_capacity(fallbacks.len() + 2);
    push_unique_font_name(&mut chain, primary);
    push_unique_font_name(&mut chain, secondary);
    for fallback in fallbacks {
        push_unique_font_name(&mut chain, Some(fallback));
    }
    chain
}

fn push_unique_font_name(families: &mut Vec<String>, font_name: Option<&str>) {
    let Some(font_name) = font_name else {
        return;
    };

    if families.iter().all(|item| item != font_name) {
        families.push(font_name.to_owned());
    }
}

pub fn load_system_font_db() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
}

#[cfg(target_os = "windows")]
fn load_font_candidate_fast(candidates: &[FontCandidate]) -> Option<LoadedFont> {
    for candidate in candidates {
        for family in candidate.families {
            if let Some(loaded) = load_windows_font_family(candidate.egui_name, family) {
                return Some(loaded);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn load_windows_font_family(egui_name: &'static str, family: &str) -> Option<LoadedFont> {
    let path = query_windows_font_path(family)?;
    let mut db = fontdb::Database::new();
    db.load_font_file(&path).ok()?;

    let face = db
        .faces()
        .find(|face| face_matches_family(face, family))
        .or_else(|| db.faces().next())?;
    let data = db.with_face_data(face.id, |font_data, _| font_data.to_vec())?;
    let family_name = face
        .families
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| family.to_owned());

    Some(LoadedFont {
        egui_name,
        family_name,
        metrics: measure_font_metrics(&data, face.index),
        data,
        index: face.index,
    })
}

#[cfg(target_os = "windows")]
fn face_matches_family(face: &fontdb::FaceInfo, family: &str) -> bool {
    let normalized_family = normalize_font_name(family);

    face.families.iter().any(|(name, _)| {
        let normalized_name = normalize_font_name(name);
        normalized_name == normalized_family
            || normalized_name.contains(&normalized_family)
            || normalized_family.contains(&normalized_name)
    }) || normalize_font_name(&face.post_script_name).contains(&normalized_family)
}

#[cfg(target_os = "windows")]
fn query_windows_font_path(family: &str) -> Option<PathBuf> {
    let mut best_match: Option<(usize, PathBuf)> = None;

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let Ok(key) = RegKey::predef(hive).open_subkey(WINDOWS_FONT_REGISTRY_KEY) else {
            continue;
        };

        for value_name in key.enum_values().flatten().map(|(name, _)| name) {
            let Some(score) = registry_font_name_score(&value_name, family) else {
                continue;
            };
            let Ok(file_name) = key.get_value::<String, _>(value_name.as_str()) else {
                continue;
            };
            let Some(path) = resolve_windows_font_path(&file_name) else {
                continue;
            };

            match &best_match {
                Some((best_score, _)) if *best_score <= score => {}
                _ => best_match = Some((score, path)),
            }
        }
    }

    best_match.map(|(_, path)| path)
}

#[cfg(target_os = "windows")]
fn registry_font_name_score(value_name: &str, family: &str) -> Option<usize> {
    let normalized_registry_name = normalize_font_name(strip_registry_font_suffix(value_name));
    let normalized_family = normalize_font_name(family);

    if normalized_registry_name == normalized_family {
        return Some(0);
    }

    if normalized_registry_name.starts_with(&normalized_family) {
        return Some(
            1 + normalized_registry_name
                .len()
                .saturating_sub(normalized_family.len()),
        );
    }

    normalized_registry_name
        .contains(&normalized_family)
        .then_some(
            100 + normalized_registry_name
                .len()
                .saturating_sub(normalized_family.len()),
        )
}

#[cfg(target_os = "windows")]
fn strip_registry_font_suffix(value_name: &str) -> &str {
    value_name
        .split_once('(')
        .map(|(name, _)| name.trim())
        .unwrap_or_else(|| value_name.trim())
}

#[cfg(target_os = "windows")]
fn resolve_windows_font_path(file_name: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(file_name);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }

    windows_font_dirs()
        .into_iter()
        .map(|dir| dir.join(file_name))
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn windows_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(3);

    if let Some(system_root) = std::env::var_os("SYSTEMROOT") {
        dirs.push(Path::new(&system_root).join("Fonts"));
    } else {
        dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
    }

    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = Path::new(&home);
        dirs.push(home.join(r"AppData\Local\Microsoft\Windows\Fonts"));
        dirs.push(home.join(r"AppData\Roaming\Microsoft\Windows\Fonts"));
    }

    dirs
}

#[cfg(target_os = "windows")]
fn normalize_font_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

pub fn choose_font_candidate<F>(
    candidates: &[FontCandidate],
    is_available: F,
) -> Option<FontCandidate>
where
    F: Fn(&str) -> bool,
{
    candidates
        .iter()
        .copied()
        .find(|candidate| candidate.families.iter().any(|family| is_available(family)))
}

pub fn load_font_candidate(
    db: &fontdb::Database,
    candidates: &[FontCandidate],
) -> Option<LoadedFont> {
    let candidate =
        choose_font_candidate(candidates, |family| query_font_family(db, family).is_some())?;
    load_candidate_font_data(db, candidate)
}

pub fn load_candidate_font_data(
    db: &fontdb::Database,
    candidate: FontCandidate,
) -> Option<LoadedFont> {
    for family in candidate.families {
        if let Some(id) = query_font_family(db, family) {
            let Some(face) = db.face(id) else {
                continue;
            };
            let Some(data) = db.with_face_data(id, |data, _| data.to_vec()) else {
                continue;
            };
            return Some(LoadedFont {
                egui_name: candidate.egui_name,
                family_name: (*family).to_owned(),
                metrics: measure_font_metrics(&data, face.index),
                data,
                index: face.index,
            });
        }
    }

    None
}

pub fn query_font_family(db: &fontdb::Database, family: &str) -> Option<fontdb::ID> {
    let families = [fontdb::Family::Name(family)];
    db.query(&fontdb::Query {
        families: &families,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::cell::Cell;

    #[test]
    fn font_sizes_follow_welly_default_proportions() {
        assert_eq!(CHINESE_FONT_SIZE, (35.0_f32 * 22.0_f32 / 24.0).round());
        assert_eq!(ENGLISH_FONT_SIZE, (35.0_f32 * 18.0_f32 / 24.0).round());
    }

    #[test]
    fn ascii_cells_use_english_font_even_when_cell_is_wide() {
        let cell = Cell {
            ch: '9',
            width: 2,
            ..Default::default()
        };

        assert_eq!(font_for_cell(&cell), (ENGLISH_FONT_NAME, ENGLISH_FONT_SIZE));
    }

    #[test]
    fn chinese_cells_use_chinese_font() {
        let cell = Cell {
            ch: '在',
            width: 2,
            ..Default::default()
        };

        assert_eq!(font_for_cell(&cell), (CHINESE_FONT_NAME, CHINESE_FONT_SIZE));
    }

    #[test]
    fn choose_font_candidate_returns_first_available_candidate() {
        let candidates = [
            FontCandidate {
                egui_name: "missing",
                families: &["Missing Font"],
            },
            FontCandidate {
                egui_name: "available",
                families: &["Available Font"],
            },
        ];
        let installed_families = ["Available Font"];

        let chosen =
            choose_font_candidate(&candidates, |family| installed_families.contains(&family));

        assert_eq!(chosen.unwrap().egui_name, "available");
    }

    #[test]
    fn chinese_font_candidates_prefer_heiti_sc() {
        assert_eq!(CHINESE_FONT_CANDIDATES[0].families, &["Heiti SC"]);
    }

    #[test]
    fn chinese_font_candidates_use_shared_heiti_order() {
        let families: Vec<&str> = CHINESE_FONT_CANDIDATES
            .iter()
            .flat_map(|candidate| candidate.families)
            .copied()
            .collect();

        assert_eq!(
            families,
            vec![
                "Heiti SC",
                "SimHei",
                "Noto Sans Mono CJK SC",
                "Sarasa Mono SC"
            ]
        );
    }

    #[test]
    fn english_font_candidates_use_shared_monospace_order() {
        let families: Vec<&str> = ENGLISH_FONT_CANDIDATES
            .iter()
            .flat_map(|candidate| candidate.families)
            .copied()
            .collect();

        assert_eq!(families, vec!["Monaco", "Cascadia Mono", "CaskaydiaMono"]);
    }

    #[test]
    fn english_font_candidates_do_not_include_consolas() {
        assert!(!ENGLISH_FONT_CANDIDATES
            .iter()
            .flat_map(|candidate| candidate.families)
            .any(|family| *family == "Consolas"));
    }

    #[test]
    fn named_font_families_fallback_to_monospace_before_custom_fonts_load() {
        let fonts = build_font_definitions_from_loaded_fonts(None, None, false);
        let default_monospace_fonts = fonts.families.get(&FontFamily::Monospace).unwrap();

        assert_eq!(
            fonts
                .families
                .get(&FontFamily::Name(ENGLISH_FONT_NAME.into()))
                .unwrap(),
            default_monospace_fonts
        );
        assert_eq!(
            fonts
                .families
                .get(&FontFamily::Name(CHINESE_FONT_NAME.into()))
                .unwrap(),
            default_monospace_fonts
        );
    }

    #[test]
    fn named_font_families_reference_known_font_data_keys() {
        let fonts = build_font_definitions_from_loaded_fonts(None, None, false);

        for family in [
            FontFamily::Name(ENGLISH_FONT_NAME.into()),
            FontFamily::Name(CHINESE_FONT_NAME.into()),
        ] {
            for font_name in fonts.families.get(&family).unwrap() {
                assert!(
                    fonts.font_data.contains_key(font_name),
                    "{family:?} references missing font data {font_name:?}"
                );
            }
        }
    }

    #[test]
    fn family_chain_appends_real_fallback_font_names() {
        let fallbacks = vec!["fallback-a".to_owned(), "fallback-b".to_owned()];

        assert_eq!(family_chain(None, None, &fallbacks), fallbacks);
        assert_eq!(
            family_chain(Some(ENGLISH_FONT_NAME), Some("fallback-a"), &fallbacks),
            vec![
                ENGLISH_FONT_NAME.to_owned(),
                "fallback-a".to_owned(),
                "fallback-b".to_owned()
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn registry_font_name_score_ignores_registry_suffixes() {
        assert_eq!(
            registry_font_name_score("Cascadia Mono (TrueType)", "Cascadia Mono"),
            Some(0)
        );
        assert!(
            registry_font_name_score("SimHei & Microsoft YaHei UI (TrueType)", "SimHei").is_some()
        );
    }
}
