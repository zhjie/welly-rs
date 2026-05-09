use crate::ansi_parser::AnsiParser;
use crate::terminal::Terminal;
use russh::*;
use std::sync::{Arc, Mutex};

pub struct SshClient {
    session: Arc<Mutex<client::Handle<Client>>>,
    channel_id: ChannelId,
}

impl SshClient {
    pub async fn connect(
        host: &str,
        port: u16,
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

        let mut session = client::connect(config, (host, port), sh).await?;

        let key_path = dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Containers/com.ytang.Welly/Data/.ssh/id_ed25519");
        
        let auth_res = if key_path.exists() {
            log::info!("Using SSH key authentication with: {:?}", key_path);
            let key = russh::keys::load_secret_key(&key_path, None
            ).map_err(|e| {
                log::error!("Failed to load SSH key: {}", e);
                russh::Error::CouldNotReadKey
            })?;
            
            let key_with_hash = russh::keys::PrivateKeyWithHashAlg::new(
                Arc::new(key),
                None,
            );
            
            session
                .authenticate_publickey("cppbuilder", key_with_hash)
                .await?
        } else {
            log::info!("SSH key not found, trying password authentication");
            session
                .authenticate_password("cppbuilder", "cppbuilder")
                .await?
        };

        if !auth_res.success() {
            return Err(russh::Error::CouldNotReadKey);
        }

        let mut channel = session.channel_open_session().await?;
        let channel_id = channel.id();

        let client = Self {
            session: Arc::new(Mutex::new(session)),
            channel_id,
        };

        let client_arc = Arc::new(client);

        channel.request_pty(true, "xterm-256color", 80, 24, 0, 0, &[]).await?;
        channel.request_shell(true).await?;

        tokio::spawn(async move {
            log::info!("Starting SSH data receive loop");
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        log::debug!("Received {} bytes", data.len());
                        {
                            let mut parser = parser.lock().unwrap();
                            let mut term = terminal.lock().unwrap();
                            parser.feed_bytes(&data, &mut term);
                        }
                        ctx.request_repaint();
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        log::debug!("Received extended data: {} bytes", data.len());
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
        });

        Ok(client_arc)
    }

    pub async fn send_data(&self, data: &[u8]) -> Result<(), russh::Error> {
        let session = self.session.lock().unwrap();
        let _ = session.data(self.channel_id, bytes::Bytes::copy_from_slice(data)).await;
        Ok(())
    }
}

struct Client {}

impl client::Handler for Client {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}
