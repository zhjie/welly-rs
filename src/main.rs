#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::items_after_test_module)]

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

const ZOOM_STEP: f32 = 1.05;
const APP_ICON_RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/welly-rs-app-icon.rgba"));



mod backend;
mod ui;
mod config;

use ui::egui::fonts::*;
use ui::egui::input::bytes_for_egui_event;
use ui::egui::render::{render_terminal, TerminalResponse, CELL_WIDTH, CELL_HEIGHT, TERMINAL_COLS, TERMINAL_ROWS, MIN_ZOOM, MAX_ZOOM};
use ui::egui::selection::{GridPoint, Selection, normalize_selected_url_for_open, selected_text, terminal_screen_text, url_at_grid_point};

use backend::ansi_parser::AnsiParser;
use backend::attachment::{parse_image_attachments, ImageAttachment};
use config::ConnectionSettings;
use backend::ssh::{is_channel_closed_error, SshClient};
use backend::terminal::Terminal;

type ConnectResult = Result<Arc<SshClient>, String>;
type ConnectSender = Sender<ConnectResult>;
type ConnectReceiver = Receiver<ConnectResult>;



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
            Ok(Box::new(App::default()))
        }),
    )
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
    suppress_mouse_entry_click: bool,
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
            suppress_mouse_entry_click: false,
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
                let term = self.terminal.lock().unwrap();
                let snap = term.snapshot();
                let terminal_response = render_terminal(ui, &snap, self.selection);
                drop(term);

                self.handle_terminal_url_click(&terminal_response);
                self.handle_terminal_selection(ctx, &terminal_response);
                self.handle_terminal_mouse_click(&terminal_response);
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

        // `centered: true` centers the window on the screen, but on Windows the
        // taskbar at the bottom makes the work area's vertical center appear lower
        // than expected. Read the actual monitor size and outer rect from the first
        // frame and move the window up so it sits at roughly 1/3 from the top.
        let (monitor_size, outer_rect) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.monitor_size, vp.outer_rect)
        });
        if let (Some(monitor), Some(outer)) = (monitor_size, outer_rect) {
            let win_h = outer.height();
            let target_y = (monitor.y - win_h) / 3.0;
            if target_y >= 0.0 && (target_y - outer.min.y).abs() > 10.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    outer.min.x,
                    target_y,
                )));
            }
        }

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
        let ctx_for_notify = ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> =
            Arc::new(move || ctx_for_notify.request_repaint());
        let (tx, rx): (ConnectSender, ConnectReceiver) = crossbeam_channel::bounded(1);
        self.connect_rx = Some(rx);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match SshClient::connect(settings, terminal, parser, notify).await {
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
            parse_image_attachments(&terminal_screen_text(&terminal.snapshot()))
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
                egui::Event::MouseWheel { .. } => {
                    if let Some(bytes) = bytes_for_egui_event(&event) {
                        self.selection = None;
                        self.send_bytes(bytes);
                    }
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

                    // macOS: Cmd+R (command=true, ctrl=false)
                    // Windows/Linux: Alt+R (alt=true, ctrl=false)
                    let is_reconnect = key == egui::Key::R
                        && (modifiers.command || modifiers.alt)
                        && !modifiers.ctrl;
                    if is_reconnect {
                        self.selection = None;
                        self.reconnect(ctx);
                        continue;
                    }

                    if handle_zoom_shortcut(&mut self.zoom, key, modifiers) {
                        self.selection = None;
                        self.pending_inner_size = Some(terminal_size_for_zoom(self.zoom));
                        continue;
                    }

                    if let Some(bytes) = bytes_for_egui_event(&event) {
                        self.selection = None;
                        self.send_bytes(bytes);
                    }
                }
                egui::Event::Text(_) | egui::Event::Ime(_) => {
                    if let Some(bytes) = bytes_for_egui_event(&event) {
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
                self.suppress_mouse_entry_click = true;
            }
        } else if terminal_response.response.dragged() {
            if let (Some(selection), Some(point)) =
                (&mut self.selection, terminal_response.interact_grid_point())
            {
                selection.end = point;
                self.suppress_mouse_entry_click = true;
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::C) && input.modifiers.command) {
            self.copy_selection(ctx);
        }

        if terminal_response
            .response
            .double_clicked_by(egui::PointerButton::Primary)
        {
            self.suppress_mouse_entry_click = true;
            self.open_selected_url();
        }
    }

    fn handle_terminal_url_click(&self, terminal_response: &TerminalResponse) {
        let Some(point) = terminal_response.hover_grid_point() else {
            return;
        };

        let url = {
            let terminal = self.terminal.lock().unwrap();
            url_at_grid_point(&terminal.snapshot(), point)
        };

        if let Some(url) = url {
            terminal_response
                .response
                .ctx
                .set_cursor_icon(egui::CursorIcon::PointingHand);
            if terminal_response
                .response
                .clicked_by(egui::PointerButton::Primary)
            {
                open_url(&url);
            }
        }
    }

    fn handle_terminal_mouse_click(&mut self, terminal_response: &TerminalResponse) {
        if !terminal_response
            .response
            .clicked_by(egui::PointerButton::Primary)
        {
            return;
        }

        if self.suppress_mouse_entry_click {
            self.suppress_mouse_entry_click = false;
            return;
        }

        let Some(point) = terminal_response.interact_grid_point() else {
            return;
        };

        let terminal = self.terminal.lock().unwrap();
        if url_at_grid_point(&terminal.snapshot(), point).is_some() {
            return;
        }

        let bytes = if is_mouse_entry_click_point(&terminal, point) {
            Some(mouse_entry_click_to_bytes(terminal.cursor_row, point.row))
        } else {
            mouse_background_navigation_bytes(point)
        };
        drop(terminal);

        if let Some(bytes) = bytes {
            self.selection = None;
            self.send_bytes(bytes);
        }
    }

    fn copy_selection(&self, ctx: &egui::Context) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };

        let text = {
            let terminal = self.terminal.lock().unwrap();
            selected_text(&terminal.snapshot(), selection)
        };

        if text.is_empty() {
            return false;
        }

        ctx.copy_text(text);
        true
    }

    fn open_selected_url(&self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };

        let url = {
            let terminal = self.terminal.lock().unwrap();
            normalize_selected_url_for_open(&selected_text(&terminal.snapshot(), selection))
        };

        if let Some(url) = url {
            open_url(&url);
            true
        } else {
            false
        }
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
        open_url(&attachment.image_url);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OpenUrlCommand {
    program: &'static str,
    args: Vec<String>,
}

fn open_url_command(url: &str) -> OpenUrlCommand {
    #[cfg(target_os = "windows")]
    {
        return OpenUrlCommand {
            program: "rundll32.exe",
            args: vec!["url.dll,FileProtocolHandler".to_owned(), url.to_owned()],
        };
    }

    #[cfg(target_os = "macos")]
    {
        return OpenUrlCommand {
            program: "open",
            args: vec![url.to_owned()],
        };
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return OpenUrlCommand {
            program: "xdg-open",
            args: vec![url.to_owned()],
        };
    }

    #[allow(unreachable_code)]
    OpenUrlCommand {
        program: "open",
        args: vec![url.to_owned()],
    }
}

fn open_url(url: &str) {
    let command = open_url_command(url);
    if let Err(error) = Command::new(command.program).args(&command.args).spawn() {
        log::error!("Failed to open URL {}: {}", url, error);
    }
}



fn mouse_entry_click_to_bytes(cursor_row: usize, target_row: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    if target_row > cursor_row {
        for _ in cursor_row..target_row {
            bytes.extend_from_slice(b"\x1b[B");
        }
    } else {
        for _ in target_row..cursor_row {
            bytes.extend_from_slice(b"\x1b[A");
        }
    }
    bytes.push(b'\r');
    bytes
}

fn is_mouse_entry_click_point(term: &Terminal, point: GridPoint) -> bool {
    if !(3..term.rows.saturating_sub(1)).contains(&point.row) || point.col < 2 {
        return false;
    }

    let row = &term.grid[point.row];
    let Some(start) = row
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(col, cell)| (cell.width != 0 && cell.ch != ' ').then_some(col))
    else {
        return false;
    };

    let Some(end) = row.iter().enumerate().rev().find_map(|(col, cell)| {
        (cell.width != 0 && cell.ch != ' ' && cell.ch != '\0').then_some(col)
    }) else {
        return false;
    };

    let click_end = end
        .max(start.saturating_add(29))
        .min(term.cols.saturating_sub(1));
    (start..=click_end).contains(&point.col)
}

fn mouse_background_navigation_bytes(point: GridPoint) -> Option<Vec<u8>> {
    if point.col == 0 && (3..TERMINAL_ROWS.saturating_sub(1)).contains(&point.row) {
        return Some(b"\x1b[D".to_vec());
    }

    if point.col >= 20 {
        if point.row < TERMINAL_ROWS / 2 {
            return Some(b"\x1b[5~".to_vec());
        }
        return Some(b"\x1b[6~".to_vec());
    }

    None
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
    use super::{handle_zoom_shortcut,
        is_mouse_entry_click_point, mouse_background_navigation_bytes,
        mouse_entry_click_to_bytes, terminal_aspect_fit_size,
        terminal_size_for_zoom,
    };
    use crate::backend::terminal::Terminal;
    use crate::ui::egui::render::{terminal_render_scale, TERMINAL_COLS, TERMINAL_ROWS, CELL_WIDTH, CELL_HEIGHT, MIN_ZOOM};
    use crate::ui::egui::selection::GridPoint;


    #[test]
    fn mouse_entry_click_moves_cursor_to_row_and_enters() {
        assert_eq!(
            mouse_entry_click_to_bytes(3, 6),
            b"\x1b[B\x1b[B\x1b[B\r".to_vec()
        );
        assert_eq!(
            mouse_entry_click_to_bytes(6, 3),
            b"\x1b[A\x1b[A\x1b[A\r".to_vec()
        );
    }

    #[test]
    fn mouse_background_areas_map_to_welly_navigation_keys() {
        assert_eq!(
            mouse_background_navigation_bytes(GridPoint { row: 8, col: 0 }),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            mouse_background_navigation_bytes(GridPoint { row: 4, col: 30 }),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            mouse_background_navigation_bytes(GridPoint { row: 18, col: 30 }),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            mouse_background_navigation_bytes(GridPoint { row: 8, col: 10 }),
            None
        );
    }

    #[test]
    fn mouse_entry_click_point_uses_visible_text_range() {
        let mut terminal = Terminal::new(24, 80);
        put_ascii(&mut terminal, 5, 12, "Re: title");

        assert!(is_mouse_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 12 }
        ));
        assert!(is_mouse_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 38 }
        ));
        assert!(!is_mouse_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 60 }
        ));
        assert!(!is_mouse_entry_click_point(
            &terminal,
            GridPoint { row: 2, col: 12 }
        ));
    }

    fn put_ascii(terminal: &mut Terminal, row: usize, col: usize, text: &str) {
        terminal.set_cursor(row, col);
        for ch in text.chars() {
            terminal.put_char(ch);
        }
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
        let base_width = TERMINAL_COLS as f32 * CELL_WIDTH;
        let base_height = TERMINAL_ROWS as f32 * CELL_HEIGHT;

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
            MIN_ZOOM
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
                TERMINAL_COLS as f32 * CELL_WIDTH * 2.0,
                TERMINAL_ROWS as f32 * CELL_HEIGHT * 2.0
            )
        );
    }

    #[test]
    fn attachment_button_label_opens_all_detected_images() {
        let attachment = crate::backend::attachment::ImageAttachment {
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

#[cfg(target_os = "windows")]
    #[test]
    fn open_url_command_uses_windows_url_protocol_handler() {
        assert_eq!(
            super::open_url_command("https://example.com/path?a=1&b=2"),
            super::OpenUrlCommand {
                program: "rundll32.exe",
                args: vec![
                    "url.dll,FileProtocolHandler".to_owned(),
                    "https://example.com/path?a=1&b=2".to_owned(),
                ],
            }
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn open_url_command_uses_macos_open() {
        assert_eq!(
            super::open_url_command("https://example.com/path"),
            super::OpenUrlCommand {
                program: "open",
                args: vec!["https://example.com/path".to_owned()],
            }
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn open_url_command_uses_xdg_open_on_unix_desktops() {
        assert_eq!(
            super::open_url_command("https://example.com/path"),
            super::OpenUrlCommand {
                program: "xdg-open",
                args: vec!["https://example.com/path".to_owned()],
            }
        );
    }
}

