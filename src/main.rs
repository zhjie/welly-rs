#![allow(clippy::items_after_test_module)]

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};
use std::process::Command;
use std::sync::{Arc, Mutex};

const CELL_WIDTH: f32 = 18.0;
const CELL_HEIGHT: f32 = 35.0;
const CHINESE_FONT_SIZE: f32 = 32.0;
const ENGLISH_FONT_SIZE: f32 = 26.0;
const CHINESE_LEFT_MARGIN: f32 = 1.0;
const CHINESE_TOP_MARGIN: f32 = 1.0;
const ENGLISH_LEFT_MARGIN: f32 = 1.0;
const ENGLISH_TOP_MARGIN: f32 = 1.0;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 3.0;
const ZOOM_STEP: f32 = 1.05;
const TERMINAL_COLS: usize = 80;
const TERMINAL_ROWS: usize = 24;

const ENGLISH_FONT_PATH: &str = "/System/Library/Fonts/Monaco.ttf";
const ENGLISH_FONT_NAME: &str = "monaco";

const CHINESE_FONT_PATH: &str = "/System/Library/Fonts/STHeiti Medium.ttc";
const CHINESE_FONT_NAME: &str = "stheiti";

mod ansi_parser;
mod attachment;
mod cell;
mod config;
mod ssh;
mod terminal;

use ansi_parser::AnsiParser;
use attachment::{parse_image_attachments, ImageAttachment};
use config::ConnectionSettings;
use encoding_rs::GB18030;
use ssh::{is_channel_closed_error, SshClient};
use terminal::Terminal;

type ConnectResult = Result<Arc<SshClient>, String>;
type ConnectSender = Sender<ConnectResult>;
type ConnectReceiver = Receiver<ConnectResult>;

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
            ]),
        ..Default::default()
    };

    eframe::run_native(
        "Welly-rs BBS Client",
        options,
        Box::new(|cc| {
            configure_fonts(&cc.egui_ctx);
            configure_terminal_view(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    if let Ok(english_font) = std::fs::read(ENGLISH_FONT_PATH) {
        fonts.font_data.insert(
            ENGLISH_FONT_NAME.to_owned(),
            FontData::from_owned(english_font),
        );
    } else {
        log::warn!("English font not found: {}", ENGLISH_FONT_PATH);
    }

    if let Ok(chinese_font) = std::fs::read(CHINESE_FONT_PATH) {
        fonts.font_data.insert(
            CHINESE_FONT_NAME.to_owned(),
            FontData::from_owned(chinese_font),
        );
    } else {
        log::warn!("Chinese font not found: {}", CHINESE_FONT_PATH);
    }

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, ENGLISH_FONT_NAME.to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push(CHINESE_FONT_NAME.to_owned());

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, ENGLISH_FONT_NAME.to_owned());
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push(CHINESE_FONT_NAME.to_owned());

    fonts.families.insert(
        FontFamily::Name(ENGLISH_FONT_NAME.into()),
        vec![ENGLISH_FONT_NAME.to_owned(), CHINESE_FONT_NAME.to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name(CHINESE_FONT_NAME.into()),
        vec![CHINESE_FONT_NAME.to_owned(), ENGLISH_FONT_NAME.to_owned()],
    );

    ctx.set_fonts(fonts);
}

fn configure_terminal_view(ctx: &egui::Context) {
    ctx.options_mut(|options| {
        options.zoom_with_keyboard = false;
    });
    ctx.set_zoom_factor(1.0);
}

struct App {
    terminal: Arc<Mutex<Terminal>>,
    parser: Arc<Mutex<AnsiParser>>,
    ssh_client: Option<Arc<SshClient>>,
    connect_rx: Option<ConnectReceiver>,
    connected: bool,
    settings: ConnectionSettings,
    login_host: String,
    login_port: String,
    login_username: String,
    login_password: String,
    connection_error: Option<String>,
    auto_connect_attempted: bool,
    zoom: f32,
    selection: Option<Selection>,
    pending_inner_size: Option<egui::Vec2>,
    last_inner_size: Option<egui::Vec2>,
    configured_viewport: bool,
}

impl Default for App {
    fn default() -> Self {
        let settings = ConnectionSettings::load_default();
        let login_host = settings.host.clone();
        let login_port = settings.port.to_string();
        let login_username = settings.username.clone().unwrap_or_default();

        Self {
            terminal: Arc::new(Mutex::new(Terminal::new(TERMINAL_ROWS, TERMINAL_COLS))),
            parser: Arc::new(Mutex::new(AnsiParser::new())),
            ssh_client: None,
            connect_rx: None,
            connected: false,
            settings,
            login_host,
            login_port,
            login_username,
            login_password: String::new(),
            connection_error: None,
            auto_connect_attempted: false,
            zoom: 1.0,
            selection: None,
            pending_inner_size: None,
            last_inner_size: None,
            configured_viewport: false,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.connected
            && self.connect_rx.is_none()
            && self.settings.username.is_some()
            && !self.auto_connect_attempted
        {
            self.start_connect(ctx);
        }
        self.configure_viewport_once(ctx);

        if let Some(rx) = &self.connect_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(client) => {
                        self.ssh_client = Some(client);
                        self.connected = true;
                    }
                    Err(e) => {
                        log::error!("SSH error: {}", e);
                        self.connected = false;
                        self.connection_error = Some(e);
                    }
                }
                self.connect_rx = None;
            }
        }

        self.handle_keyboard(ctx);
        self.sync_window_size_to_terminal(ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::BLACK)
                    .inner_margin(0.0),
            )
            .show(ctx, |ui| {
                let terminal_response = render_terminal(ui, &self.terminal, self.selection);
                self.handle_terminal_selection(ctx, &terminal_response);
                self.render_attachment_button(ui);
                if !self.connected
                    && self.connect_rx.is_none()
                    && (self.settings.username.is_none() || self.connection_error.is_some())
                {
                    self.render_login(ui, ctx);
                }
            });
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::BLACK.to_normalized_gamma_f32()
    }
}

impl App {
    fn configure_viewport_once(&mut self, ctx: &egui::Context) {
        if self.configured_viewport {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::ResizeIncrements(Some(egui::vec2(
            CELL_WIDTH,
            CELL_HEIGHT,
        ))));
        self.configured_viewport = true;
    }

    fn start_connect(&mut self, ctx: &egui::Context) {
        self.connected = false;
        self.auto_connect_attempted = true;
        self.ssh_client = None;
        self.terminal.lock().unwrap().clear_all();
        self.parser = Arc::new(Mutex::new(AnsiParser::new()));

        let terminal = Arc::clone(&self.terminal);
        let parser = Arc::clone(&self.parser);
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        let (tx, rx): (ConnectSender, ConnectReceiver) = crossbeam_channel::bounded(1);
        self.connect_rx = Some(rx);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match SshClient::connect(settings, terminal, parser, ctx).await {
                    Ok(client) => {
                        log::info!("SSH connected successfully");
                        let _ = tx.send(Ok(client));
                        std::future::pending::<()>().await;
                    }
                    Err(e) => {
                        log::error!("SSH error: {}", e);
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            });
        });
    }

    fn reconnect(&mut self, ctx: &egui::Context) {
        self.start_connect(ctx);
    }

    fn render_login(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(220));

        egui::Area::new("login_panel".into())
            .fixed_pos(rect.center() - egui::vec2(180.0, 110.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(20, 20, 20))
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.heading("登录配置");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Host");
                            ui.text_edit_singleline(&mut self.login_host);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Port");
                            ui.text_edit_singleline(&mut self.login_port);
                        });
                        ui.horizontal(|ui| {
                            ui.label("User");
                            ui.text_edit_singleline(&mut self.login_username);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Pass");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.login_password).password(true),
                            );
                        });

                        if let Some(error) = &self.connection_error {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                        }

                        ui.add_space(8.0);
                        if ui.button("Connect").clicked() {
                            if let Ok(port) = self.login_port.parse() {
                                self.settings = ConnectionSettings {
                                    host: self.login_host.trim().to_owned(),
                                    port,
                                    username: if self.login_username.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.login_username.trim().to_owned())
                                    },
                                    password: if self.login_password.is_empty() {
                                        None
                                    } else {
                                        Some(self.login_password.clone())
                                    },
                                    identity_files: self.settings.identity_files.clone(),
                                };
                                self.connection_error = None;
                                if self.settings.username.is_some() {
                                    self.auto_connect_attempted = false;
                                    self.start_connect(ctx);
                                }
                            } else {
                                self.connection_error = Some("Invalid port".to_owned());
                            }
                        }
                    });
            });
    }

    fn render_attachment_button(&self, ui: &mut egui::Ui) {
        let attachments = {
            let terminal = self.terminal.lock().unwrap();
            parse_image_attachments(&terminal_screen_text(&terminal))
        };
        let Some(first_attachment) = attachments.first() else {
            return;
        };

        let label = attachment_button_label(first_attachment, attachments.len());
        egui::Area::new("image_attachment_button".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_black_alpha(210))
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        if ui.button(label).clicked() {
                            open_image_attachments(&attachments);
                        }
                    });
            });
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Copy => {
                    self.copy_selection(ctx);
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.command && key == egui::Key::C && self.copy_selection(ctx) {
                        continue;
                    }

                    if modifiers.command && key == egui::Key::R {
                        self.selection = None;
                        self.reconnect(ctx);
                        continue;
                    }

                    if handle_zoom_shortcut(&mut self.zoom, key, modifiers) {
                        self.selection = None;
                        self.pending_inner_size = Some(terminal_size_for_zoom(self.zoom));
                        continue;
                    }

                    if let Some(bytes) = terminal_event_to_bytes(&event) {
                        self.selection = None;
                        self.send_bytes(bytes);
                    }
                }
                egui::Event::Text(_) | egui::Event::Ime(_) => {
                    if let Some(bytes) = terminal_event_to_bytes(&event) {
                        self.selection = None;
                        self.send_bytes(bytes);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_terminal_selection(
        &mut self,
        ctx: &egui::Context,
        terminal_response: &TerminalResponse,
    ) {
        if terminal_response.response.drag_started() {
            if let Some(point) = terminal_response.interact_grid_point() {
                self.selection = Some(Selection::new(point));
            }
        } else if terminal_response.response.dragged() {
            if let (Some(selection), Some(point)) =
                (&mut self.selection, terminal_response.interact_grid_point())
            {
                selection.end = point;
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::C) && input.modifiers.command) {
            self.copy_selection(ctx);
        }
    }

    fn copy_selection(&self, ctx: &egui::Context) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };

        let text = {
            let terminal = self.terminal.lock().unwrap();
            selected_text(&terminal, selection)
        };

        if text.is_empty() {
            return false;
        }

        ctx.copy_text(text);
        true
    }

    fn send_bytes(&self, bytes: Vec<u8>) {
        if let Some(client) = &self.ssh_client {
            if !client.is_connected() {
                return;
            }

            let client = Arc::clone(client);
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = client.send_data(&bytes).await {
                        if is_channel_closed_error(&e) {
                            log::debug!("Ignoring send after SSH channel ended: {}", e);
                        } else {
                            log::error!("Send error: {}", e);
                        }
                    }
                });
            });
        }
    }

    fn sync_window_size_to_terminal(&mut self, ctx: &egui::Context) {
        let current_size = ctx.input(|i| i.viewport().inner_rect.map(|rect| rect.size()));
        let Some(current_size) = current_size else {
            return;
        };

        let user_resized = self
            .last_inner_size
            .map(|last| (last - current_size).length_sq() > 1.0)
            .unwrap_or(false);
        self.last_inner_size = Some(current_size);

        let target_size = if let Some(pending_size) = self.pending_inner_size {
            pending_size
        } else if user_resized {
            terminal_aspect_fit_size(current_size)
        } else {
            return;
        };

        if (current_size - target_size).length_sq() > 1.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size));
        } else {
            self.pending_inner_size = None;
            self.last_inner_size = Some(target_size);
        }
    }
}

fn attachment_button_label(attachment: &ImageAttachment, count: usize) -> String {
    if count > 1 {
        format!("打开 {count} 张图")
    } else {
        format!("打开 {}", attachment.filename)
    }
}

fn open_image_attachments(attachments: &[ImageAttachment]) {
    for attachment in attachments {
        if let Err(error) = Command::new("open").arg(&attachment.image_url).spawn() {
            log::error!(
                "Failed to open image attachment {}: {}",
                attachment.image_url,
                error
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridPoint {
    row: usize,
    col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    start: GridPoint,
    end: GridPoint,
}

impl Selection {
    fn new(point: GridPoint) -> Self {
        Self {
            start: point,
            end: point,
        }
    }

    fn normalized(self) -> (GridPoint, GridPoint) {
        if grid_index(self.start) <= grid_index(self.end) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

fn grid_index(point: GridPoint) -> usize {
    point.row * TERMINAL_COLS + point.col
}

struct TerminalResponse {
    response: egui::Response,
    rect: egui::Rect,
    cell_width: f32,
    cell_height: f32,
    rows: usize,
    cols: usize,
}

impl TerminalResponse {
    fn interact_grid_point(&self) -> Option<GridPoint> {
        let pos = self.response.interact_pointer_pos()?;
        pos_to_grid_point(
            pos,
            self.rect,
            self.cell_width,
            self.cell_height,
            self.rows,
            self.cols,
        )
    }
}

fn pos_to_grid_point(
    pos: egui::Pos2,
    rect: egui::Rect,
    cell_width: f32,
    cell_height: f32,
    rows: usize,
    cols: usize,
) -> Option<GridPoint> {
    if !rect.contains(pos) {
        return None;
    }

    let col = ((pos.x - rect.min.x) / cell_width).floor() as usize;
    let row = ((pos.y - rect.min.y) / cell_height).floor() as usize;
    Some(GridPoint {
        row: row.min(rows.saturating_sub(1)),
        col: col.min(cols.saturating_sub(1)),
    })
}

fn selected_text(term: &Terminal, selection: Selection) -> String {
    let (start, end) = selection.normalized();
    let mut lines = Vec::new();

    for row in start.row..=end.row {
        let start_col = if row == start.row { start.col } else { 0 };
        let end_col = if row == end.row {
            end.col
        } else {
            term.cols.saturating_sub(1)
        };

        let mut line = String::new();
        for col in start_col..=end_col.min(term.cols.saturating_sub(1)) {
            let cell = &term.grid[row][col];
            if cell.width == 0 {
                continue;
            }
            line.push(cell.ch);
        }
        lines.push(line.trim_end().to_owned());
    }

    lines.join("\n")
}

fn terminal_screen_text(term: &Terminal) -> String {
    let selection = Selection {
        start: GridPoint { row: 0, col: 0 },
        end: GridPoint {
            row: term.rows.saturating_sub(1),
            col: term.cols.saturating_sub(1),
        },
    };
    selected_text(term, selection)
}

fn terminal_event_to_bytes(event: &egui::Event) -> Option<Vec<u8>> {
    match event {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => key_event_to_bytes(*key, *modifiers),
        egui::Event::Text(text) => text_to_bytes(text),
        egui::Event::Ime(egui::ImeEvent::Commit(text)) => text_to_bytes(text),
        _ => None,
    }
}

fn text_to_bytes(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() {
        None
    } else {
        let (bytes, _, _) = GB18030.encode(text);
        Some(bytes.into_owned())
    }
}

fn key_event_to_bytes(key: egui::Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
    if modifiers.command {
        return None;
    }

    if modifiers.ctrl && !modifiers.alt {
        return control_key_to_bytes(key);
    }

    if modifiers.alt {
        return alt_key_to_bytes(key);
    }

    match key {
        egui::Key::Enter => Some(vec![b'\r']),
        egui::Key::Backspace => Some(vec![0x7f]),
        egui::Key::Delete => Some(b"\x1b[3~".to_vec()),
        egui::Key::Tab => Some(vec![b'\t']),
        egui::Key::Escape => Some(vec![0x1b]),
        egui::Key::ArrowUp => Some(b"\x1b[A".to_vec()),
        egui::Key::ArrowDown => Some(b"\x1b[B".to_vec()),
        egui::Key::ArrowRight => Some(b"\x1b[C".to_vec()),
        egui::Key::ArrowLeft => Some(b"\x1b[D".to_vec()),
        egui::Key::Home => Some(b"\x1b[1~".to_vec()),
        egui::Key::End => Some(b"\x1b[4~".to_vec()),
        egui::Key::PageUp => Some(b"\x1b[5~".to_vec()),
        egui::Key::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}

fn control_key_to_bytes(key: egui::Key) -> Option<Vec<u8>> {
    let byte = match key {
        egui::Key::A => 0x01,
        egui::Key::B => 0x02,
        egui::Key::C => 0x03,
        egui::Key::D => 0x04,
        egui::Key::E => 0x05,
        egui::Key::F => 0x06,
        egui::Key::G => 0x07,
        egui::Key::H | egui::Key::Backspace => 0x08,
        egui::Key::I | egui::Key::Tab => 0x09,
        egui::Key::J => 0x0a,
        egui::Key::K => 0x0b,
        egui::Key::L => 0x0c,
        egui::Key::M | egui::Key::Enter => 0x0d,
        egui::Key::N => 0x0e,
        egui::Key::O => 0x0f,
        egui::Key::P => 0x10,
        egui::Key::Q => 0x11,
        egui::Key::R => 0x12,
        egui::Key::S => 0x13,
        egui::Key::T => 0x14,
        egui::Key::U => 0x15,
        egui::Key::V => 0x16,
        egui::Key::W => 0x17,
        egui::Key::X => 0x18,
        egui::Key::Y => 0x19,
        egui::Key::Z => 0x1a,
        egui::Key::OpenBracket | egui::Key::Escape => 0x1b,
        egui::Key::Backslash => 0x1c,
        egui::Key::CloseBracket => 0x1d,
        egui::Key::Num6 => 0x1e,
        egui::Key::Minus => 0x1f,
        _ => return None,
    };
    Some(vec![byte])
}

fn alt_key_to_bytes(key: egui::Key) -> Option<Vec<u8>> {
    match key {
        egui::Key::ArrowUp => Some(b"\x1b[5~".to_vec()),
        egui::Key::ArrowDown => Some(b"\x1b[6~".to_vec()),
        egui::Key::ArrowRight => Some(b"\x1b[4~".to_vec()),
        egui::Key::ArrowLeft => Some(b"\x1b[1~".to_vec()),
        _ => None,
    }
}

fn handle_zoom_shortcut(zoom: &mut f32, key: egui::Key, modifiers: egui::Modifiers) -> bool {
    if !modifiers.command || modifiers.alt || modifiers.ctrl {
        return false;
    }

    match key {
        egui::Key::Plus | egui::Key::Equals => {
            *zoom = (*zoom * ZOOM_STEP).min(MAX_ZOOM);
            true
        }
        egui::Key::Minus => {
            *zoom = (*zoom / ZOOM_STEP).max(MIN_ZOOM);
            true
        }
        egui::Key::Num0 => {
            *zoom = 1.0;
            true
        }
        _ => false,
    }
}

fn terminal_size_for_zoom(zoom: f32) -> egui::Vec2 {
    egui::vec2(
        TERMINAL_COLS as f32 * CELL_WIDTH * zoom,
        TERMINAL_ROWS as f32 * CELL_HEIGHT * zoom,
    )
}

fn terminal_aspect_fit_size(size: egui::Vec2) -> egui::Vec2 {
    let ratio = (TERMINAL_COLS as f32 * CELL_WIDTH) / (TERMINAL_ROWS as f32 * CELL_HEIGHT);
    let height_from_width = size.x / ratio;

    if height_from_width <= size.y {
        egui::vec2(size.x, height_from_width)
    } else {
        egui::vec2(size.y * ratio, size.y)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        handle_zoom_shortcut, key_event_to_bytes, selected_text, terminal_aspect_fit_size,
        terminal_event_to_bytes, terminal_render_scale, terminal_size_for_zoom, GridPoint,
        Selection, TERMINAL_COLS, TERMINAL_ROWS,
    };
    use crate::terminal::Terminal;

    #[test]
    fn control_letter_sends_ascii_control_code() {
        assert_eq!(
            key_event_to_bytes(egui::Key::G, egui::Modifiers::CTRL),
            Some(vec![0x07])
        );
        assert_eq!(
            key_event_to_bytes(egui::Key::Enter, egui::Modifiers::CTRL),
            Some(vec![0x0d])
        );
    }

    #[test]
    fn alt_arrows_match_welly_navigation_shortcuts() {
        assert_eq!(
            key_event_to_bytes(egui::Key::ArrowUp, egui::Modifiers::ALT),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            key_event_to_bytes(egui::Key::ArrowDown, egui::Modifiers::ALT),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            key_event_to_bytes(egui::Key::ArrowRight, egui::Modifiers::ALT),
            Some(b"\x1b[4~".to_vec())
        );
        assert_eq!(
            key_event_to_bytes(egui::Key::ArrowLeft, egui::Modifiers::ALT),
            Some(b"\x1b[1~".to_vec())
        );
    }

    #[test]
    fn command_shortcuts_are_not_sent_to_bbs() {
        assert_eq!(
            key_event_to_bytes(egui::Key::G, egui::Modifiers::COMMAND),
            None
        );
    }

    #[test]
    fn command_plus_minus_adjust_zoom_without_sending_to_bbs() {
        let mut zoom = 1.0;

        assert!(handle_zoom_shortcut(
            &mut zoom,
            egui::Key::Plus,
            egui::Modifiers::COMMAND
        ));
        assert!(zoom > 1.0);

        assert!(handle_zoom_shortcut(
            &mut zoom,
            egui::Key::Minus,
            egui::Modifiers::COMMAND
        ));
        assert!((zoom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn terminal_render_scale_tracks_available_size() {
        let base_width = TERMINAL_COLS as f32 * super::CELL_WIDTH;
        let base_height = TERMINAL_ROWS as f32 * super::CELL_HEIGHT;

        assert!(
            (terminal_render_scale(base_width, base_height, TERMINAL_COLS, TERMINAL_ROWS) - 1.0)
                .abs()
                < f32::EPSILON
        );
        assert!(
            (terminal_render_scale(
                base_width * 2.0,
                base_height * 2.0,
                TERMINAL_COLS,
                TERMINAL_ROWS
            ) - 2.0)
                .abs()
                < f32::EPSILON
        );
        assert_eq!(
            terminal_render_scale(1.0, 1.0, TERMINAL_COLS, TERMINAL_ROWS),
            super::MIN_ZOOM
        );
    }

    #[test]
    fn terminal_aspect_fit_size_preserves_terminal_ratio() {
        let wide = terminal_aspect_fit_size(egui::vec2(3000.0, 840.0));
        let tall = terminal_aspect_fit_size(egui::vec2(1440.0, 2000.0));
        let expected_ratio = terminal_size_for_zoom(1.0).x / terminal_size_for_zoom(1.0).y;

        assert!((wide.x / wide.y - expected_ratio).abs() < 0.001);
        assert_eq!(wide.y, 840.0);
        assert!((tall.x / tall.y - expected_ratio).abs() < 0.001);
        assert_eq!(tall.x, 1440.0);
    }

    #[test]
    fn terminal_size_for_zoom_scales_full_window() {
        assert_eq!(
            terminal_size_for_zoom(2.0),
            egui::vec2(
                TERMINAL_COLS as f32 * super::CELL_WIDTH * 2.0,
                TERMINAL_ROWS as f32 * super::CELL_HEIGHT * 2.0
            )
        );
    }

    #[test]
    fn attachment_button_label_opens_all_detected_images() {
        let attachment = crate::attachment::ImageAttachment {
            filename: "first.png".to_owned(),
            size: "12 KB".to_owned(),
            image_url: "https://www.newsmth.net/att.php?first.png".to_owned(),
            article_url: None,
        };

        assert_eq!(
            super::attachment_button_label(&attachment, 3),
            "打开 3 张图"
        );
    }

    #[test]
    fn ime_commit_sends_committed_text() {
        let event = egui::Event::Ime(egui::ImeEvent::Commit("中文".to_owned()));

        assert_eq!(
            terminal_event_to_bytes(&event),
            Some(vec![0xd6, 0xd0, 0xce, 0xc4])
        );
    }

    #[test]
    fn font_sizes_follow_welly_default_proportions() {
        assert_eq!(
            super::CHINESE_FONT_SIZE,
            (super::CELL_HEIGHT * 22.0_f32 / 24.0).round()
        );
        assert_eq!(
            super::ENGLISH_FONT_SIZE,
            (super::CELL_HEIGHT * 18.0_f32 / 24.0).round()
        );
    }

    #[test]
    fn text_position_uses_cell_top_margin() {
        let chinese = crate::cell::Cell {
            ch: '在',
            width: 2,
            ..Default::default()
        };
        let english = crate::cell::Cell {
            ch: '9',
            width: 1,
            ..Default::default()
        };

        assert_eq!(
            super::text_paint_position(10.0, 20.0, 1.0, &chinese).y,
            20.0 + super::CHINESE_TOP_MARGIN
        );
        assert_eq!(
            super::text_paint_position(10.0, 20.0, 1.0, &english).y,
            20.0 + super::ENGLISH_TOP_MARGIN
        );
    }

    #[test]
    fn default_colors_reverse_to_visible_black_on_light_background() {
        let cell = crate::cell::Cell {
            reverse: true,
            ..Default::default()
        };

        assert_eq!(super::cell_foreground_color(&cell), egui::Color32::BLACK);
        assert_eq!(
            super::cell_background_color(&cell),
            egui::Color32::from_rgb(229, 229, 229)
        );
    }

    #[test]
    fn selection_extracts_single_line_text() {
        let mut terminal = Terminal::new(2, 8);
        put_ascii(&mut terminal, 0, 0, "hello");

        let text = selected_text(
            &terminal,
            Selection {
                start: GridPoint { row: 0, col: 1 },
                end: GridPoint { row: 0, col: 3 },
            },
        );

        assert_eq!(text, "ell");
    }

    #[test]
    fn selection_extracts_multiline_text_and_trims_right_spaces() {
        let mut terminal = Terminal::new(3, 8);
        put_ascii(&mut terminal, 0, 0, "ab  ");
        put_ascii(&mut terminal, 1, 0, "cd  ");

        let text = selected_text(
            &terminal,
            Selection {
                start: GridPoint { row: 0, col: 0 },
                end: GridPoint { row: 1, col: 3 },
            },
        );

        assert_eq!(text, "ab\ncd");
    }

    #[test]
    fn selection_skips_double_width_continuation_cells() {
        let mut terminal = Terminal::new(1, 8);
        terminal.set_cursor(0, 0);
        terminal.put_char('中');
        terminal.put_char('A');

        let text = selected_text(
            &terminal,
            Selection {
                start: GridPoint { row: 0, col: 0 },
                end: GridPoint { row: 0, col: 2 },
            },
        );

        assert_eq!(text, "中A");
    }

    fn put_ascii(terminal: &mut Terminal, row: usize, col: usize, text: &str) {
        terminal.set_cursor(row, col);
        for ch in text.chars() {
            terminal.put_char(ch);
        }
    }
}

fn render_terminal(
    ui: &mut egui::Ui,
    terminal: &Arc<Mutex<Terminal>>,
    selection: Option<Selection>,
) -> TerminalResponse {
    let term = terminal.lock().unwrap();
    let available_size = ui.available_size();
    let render_scale =
        terminal_render_scale(available_size.x, available_size.y, term.cols, term.rows);
    let cell_width = CELL_WIDTH * render_scale;
    let cell_height = CELL_HEIGHT * render_scale;
    let total_width = term.cols as f32 * cell_width;
    let total_height = term.rows as f32 * cell_height;

    let (response, painter) = ui.allocate_painter(available_size, egui::Sense::click_and_drag());
    if response.clicked() || !response.ctx.wants_keyboard_input() {
        response.request_focus();
    }
    let terminal_rect =
        egui::Rect::from_min_size(response.rect.min, egui::vec2(total_width, total_height));
    if response.has_focus() {
        let cursor_col = term.cursor_col.min(term.cols.saturating_sub(1));
        let cursor_rect = egui::Rect::from_min_size(
            egui::pos2(
                terminal_rect.min.x + cursor_col as f32 * cell_width,
                terminal_rect.min.y + term.cursor_row as f32 * cell_height,
            ),
            egui::vec2(cell_width, cell_height),
        );
        ui.ctx().output_mut(|output| {
            output.ime = Some(egui::output::IMEOutput {
                rect: terminal_rect,
                cursor_rect,
            });
        });
    }
    let geometry = TerminalPaintGeometry {
        rect: terminal_rect,
        canvas_rect: response.rect,
        cell_width,
        cell_height,
        render_scale,
    };
    paint_terminal(&term, geometry, painter.clone(), selection);

    drop(term);
    TerminalResponse {
        response,
        rect: terminal_rect,
        cell_width,
        cell_height,
        rows: TERMINAL_ROWS,
        cols: TERMINAL_COLS,
    }
}

fn paint_selection(
    term: &Terminal,
    selection: Selection,
    rect: egui::Rect,
    painter: &egui::Painter,
    cell_width: f32,
    cell_height: f32,
) {
    let (start, end) = selection.normalized();
    let color = egui::Color32::from_rgba_premultiplied(120, 170, 255, 90);

    for row in start.row..=end.row.min(term.rows.saturating_sub(1)) {
        let start_col = if row == start.row { start.col } else { 0 };
        let end_col = if row == end.row {
            end.col
        } else {
            term.cols.saturating_sub(1)
        };
        let left = rect.min.x + start_col as f32 * cell_width;
        let top = rect.min.y + row as f32 * cell_height;
        let width = (end_col.saturating_sub(start_col) + 1) as f32 * cell_width;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, cell_height)),
            0.0,
            color,
        );
    }
}

fn paint_terminal(
    term: &Terminal,
    geometry: TerminalPaintGeometry,
    painter: egui::Painter,
    selection: Option<Selection>,
) {
    painter.rect_filled(geometry.canvas_rect, 0.0, egui::Color32::BLACK);
    paint_terminal_edge_bleed(
        term,
        geometry.rect,
        geometry.canvas_rect,
        &painter,
        geometry.cell_width,
        geometry.cell_height,
    );
    if let Some(selection) = selection {
        paint_selection(
            term,
            selection,
            geometry.rect,
            &painter,
            geometry.cell_width,
            geometry.cell_height,
        );
    }

    for row in 0..term.rows {
        for col in 0..term.cols {
            let cell = &term.grid[row][col];
            if cell.width == 0 {
                continue;
            }

            let x = geometry.rect.min.x + col as f32 * geometry.cell_width;
            let y = geometry.rect.min.y + row as f32 * geometry.cell_height;

            let bg_color = cell_background_color(cell);

            if cell.bg_color != cell::Color::Default || cell.reverse {
                let bg_width = geometry.cell_width * cell.width as f32;
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(bg_width, geometry.cell_height),
                    ),
                    0.0,
                    bg_color,
                );
            }

            let fg_color = cell_foreground_color(cell);

            let cell_rect = egui::Rect::from_min_size(
                egui::pos2(x, y),
                egui::vec2(
                    geometry.cell_width * cell.width as f32,
                    geometry.cell_height,
                ),
            );
            if draw_welly_box_char(&painter, cell_rect, cell.ch, fg_color, geometry.cell_width) {
                continue;
            }

            let (font_size, font_family) = if cell.width > 1 {
                (
                    CHINESE_FONT_SIZE * geometry.render_scale,
                    FontFamily::Name(CHINESE_FONT_NAME.into()),
                )
            } else {
                (
                    ENGLISH_FONT_SIZE * geometry.render_scale,
                    FontFamily::Name(ENGLISH_FONT_NAME.into()),
                )
            };
            painter.text(
                text_paint_position(x, y, geometry.render_scale, cell),
                egui::Align2::LEFT_TOP,
                cell.ch.to_string(),
                egui::FontId::new(font_size, font_family),
                fg_color,
            );
        }
    }

    let cursor_col = term.cursor_col.min(term.cols.saturating_sub(1));
    let cursor_cell = &term.grid[term.cursor_row][cursor_col];
    let cursor_width = if cursor_cell.width > 1 {
        cursor_cell.width
    } else {
        1
    };
    let cursor_x = geometry.rect.min.x + cursor_col as f32 * geometry.cell_width;
    let cursor_y = geometry.rect.min.y + term.cursor_row as f32 * geometry.cell_height;
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(cursor_x, cursor_y),
            egui::vec2(
                geometry.cell_width * cursor_width as f32,
                geometry.cell_height,
            ),
        ),
        0.0,
        egui::Color32::from_rgb(200, 200, 200),
    );
}

#[derive(Clone, Copy)]
struct TerminalPaintGeometry {
    rect: egui::Rect,
    canvas_rect: egui::Rect,
    cell_width: f32,
    cell_height: f32,
    render_scale: f32,
}

fn text_paint_position(x: f32, y: f32, render_scale: f32, cell: &cell::Cell) -> egui::Pos2 {
    let (x_offset, y_offset) = if cell.width > 1 {
        (CHINESE_LEFT_MARGIN, CHINESE_TOP_MARGIN)
    } else {
        (ENGLISH_LEFT_MARGIN, ENGLISH_TOP_MARGIN)
    };

    egui::pos2(x + x_offset * render_scale, y + y_offset * render_scale)
}

fn paint_terminal_edge_bleed(
    term: &Terminal,
    terminal_rect: egui::Rect,
    canvas_rect: egui::Rect,
    painter: &egui::Painter,
    cell_width: f32,
    cell_height: f32,
) {
    if terminal_rect.right() < canvas_rect.right() {
        for row in 0..term.rows {
            let cell = visible_cell_at(&term.grid[row], term.cols.saturating_sub(1));
            let color = cell_background_color(cell);

            let top = terminal_rect.top() + row as f32 * cell_height;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(terminal_rect.right(), top),
                    egui::pos2(canvas_rect.right(), top + cell_height),
                ),
                0.0,
                color,
            );
        }
    }

    if terminal_rect.bottom() < canvas_rect.bottom() {
        let row = term.rows.saturating_sub(1);
        for col in 0..term.cols {
            let cell = visible_cell_at(&term.grid[row], col);
            let color = cell_background_color(cell);

            let left = terminal_rect.left() + col as f32 * cell_width;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(left, terminal_rect.bottom()),
                    egui::pos2(left + cell_width, canvas_rect.bottom()),
                ),
                0.0,
                color,
            );
        }

        let cell = visible_cell_at(&term.grid[row], term.cols.saturating_sub(1));
        let color = cell_background_color(cell);
        painter.rect_filled(
            egui::Rect::from_min_max(terminal_rect.right_bottom(), canvas_rect.right_bottom()),
            0.0,
            color,
        );
    }
}

fn visible_cell_at(row: &[cell::Cell], col: usize) -> &cell::Cell {
    if row[col].width != 0 || col == 0 {
        &row[col]
    } else {
        &row[col - 1]
    }
}

fn cell_background_color(cell: &cell::Cell) -> egui::Color32 {
    if cell.reverse {
        foreground_color(cell.fg_color)
    } else {
        background_color(cell.bg_color)
    }
}

fn cell_foreground_color(cell: &cell::Cell) -> egui::Color32 {
    if cell.reverse {
        background_color(cell.bg_color)
    } else {
        foreground_color(cell.fg_color)
    }
}

fn foreground_color(color: cell::Color) -> egui::Color32 {
    match color {
        cell::Color::Default => cell::Color::White.egui_color(),
        _ => color.egui_color(),
    }
}

fn background_color(color: cell::Color) -> egui::Color32 {
    match color {
        cell::Color::Default => egui::Color32::BLACK,
        _ => color.egui_color(),
    }
}

fn terminal_render_scale(
    available_width: f32,
    available_height: f32,
    cols: usize,
    rows: usize,
) -> f32 {
    let base_width = cols as f32 * CELL_WIDTH;
    let base_height = rows as f32 * CELL_HEIGHT;
    let fit_scale = (available_width / base_width).min(available_height / base_height);
    fit_scale.clamp(MIN_ZOOM, MAX_ZOOM)
}

fn draw_welly_box_char(
    painter: &egui::Painter,
    rect: egui::Rect,
    ch: char,
    color: egui::Color32,
    cell_width: f32,
) -> bool {
    let stroke_width = (cell_width / 6.0).round().max(1.0);
    let half_stroke = stroke_width / 2.0;
    let center_x = rect.center().x;
    let center_y = rect.center().y;

    let horizontal = |left: f32, right: f32| {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(left, center_y - half_stroke),
                egui::pos2(right, center_y + half_stroke),
            ),
            0.0,
            color,
        );
    };

    let vertical = |top: f32, bottom: f32| {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(center_x - half_stroke, top),
                egui::pos2(center_x + half_stroke, bottom),
            ),
            0.0,
            color,
        );
    };

    match ch {
        '─' => horizontal(rect.left(), rect.right()),
        '│' => vertical(rect.top(), rect.bottom()),
        '┌' => {
            horizontal(center_x, rect.right());
            vertical(center_y, rect.bottom());
        }
        '┐' => {
            horizontal(rect.left(), center_x);
            vertical(center_y, rect.bottom());
        }
        '└' => {
            horizontal(center_x, rect.right());
            vertical(rect.top(), center_y);
        }
        '┘' => {
            horizontal(rect.left(), center_x);
            vertical(rect.top(), center_y);
        }
        '├' => {
            horizontal(center_x, rect.right());
            vertical(rect.top(), rect.bottom());
        }
        '┤' => {
            horizontal(rect.left(), center_x);
            vertical(rect.top(), rect.bottom());
        }
        '┬' => {
            horizontal(rect.left(), rect.right());
            vertical(center_y, rect.bottom());
        }
        '┴' => {
            horizontal(rect.left(), rect.right());
            vertical(rect.top(), center_y);
        }
        '┼' => {
            horizontal(rect.left(), rect.right());
            vertical(rect.top(), rect.bottom());
        }
        '◆' => {
            let inset_x = stroke_width;
            let inset_y = stroke_width;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(center_x, rect.top() + inset_y),
                    egui::pos2(rect.right() - inset_x, center_y),
                    egui::pos2(center_x, rect.bottom() - inset_y),
                    egui::pos2(rect.left() + inset_x, center_y),
                ],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '━' => horizontal(rect.left(), rect.right()),
        '┃' => vertical(rect.top(), rect.bottom()),
        '▒' => {
            let shade = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 120);
            painter.rect_filled(rect, 0.0, shade);
        }
        '█' | '◼' => {
            painter.rect_filled(rect.shrink2(egui::vec2(1.0, 1.0)), 0.0, color);
        }
        '▁'..='▇' => {
            let levels = (ch as u32 - '▁' as u32 + 1) as f32;
            let height = rect.height() * levels / 8.0;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.bottom() - height),
                    rect.right_bottom(),
                ),
                0.0,
                color,
            );
        }
        '▉'..='▏' => {
            let levels = 8.0 - (ch as u32 - '▉' as u32) as f32;
            let width = cell_width * levels / 8.0;
            painter.rect_filled(
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(width, rect.height())),
                0.0,
                color,
            );
        }
        '▔' => {
            painter.rect_filled(
                egui::Rect::from_min_size(rect.left_top(), egui::vec2(cell_width, stroke_width)),
                0.0,
                color,
            );
        }
        '▕' => {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.right() - stroke_width, rect.top()),
                    rect.right_bottom(),
                ),
                0.0,
                color,
            );
        }
        '◢' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_bottom(), rect.right_bottom(), rect.right_top()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◣' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_bottom(), rect.right_bottom(), rect.left_top()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◤' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_top(), rect.right_top(), rect.left_bottom()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '◥' => {
            painter.add(egui::Shape::convex_polygon(
                vec![rect.left_top(), rect.right_top(), rect.right_bottom()],
                color,
                egui::Stroke::new(0.0, color),
            ));
        }
        '╱' | '／' => {
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        '╲' | '﹨' | '＼' => {
            painter.line_segment(
                [rect.left_top(), rect.right_bottom()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        '╳' => {
            painter.line_segment(
                [rect.left_bottom(), rect.right_top()],
                egui::Stroke::new(stroke_width, color),
            );
            painter.line_segment(
                [rect.left_top(), rect.right_bottom()],
                egui::Stroke::new(stroke_width, color),
            );
        }
        _ => return false,
    }

    true
}
