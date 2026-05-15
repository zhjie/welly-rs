//! High-level backend API consumed by frontends.
//!
//! Spec §3 API. All InputEvent → bytes translation lives here so a future
//! gpui frontend can reuse the byte mappings without touching SSH or
//! terminal state directly.

use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use encoding_rs::GB18030;
use tokio::sync::watch;

use super::ansi_parser::AnsiParser;
use super::input::{InputEvent, MouseEvent};
use super::snapshot::TerminalSnapshot;
use super::ssh::{is_channel_closed_error, SshClient};
use super::terminal::Terminal;
use super::{keys, mouse};
use crate::config::ConnectionSettings;

type ConnectResult = Result<Arc<SshClient>, String>;

pub struct Backend {
    settings: Mutex<ConnectionSettings>,
    terminal: Arc<Mutex<Terminal>>,
    parser: Arc<Mutex<AnsiParser>>,
    client: Mutex<Option<Arc<SshClient>>>,
    connect_rx: Mutex<Option<Receiver<ConnectResult>>>,
    connection_error: Mutex<Option<String>>,
    notify: Arc<dyn Fn() + Send + Sync>,
    changes_tx: watch::Sender<()>,
}

impl Backend {
    pub fn new(config: ConnectionSettings, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (changes_tx, _rx) = watch::channel(());
        Self {
            settings: Mutex::new(config),
            terminal: Arc::new(Mutex::new(Terminal::new(24, 80))),
            parser: Arc::new(Mutex::new(AnsiParser::new())),
            client: Mutex::new(None),
            connect_rx: Mutex::new(None),
            connection_error: Mutex::new(None),
            notify: combined_notify(notify, changes_tx.clone()),
            changes_tx,
        }
        // No auto-connect. Call `reconnect()` to start the first
        // connection — App does this when settings look valid, or after
        // the user submits the login form.
    }

    /// Read-only snapshot of terminal state. The closure runs under the
    /// terminal lock; keep it short.
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&TerminalSnapshot<'_>) -> R) -> R {
        let t = self.terminal.lock().unwrap();
        let snap = t.snapshot();
        f(&snap)
    }

    /// Translate a high-level input event into bytes and forward to SSH.
    /// Owns text/IME GB18030 encoding, Welly key escape mapping, and the
    /// wheel/entry/background mouse resolution against current cursor state.
    pub fn send_input(&self, event: InputEvent) {
        let bytes_opt = match event {
            InputEvent::Key(k) => keys::bytes_for_key(k),
            InputEvent::Mouse(m) => self.bytes_for_mouse(m),
            InputEvent::Paste(text) => paste_bytes(&text),
            InputEvent::Resize { .. } => {
                // Welly is a fixed 24×80 BBS terminal. Resize is part of the
                // InputEvent surface (spec §5) so future frontends or future
                // resizable BBS profiles can route it through here, but the
                // current Terminal has no resize path. No-op for Phase 1.
                None
            }
            InputEvent::Reconnect => {
                self.reconnect();
                None
            }
            InputEvent::Shutdown => {
                self.shutdown();
                None
            }
        };
        if let Some(b) = bytes_opt {
            self.send_bytes(b);
        }
    }

    /// Subscribe to lightweight change notifications. Returns a watch
    /// receiver that fires whenever Backend or the SSH read loop calls the
    /// internal notify. This is not a full state log; consumers should
    /// re-read snapshot / connection state after receiving a change. The
    /// egui frontend uses the push notify (egui::Context repaint); a future
    /// gpui frontend can `await` this receiver.
    #[allow(dead_code)]
    pub fn subscribe_changes(&self) -> watch::Receiver<()> {
        self.changes_tx.subscribe()
    }

    /// Drop current SSH client and start a fresh connection using the
    /// most recently configured settings.
    pub fn reconnect(&self) {
        *self.client.lock().unwrap() = None;
        self.spawn_connect();
        (self.notify)();
    }

    /// Request graceful teardown. Drops the SSH client; the background
    /// tokio runtime exits as the channel closes.
    pub fn shutdown(&self) {
        *self.client.lock().unwrap() = None;
        *self.connect_rx.lock().unwrap() = None;
        (self.notify)();
    }

    /// Update connection settings for future reconnects (login form).
    pub fn update_settings(&self, settings: ConnectionSettings) {
        *self.settings.lock().unwrap() = settings;
        (self.notify)();
    }

    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        self.client
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.is_connected())
    }

    /// Returns true while a connect attempt is in flight (before success
    /// or failure is reported via `poll_connect_result`).
    pub fn is_connecting(&self) -> bool {
        self.connect_rx.lock().unwrap().is_some()
    }

    /// Drain the connect channel non-blockingly; promotes a successful
    /// connection into `self.client` and stores an error for the UI.
    /// Returns `Some(Ok(()))` exactly once on success, `Some(Err(msg))`
    /// once on failure, `None` while still pending.
    pub fn poll_connect_result(&self) -> Option<Result<(), String>> {
        let rx = self.connect_rx.lock().unwrap().clone()?;
        let result = rx.try_recv().ok()?;
        *self.connect_rx.lock().unwrap() = None;
        match result {
            Ok(client) => {
                *self.client.lock().unwrap() = Some(client);
                (self.notify)();
                Some(Ok(()))
            }
            Err(e) => {
                *self.connection_error.lock().unwrap() = Some(e.clone());
                Some(Err(e))
            }
        }
    }

    #[allow(dead_code)]
    pub fn take_connection_error(&self) -> Option<String> {
        self.connection_error.lock().unwrap().take()
    }

    // ---- internals ----

    fn spawn_connect(&self) {
        self.terminal.lock().unwrap().clear_all();
        *self.parser.lock().unwrap() = AnsiParser::new();
        (self.notify)();

        let terminal = Arc::clone(&self.terminal);
        let parser = Arc::clone(&self.parser);
        let notify = Arc::clone(&self.notify);
        let settings = self.settings.lock().unwrap().clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        *self.connect_rx.lock().unwrap() = Some(rx);

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

    fn send_bytes(&self, bytes: Vec<u8>) {
        let Some(client) = self.client.lock().unwrap().clone() else {
            return;
        };
        if !client.is_connected() {
            return;
        }
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

    fn bytes_for_mouse(&self, event: MouseEvent) -> Option<Vec<u8>> {
        match event {
            MouseEvent::Wheel(d) => Some(mouse::bytes_for_wheel(d)),
            MouseEvent::Click(point) => {
                let term = self.terminal.lock().unwrap();
                if mouse::is_entry_click_point(&term, point) {
                    Some(mouse::bytes_for_entry_click(term.cursor_row, point.row))
                } else {
                    mouse::bytes_for_background_navigation(point)
                }
            }
        }
    }
}

fn paste_bytes(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    let (b, _, _) = GB18030.encode(text);
    Some(b.into_owned())
}

fn combined_notify(
    user: Arc<dyn Fn() + Send + Sync>,
    tx: watch::Sender<()>,
) -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(move || {
        user();
        tx.send_replace(());
    })
}
