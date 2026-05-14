use crate::backend::attachment::{parse_image_attachments, ImageAttachment};
use crate::backend::input::{InputEvent, MouseEvent};
use crate::backend::Backend;
use crate::config::ConnectionSettings;
use crate::ui::egui::input::input_event_for_egui_event;
use crate::ui::egui::render::{
    render_terminal, TerminalResponse, CELL_HEIGHT, CELL_WIDTH, MAX_ZOOM, MIN_ZOOM, TERMINAL_COLS,
    TERMINAL_ROWS,
};
use crate::ui::egui::selection::{
    normalize_selected_url_for_open, selected_text, terminal_screen_text, url_at_grid_point,
    Selection,
};
use eframe::egui;
use std::process::Command;
use std::sync::Arc;

const ZOOM_STEP: f32 = 1.05;

pub struct App {
    backend: Backend,
    connected: bool,
    login_host: String,
    login_port: String,
    login_username: String,
    login_password: String,
    /// Preserved from initial SSH config load; used when submitting login form.
    identity_files: Vec<std::path::PathBuf>,
    connection_error: Option<String>,
    zoom: f32,
    selection: Option<Selection>,
    suppress_mouse_entry_click: bool,
    pending_inner_size: Option<egui::Vec2>,
    last_inner_size: Option<egui::Vec2>,
    configured_viewport: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = ConnectionSettings::load_default();
        let login_host = settings.host.clone();
        let login_port = settings.port.to_string();
        let login_username = settings.username.clone().unwrap_or_default();
        let identity_files = settings.identity_files.clone();
        let ctx = cc.egui_ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let backend = Backend::new(settings, notify);
        if !login_username.is_empty() {
            backend.reconnect();
        }
        Self {
            backend,
            connected: false,
            login_host,
            login_port,
            login_username,
            login_password: String::new(),
            identity_files,
            connection_error: None,
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
        self.configure_viewport_once(ctx);

        match self.backend.poll_connect_result() {
            Some(Ok(())) => {
                self.connected = true;
            }
            Some(Err(e)) => {
                log::error!("SSH error: {}", e);
                self.connected = false;
                self.connection_error = Some(e);
            }
            None => {}
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
                let terminal_response = self
                    .backend
                    .with_snapshot(|snap| render_terminal(ui, snap, self.selection));

                self.handle_terminal_url_click(&terminal_response);
                self.handle_terminal_selection(ctx, &terminal_response);
                self.handle_terminal_mouse_click(&terminal_response);
                self.render_attachment_button(ui);
                if !self.connected
                    && !self.backend.is_connecting()
                    && (self.login_username.is_empty() || self.connection_error.is_some())
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
                                let new_settings = ConnectionSettings {
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
                                    identity_files: self.identity_files.clone(),
                                };
                                self.connection_error = None;
                                if new_settings.username.is_some() {
                                    self.connected = false;
                                    self.backend.update_settings(new_settings);
                                    self.backend.reconnect();
                                }
                            } else {
                                self.connection_error = Some("Invalid port".to_owned());
                            }
                        }
                    });
            });
    }

    fn render_attachment_button(&self, ui: &mut egui::Ui) {
        let attachments = self
            .backend
            .with_snapshot(|snap| parse_image_attachments(&terminal_screen_text(snap)));
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
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    // macOS: Cmd+R (command=true, ctrl=false)
                    // Windows/Linux: Alt+R (alt=true, ctrl=false)
                    let is_reconnect = key == egui::Key::R
                        && (modifiers.command || modifiers.alt)
                        && !modifiers.ctrl;
                    if is_reconnect {
                        self.selection = None;
                        self.connected = false;
                        self.backend.reconnect();
                        continue;
                    }

                    if handle_zoom_shortcut(&mut self.zoom, key, modifiers) {
                        self.selection = None;
                        self.pending_inner_size = Some(terminal_size_for_zoom(self.zoom));
                        continue;
                    }

                    if let Some(ev) = input_event_for_egui_event(&event) {
                        self.selection = None;
                        self.backend.send_input(ev);
                    }
                }
                egui::Event::MouseWheel { .. } | egui::Event::Text(_) | egui::Event::Ime(_) => {
                    if let Some(ev) = input_event_for_egui_event(&event) {
                        self.selection = None;
                        self.backend.send_input(ev);
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
        } else if terminal_response
            .response
            .drag_stopped_by(egui::PointerButton::Primary)
        {
            // Select-to-copy: automatically copy to clipboard when drag ends.
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

        let url = self.backend.with_snapshot(|snap| url_at_grid_point(snap, point));

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

        if self
            .backend
            .with_snapshot(|snap| url_at_grid_point(snap, point).is_some())
        {
            return;
        }

        self.selection = None;
        self.backend
            .send_input(InputEvent::Mouse(MouseEvent::Click(point)));
    }

    fn copy_selection(&self, ctx: &egui::Context) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };

        let text = self
            .backend
            .with_snapshot(|snap| selected_text(snap, selection));

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

        let url = self.backend.with_snapshot(|snap| {
            normalize_selected_url_for_open(&selected_text(snap, selection))
        });

        if let Some(url) = url {
            open_url(&url);
            true
        } else {
            false
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

pub fn handle_zoom_shortcut(zoom: &mut f32, key: egui::Key, modifiers: egui::Modifiers) -> bool {
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

pub fn terminal_size_for_zoom(zoom: f32) -> egui::Vec2 {
    egui::vec2(
        TERMINAL_COLS as f32 * CELL_WIDTH * zoom,
        TERMINAL_ROWS as f32 * CELL_HEIGHT * zoom,
    )
}

pub fn terminal_aspect_fit_size(size: egui::Vec2) -> egui::Vec2 {
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
        attachment_button_label, handle_zoom_shortcut, open_url_command, terminal_aspect_fit_size,
        terminal_size_for_zoom, OpenUrlCommand,
    };
    use crate::ui::egui::render::{CELL_HEIGHT, CELL_WIDTH, TERMINAL_COLS, TERMINAL_ROWS};

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

        assert_eq!(attachment_button_label(&attachment, 3), "打开 3 张图");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn open_url_command_uses_windows_url_protocol_handler() {
        assert_eq!(
            open_url_command("https://example.com/path?a=1&b=2"),
            OpenUrlCommand {
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
            open_url_command("https://example.com/path"),
            OpenUrlCommand {
                program: "open",
                args: vec!["https://example.com/path".to_owned()],
            }
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn open_url_command_uses_xdg_open_on_unix_desktops() {
        assert_eq!(
            open_url_command("https://example.com/path"),
            OpenUrlCommand {
                program: "xdg-open",
                args: vec!["https://example.com/path".to_owned()],
            }
        );
    }
}
