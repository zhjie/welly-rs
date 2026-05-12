use crate::ansi_parser::AnsiParser;
use crate::config::ConnectionSettings;
use crate::terminal::Terminal;
use russh::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ANTI_IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const ANTI_IDLE_THRESHOLD: Duration = Duration::from_secs(59);
const ANTI_IDLE_PAYLOAD: [u8; 6] = [0; 6];

pub struct SshClient {
    session: Arc<tokio::sync::Mutex<client::Handle<Client>>>,
    channel_id: ChannelId,
    last_send_at: Arc<Mutex<Instant>>,
    connected: Arc<AtomicBool>,
}

impl SshClient {
    pub async fn connect(
        settings: ConnectionSettings,
        terminal: Arc<Mutex<Terminal>>,
        parser: Arc<Mutex<AnsiParser>>,
        ctx: eframe::egui::Context,
    ) -> Result<Arc<Self>, russh::Error> {
        let config = client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(300)),
            ..Default::default()
        };
        let config = Arc::new(config);
        let sh = Client {};

        let mut session =
            client::connect(config, (settings.host.as_str(), settings.port), sh).await?;
        authenticate(&mut session, &settings).await?;

        let mut channel = session.channel_open_session().await?;
        let channel_id = channel.id();

        let client = Self {
            session: Arc::new(tokio::sync::Mutex::new(session)),
            channel_id,
            last_send_at: Arc::new(Mutex::new(Instant::now())),
            connected: Arc::new(AtomicBool::new(true)),
        };

        let client_arc = Arc::new(client);
        client_arc.start_anti_idle_loop();

        channel
            .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;

        let connected = Arc::clone(&client_arc.connected);
        tokio::spawn(async move {
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        {
                            let mut parser = parser.lock().unwrap();
                            let mut term = terminal.lock().unwrap();
                            parser.feed_bytes(&data, &mut term);
                        }
                        ctx.request_repaint();
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        {
                            let mut parser = parser.lock().unwrap();
                            let mut term = terminal.lock().unwrap();
                            parser.feed_bytes(&data, &mut term);
                        }
                        ctx.request_repaint();
                    }
                    Some(ChannelMsg::Eof) => {
                        log::info!("SSH channel EOF");
                        break;
                    }
                    Some(ChannelMsg::Close) => {
                        log::info!("SSH channel closed");
                        break;
                    }
                    None => {
                        log::info!("SSH channel ended");
                        break;
                    }
                    _ => {}
                }
            }
            connected.store(false, Ordering::SeqCst);
        });

        Ok(client_arc)
    }

    pub async fn send_data(&self, data: &[u8]) -> Result<(), russh::Error> {
        if !self.is_connected() {
            return Err(russh::Error::Disconnect);
        }

        let session = self.session.lock().await;
        let result = session
            .data(self.channel_id, bytes::Bytes::copy_from_slice(data))
            .await
            .map_err(|_| russh::Error::SendError);
        if result.is_err() {
            self.connected.store(false, Ordering::SeqCst);
        }
        result?;
        *self.last_send_at.lock().unwrap() = Instant::now();
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn start_anti_idle_loop(self: &Arc<Self>) {
        let client = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ANTI_IDLE_CHECK_INTERVAL);
            loop {
                interval.tick().await;
                if !client.is_connected() {
                    break;
                }
                if client.should_send_anti_idle(Instant::now()) {
                    if let Err(e) = client.send_data(&ANTI_IDLE_PAYLOAD).await {
                        log::debug!("Anti-idle stopped after SSH channel ended: {}", e);
                        break;
                    }
                }
            }
        });
    }

    fn should_send_anti_idle(&self, now: Instant) -> bool {
        anti_idle_due(*self.last_send_at.lock().unwrap(), now)
    }
}

pub fn is_channel_closed_error(error: &russh::Error) -> bool {
    matches!(error, russh::Error::Disconnect | russh::Error::SendError)
}

fn anti_idle_due(last_send_at: Instant, now: Instant) -> bool {
    now.duration_since(last_send_at) >= ANTI_IDLE_THRESHOLD
}

struct Client {}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn authenticate(
    session: &mut client::Handle<Client>,
    settings: &ConnectionSettings,
) -> Result<(), russh::Error> {
    let Some(username) = settings.username.as_deref() else {
        return Err(russh::Error::NotAuthenticated);
    };

    for key_path in settings.identity_files.iter().filter(|path| path.exists()) {
        log::info!("Trying SSH key authentication with: {:?}", key_path);
        let key = russh::keys::load_secret_key(key_path, None).map_err(|e| {
            log::error!("Failed to load SSH key: {}", e);
            russh::Error::CouldNotReadKey
        })?;
        let key_with_hash = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);

        if session
            .authenticate_publickey(username, key_with_hash)
            .await?
            .success()
        {
            return Ok(());
        }
    }

    if let Some(password) = settings.password.as_deref() {
        if session
            .authenticate_password(username, password)
            .await?
            .success()
        {
            return Ok(());
        }
    }

    Err(russh::Error::NotAuthenticated)
}

#[cfg(test)]
mod tests {
    use super::{anti_idle_due, ANTI_IDLE_PAYLOAD};
    use std::time::{Duration, Instant};

    #[test]
    fn anti_idle_payload_matches_welly_nul_bytes() {
        assert_eq!(ANTI_IDLE_PAYLOAD, [0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn anti_idle_stays_quiet_before_threshold() {
        let last_send = Instant::now();

        assert!(!anti_idle_due(
            last_send,
            last_send + Duration::from_secs(58)
        ));
    }

    #[test]
    fn anti_idle_fires_after_welly_idle_window() {
        let last_send = Instant::now();

        assert!(anti_idle_due(
            last_send,
            last_send + Duration::from_secs(59)
        ));
    }
}
