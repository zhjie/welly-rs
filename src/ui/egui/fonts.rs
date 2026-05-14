use crate::backend::cell::Cell;
use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};
use std::borrow::Cow;

pub const CHINESE_FONT_SIZE: f32 = 32.0;
pub const ENGLISH_FONT_SIZE: f32 = 26.0;
pub const CHINESE_LEFT_MARGIN: f32 = 1.0;
pub const CHINESE_TOP_MARGIN: f32 = 1.0;
pub const ENGLISH_LEFT_MARGIN: f32 = 1.0;
pub const ENGLISH_TOP_MARGIN: f32 = 1.0;

pub const ENGLISH_FONT_NAME: &str = "welly-english";
pub const CHINESE_FONT_NAME: &str = "welly-chinese";

pub const ENGLISH_FONT_CANDIDATES: &[FontCandidate] = &[
    FontCandidate {
        egui_name: ENGLISH_FONT_NAME,
        families: &["Monaco"],
    },
    FontCandidate {
        egui_name: ENGLISH_FONT_NAME,
        families: &["Cascadia Mono"],
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
}

pub fn font_for_cell(cell: &Cell) -> (&'static str, f32) {
    if cell.ch.is_ascii() {
        (ENGLISH_FONT_NAME, ENGLISH_FONT_SIZE)
    } else {
        (CHINESE_FONT_NAME, CHINESE_FONT_SIZE)
    }
}

pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let font_db = load_system_font_db();

    let english_font = load_font_candidate(&font_db, ENGLISH_FONT_CANDIDATES);
    let chinese_font = load_font_candidate(&font_db, CHINESE_FONT_CANDIDATES);

    if let Some(loaded) = &english_font {
        fonts.font_data.insert(
            loaded.egui_name.to_owned(),
            FontData {
                font: Cow::Owned(loaded.data.clone()),
                index: loaded.index,
                tweak: Default::default(),
            },
        );
        log::info!("Using English font '{}'", loaded.family_name);
    } else {
        log::warn!("No English candidate fonts found; egui will use fallback families");
    }

    if let Some(loaded) = &chinese_font {
        fonts.font_data.insert(
            loaded.egui_name.to_owned(),
            FontData {
                font: Cow::Owned(loaded.data.clone()),
                index: loaded.index,
                tweak: Default::default(),
            },
        );
        log::info!("Using Chinese font '{}'", loaded.family_name);
    } else {
        log::warn!("No Chinese candidate fonts found; egui will use fallback families");
    }

    let english_family = english_font
        .as_ref()
        .map(|font| font.egui_name)
        .unwrap_or("Monospace");
    let chinese_family = chinese_font
        .as_ref()
        .map(|font| font.egui_name)
        .unwrap_or("Monospace");

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, english_family.to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push(chinese_family.to_owned());

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, english_family.to_owned());
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push(chinese_family.to_owned());

    fonts.families.insert(
        FontFamily::Name(ENGLISH_FONT_NAME.into()),
        vec![english_family.to_owned(), chinese_family.to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name(CHINESE_FONT_NAME.into()),
        vec![chinese_family.to_owned(), english_family.to_owned()],
    );

    ctx.set_fonts(fonts);
}

pub fn load_system_font_db() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
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
}
