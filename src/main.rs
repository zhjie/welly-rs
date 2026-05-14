#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use eframe::egui;
use std::sync::{Arc, OnceLock};

const APP_ICON_RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/welly-rs-app-icon.rgba"));

mod app;
mod backend;
mod config;
mod ui;

use app::App;
use ui::egui::fonts::configure_fonts;
use ui::egui::render::{CELL_HEIGHT, CELL_WIDTH, MIN_ZOOM, TERMINAL_COLS, TERMINAL_ROWS};

fn app_icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        Arc::new(
            decode_rgba_icon(APP_ICON_RGBA).unwrap_or_else(|| egui::IconData {
                rgba: Vec::new(),
                width: 0,
                height: 0,
            }),
        )
    })
    .clone()
}

fn decode_rgba_icon(bytes: &[u8]) -> Option<egui::IconData> {
    if bytes.len() < 8 {
        return None;
    }

    let width = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[4..8].try_into().ok()?);
    if width == 0 || height == 0 || width != height {
        return None;
    }
    let rgba = bytes[8..].to_vec();
    if rgba.len() != width as usize * height as usize * 4 {
        return None;
    }

    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([
                TERMINAL_COLS as f32 * CELL_WIDTH,
                TERMINAL_ROWS as f32 * CELL_HEIGHT,
            ])
            .with_min_inner_size([
                TERMINAL_COLS as f32 * CELL_WIDTH * MIN_ZOOM,
                TERMINAL_ROWS as f32 * CELL_HEIGHT * MIN_ZOOM,
            ])
            .with_icon(app_icon()),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Welly-rs BBS Client",
        options,
        Box::new(|cc| {
            configure_fonts(&cc.egui_ctx);
            configure_terminal_view(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)))
        }),
    )
}

fn configure_terminal_view(ctx: &egui::Context) {
    ctx.options_mut(|options| {
        options.zoom_with_keyboard = false;
    });
    ctx.set_zoom_factor(1.0);
}
