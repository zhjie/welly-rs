use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSettings {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub identity_files: Vec<PathBuf>,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            host: "bbs.newsmth.net".to_owned(),
            port: 22,
            username: None,
            password: None,
            identity_files: Vec::new(),
        }
    }
}

impl ConnectionSettings {
    pub fn load_default() -> Self {
        let mut settings = Self::default();

        if let Some(home_dir) = dirs::home_dir() {
            let ssh_config_path = home_dir.join(".ssh/config");
            if let Ok(contents) = std::fs::read_to_string(&ssh_config_path) {
                settings.apply_ssh_config(&contents);
            }
        }

        settings
    }

    fn apply_ssh_config(&mut self, contents: &str) {
        let Some(block) = find_ssh_host_block(contents, &self.host) else {
            return;
        };

        for line in block.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }

            let Some((key, value)) = split_ssh_config_directive(line) else {
                continue;
            };

            match key.to_ascii_lowercase().as_str() {
                "port" => {
                    if let Ok(port) = value.parse() {
                        self.port = port;
                    }
                }
                "user" => self.username = Some(value.to_owned()),
                "identityfile" => self.identity_files.push(expand_home_path(value)),
                _ => {}
            }
        }
    }
}

fn find_ssh_host_block<'a>(contents: &'a str, host: &str) -> Option<&'a str> {
    let mut matching_start = None;
    let mut block_start = 0;

    for (line_start, line) in line_spans(contents) {
        let line_body = strip_comment(line).trim();
        if !line_body.to_ascii_lowercase().starts_with("host ") {
            continue;
        }

        if matching_start.is_some() {
            return Some(&contents[block_start..line_start]);
        }

        let patterns = line_body[4..].split_whitespace();
        if patterns.into_iter().any(|pattern| ssh_host_pattern_matches(pattern, host)) {
            matching_start = Some(line_start);
            block_start = line_start + line.len();
            if contents.as_bytes().get(block_start) == Some(&b'\n') {
                block_start += 1;
            }
        }
    }

    matching_start.map(|_| &contents[block_start..])
}

fn line_spans(contents: &str) -> impl Iterator<Item = (usize, &str)> {
    contents.split_inclusive('\n').scan(0, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.trim_end_matches('\n')))
    })
}

fn ssh_host_pattern_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return host.starts_with(prefix);
    }

    pattern == host
}

fn split_ssh_config_directive(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let key = parts.next()?;
    let value = parts.next()?.trim();
    Some((key, value))
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn expand_home_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home_dir) = dirs::home_dir() {
            return home_dir.join(rest);
        }
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::ConnectionSettings;

    #[test]
    fn default_connection_has_no_login_name() {
        let settings = ConnectionSettings::default();

        assert_eq!(settings.host, "bbs.newsmth.net");
        assert_eq!(settings.port, 22);
        assert_eq!(settings.username, None);
        assert_eq!(settings.password, None);
        assert!(settings.identity_files.is_empty());
    }

    #[test]
    fn reads_login_settings_from_matching_newsmth_entry() {
        let mut settings = ConnectionSettings::default();

        settings.apply_ssh_config(
            r#"
Host github.com
    User git

Host bbs.newsmth.net
    User bbs-user
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
"#,
        );

        assert_eq!(settings.host, "bbs.newsmth.net");
        assert_eq!(settings.username.as_deref(), Some("bbs-user"));
        assert_eq!(settings.identity_files.len(), 1);
    }

    #[test]
    fn ignores_hostname_so_deprecated_aliases_do_not_replace_default_host() {
        let mut settings = ConnectionSettings::default();

        settings.apply_ssh_config(
            r#"
Host bbs.newsmth.net
    HostName deprecated.example
    User bbs-user
"#,
        );

        assert_eq!(settings.host, "bbs.newsmth.net");
        assert_eq!(settings.username.as_deref(), Some("bbs-user"));
    }

    #[test]
    fn leaves_username_empty_without_matching_ssh_config_entry() {
        let mut settings = ConnectionSettings::default();

        settings.apply_ssh_config(
            r#"
Host github.com
    User git
"#,
        );

        assert_eq!(settings.username, None);
        assert!(settings.identity_files.is_empty());
    }
}
