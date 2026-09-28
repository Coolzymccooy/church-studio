//! OBS connection config, persisted as JSON in `app_data_dir()/obs.json`.
//!
//! The password is stored in the app config (design decision 6) but is never
//! logged: `ObsConfig` has a hand-written `Debug` that redacts it, and the UI
//! only ever receives `ObsConfigView`, which says whether one is set.
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 4455;
pub const CONFIG_FILE: &str = "obs.json";
const MAX_HOST_LEN: usize = 253;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ObsConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub enabled: bool,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            password: String::new(),
            enabled: false,
        }
    }
}

impl fmt::Debug for ObsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObsConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &"<redacted>")
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// What the UI sees: everything except the password itself.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObsConfigView {
    pub host: String,
    pub port: u16,
    pub has_password: bool,
    pub enabled: bool,
}

impl ObsConfig {
    pub fn view(&self) -> ObsConfigView {
        ObsConfigView {
            host: self.host.clone(),
            port: self.port,
            has_password: !self.password.is_empty(),
            enabled: self.enabled,
        }
    }

    /// True when a change to `other` needs the connection to be restarted.
    pub fn connection_differs(&self, other: &ObsConfig) -> bool {
        self.host != other.host
            || self.port != other.port
            || self.password != other.password
            || self.enabled != other.enabled
    }
}

/// Trimmed host name or IP address. Rejects characters that would change the
/// meaning of the `ws://host:port` URL built from it.
pub fn validate_host(host: &str) -> Result<String, String> {
    let host = host.trim();
    if host.is_empty() {
        return Err("Enter the OBS computer's address, for example 127.0.0.1".to_string());
    }
    if host.len() > MAX_HOST_LEN {
        return Err("The OBS address is too long".to_string());
    }
    let bad = |c: char| c.is_whitespace() || matches!(c, '/' | '\\' | '@' | '?' | '#');
    if host.chars().any(bad) {
        return Err("The OBS address can only be a host name or IP address".to_string());
    }
    Ok(host.to_string())
}

pub fn validate_port(port: u16) -> Result<u16, String> {
    if port == 0 {
        return Err("The OBS port must be between 1 and 65535 (OBS uses 4455)".to_string());
    }
    Ok(port)
}

/// Parse the stored file. Anything unreadable or invalid falls back to the
/// defaults field by field, so a hand-edited file can't wedge the app.
pub fn parse_config(text: &str) -> ObsConfig {
    let mut cfg: ObsConfig = serde_json::from_str(text).unwrap_or_default();
    cfg.host = validate_host(&cfg.host).unwrap_or_else(|_| DEFAULT_HOST.to_string());
    cfg.port = validate_port(cfg.port).unwrap_or(DEFAULT_PORT);
    cfg
}

pub fn load_config(path: &Path) -> ObsConfig {
    match fs::read_to_string(path) {
        Ok(text) => parse_config(&text),
        Err(_) => ObsConfig::default(),
    }
}

/// Write via a temp file and rename, so a crash never leaves half a file.
pub fn save_config(path: &Path, cfg: &ObsConfig) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create the settings folder: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("cannot write OBS settings: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot save OBS settings: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_local_obs() {
        let cfg = ObsConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 4455);
        assert!(cfg.password.is_empty());
        assert!(!cfg.enabled);
    }

    #[test]
    fn debug_never_prints_the_password() {
        let cfg = ObsConfig {
            password: "hunter2".to_string(),
            ..ObsConfig::default()
        };
        let text = format!("{cfg:?}");
        assert!(!text.contains("hunter2"));
        assert!(text.contains("redacted"));
    }

    #[test]
    fn view_hides_the_password() {
        let cfg = ObsConfig {
            password: "secret".to_string(),
            ..ObsConfig::default()
        };
        let view = cfg.view();
        assert!(view.has_password);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("hasPassword"));
    }

    #[test]
    fn parse_fills_missing_and_bad_fields_with_defaults() {
        let cfg = parse_config(r#"{"host":"  ","port":0,"enabled":true}"#);
        assert_eq!(cfg.host, DEFAULT_HOST);
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert!(cfg.enabled);
        let cfg = parse_config("not json");
        assert_eq!(cfg, ObsConfig::default());
    }

    #[test]
    fn parse_round_trips() {
        let cfg = ObsConfig {
            host: "studio-pc.local".to_string(),
            port: 4456,
            password: "pw".to_string(),
            enabled: true,
        };
        let text = serde_json::to_string(&cfg).unwrap();
        assert_eq!(parse_config(&text), cfg);
    }

    #[test]
    fn host_validation() {
        assert_eq!(validate_host(" 192.168.1.20 ").unwrap(), "192.168.1.20");
        assert!(validate_host("").is_err());
        assert!(validate_host("evil/path").is_err());
        assert!(validate_host("user@host").is_err());
        assert!(validate_host("two words").is_err());
    }

    #[test]
    fn port_validation() {
        assert!(validate_port(0).is_err());
        assert_eq!(validate_port(4455).unwrap(), 4455);
    }

    #[test]
    fn connection_differs_ignores_nothing_it_should_not() {
        let a = ObsConfig::default();
        let mut b = a.clone();
        assert!(!a.connection_differs(&b));
        b.enabled = true;
        assert!(a.connection_differs(&b));
    }
}
