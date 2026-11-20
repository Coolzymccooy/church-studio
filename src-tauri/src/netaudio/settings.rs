//! Network-audio settings and their file (`app_data_dir()/net_audio.json`,
//! decision 9). Applied at the next engine start, like NDI. A missing or
//! corrupt file falls back to "off" with a fresh passphrase; saving is
//! atomic (temp file, then rename).
use crate::ndi::persist;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const SETTINGS_FILE: &str = "net_audio.json";
pub const DEFAULT_PORT: u16 = 47_810;
/// At most this many Remote strips (decision 4).
pub const MAX_NET_INPUTS: usize = 4;
pub const DEFAULT_SLOTS: usize = 2;
/// Matches `super::MIN_TARGET_MS`: one packet plus a 10 ms device block.
pub const MIN_BUFFER_MS: u32 = 15;
pub const MAX_BUFFER_MS: u32 = 200;
pub const DEFAULT_BUFFER_MS: u32 = 20;
/// Shorter passphrases do not start network audio.
pub const MIN_PASSPHRASE_CHARS: usize = 8;
pub const MAX_PASSPHRASE_CHARS: usize = 128;
pub const MAX_TARGET_CHARS: usize = 255;
pub const MAX_NAME_CHARS: usize = 32;
pub const BUSES: [&str; 3] = ["main", "stream", "monitor"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReceiveConfig {
    pub enabled: bool,
    pub port: u16,
    /// Remote strips reserved at engine start (1–4).
    pub slots: usize,
    /// Jitter-buffer target per peer.
    pub buffer_ms: u32,
}

impl Default for ReceiveConfig {
    fn default() -> Self {
        ReceiveConfig {
            enabled: false,
            port: DEFAULT_PORT,
            slots: DEFAULT_SLOTS,
            buffer_ms: DEFAULT_BUFFER_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SendConfig {
    pub enabled: bool,
    /// "host:port" of the receiving Studio.
    pub target: String,
    /// "main", "stream" or "monitor".
    pub bus: String,
    pub stereo: bool,
    /// Shown on the receiving side; empty = this machine's name.
    pub name: String,
}

impl Default for SendConfig {
    fn default() -> Self {
        SendConfig {
            enabled: false,
            target: String::new(),
            bus: "main".to_string(),
            stereo: false,
            name: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NetAudioConfig {
    pub receive: ReceiveConfig,
    pub send: SendConfig,
    /// The room passphrase that signs every packet (decision 3).
    pub passphrase: String,
}

/// Trim, drop control characters, cut to `max` characters.
fn clean_text(text: &str, max: usize) -> String {
    let kept: String = text.chars().filter(|c| !c.is_control()).take(max).collect();
    kept.trim().to_string()
}

impl NetAudioConfig {
    /// A copy with every field in range. Mirrored by `sanitizeNetAudio` in
    /// `src/lib/netAudio.js`.
    pub fn sanitized(&self) -> Self {
        let receive = &self.receive;
        let send = &self.send;
        let bus = send.bus.trim().to_lowercase();
        NetAudioConfig {
            receive: ReceiveConfig {
                enabled: receive.enabled,
                port: if receive.port == 0 { DEFAULT_PORT } else { receive.port },
                slots: receive.slots.clamp(1, MAX_NET_INPUTS),
                buffer_ms: receive.buffer_ms.clamp(MIN_BUFFER_MS, MAX_BUFFER_MS),
            },
            send: SendConfig {
                enabled: send.enabled,
                target: clean_text(&send.target, MAX_TARGET_CHARS),
                bus: if BUSES.contains(&bus.as_str()) { bus } else { "main".to_string() },
                stereo: send.stereo,
                name: clean_text(&send.name, MAX_NAME_CHARS),
            },
            passphrase: clean_text(&self.passphrase, MAX_PASSPHRASE_CHARS),
        }
    }

    /// Why network audio cannot start with this passphrase, if it cannot.
    pub fn passphrase_problem(&self) -> Option<String> {
        (self.passphrase.chars().count() < MIN_PASSPHRASE_CHARS)
            .then(|| format!("passphrase needs at least {MIN_PASSPHRASE_CHARS} characters"))
    }

    pub fn receive_ready(&self) -> bool {
        self.receive.enabled && self.passphrase_problem().is_none()
    }

    pub fn send_ready(&self) -> bool {
        self.send.enabled && !self.send.target.is_empty() && self.passphrase_problem().is_none()
    }

    /// Remote strips the engine reserves (0 when receiving is off).
    pub fn strips(&self) -> usize {
        if self.receive_ready() {
            self.receive.slots.clamp(1, MAX_NET_INPUTS)
        } else {
            0
        }
    }

    /// Defaults with a fresh random passphrase.
    pub fn fresh() -> Self {
        NetAudioConfig {
            passphrase: generate_passphrase(),
            ..NetAudioConfig::default()
        }
    }
}

/// Twelve characters from an alphabet without look-alikes, in three groups
/// ("k7fq-9m2x-tr4d"), from the OS random source.
pub fn generate_passphrase() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 12];
    if getrandom::getrandom(&mut bytes).is_err() {
        // No OS randomness: leave it empty, so network audio cannot start
        // until the operator types one.
        return String::new();
    }
    let chars: Vec<char> = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    chars
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn from_json(text: &str) -> Result<NetAudioConfig, String> {
    serde_json::from_str::<NetAudioConfig>(text)
        .map(|config| config.sanitized())
        .map_err(|e| format!("network audio settings file is not valid: {e}"))
}

pub fn to_json(config: &NetAudioConfig) -> Result<String, String> {
    serde_json::to_string_pretty(config)
        .map_err(|e| format!("cannot encode network audio settings: {e}"))
}

/// The file's contents (`None` = missing) → settings, plus a warning when
/// the file was corrupt. Either fallback gets a fresh passphrase.
pub fn or_default(text: Option<&str>) -> (NetAudioConfig, Option<String>) {
    match text.map(from_json) {
        None => (NetAudioConfig::fresh(), None),
        Some(Ok(config)) => (config, None),
        Some(Err(err)) => (NetAudioConfig::fresh(), Some(format!("{err}; network audio is off"))),
    }
}

pub fn load(path: &Path) -> NetAudioConfig {
    let text = persist::read_optional(path);
    let (config, warning) = or_default(text.as_deref());
    if let Some(warning) = warning {
        log::warn!("{warning} ({})", path.display());
    }
    config
}

pub fn save(path: &Path, config: &NetAudioConfig) -> Result<(), String> {
    persist::write_atomic(path, &to_json(config)?)
}

/// Managed Tauri state: the settings used at the next engine start.
pub struct NetAudioSettings(Mutex<NetAudioConfig>);

impl NetAudioSettings {
    pub fn new() -> Self {
        NetAudioSettings(Mutex::new(NetAudioConfig::default()))
    }

    pub fn snapshot(&self) -> NetAudioConfig {
        match self.0.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn replace(&self, next: NetAudioConfig) {
        match self.0.lock() {
            Ok(mut guard) => *guard = next,
            Err(poisoned) => *poisoned.into_inner() = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_every_field() {
        let raw = NetAudioConfig {
            receive: ReceiveConfig { enabled: true, port: 0, slots: 9, buffer_ms: 1 },
            send: SendConfig {
                enabled: true,
                target: "  church.local:47810\u{7} ".to_string(),
                bus: " STREAM ".to_string(),
                stereo: true,
                name: "x".repeat(50),
            },
            passphrase: " secret-room ".to_string(),
        };
        let clean = raw.sanitized();
        assert_eq!(clean.receive.port, DEFAULT_PORT);
        assert_eq!(clean.receive.slots, MAX_NET_INPUTS);
        assert_eq!(clean.receive.buffer_ms, MIN_BUFFER_MS);
        assert_eq!(clean.send.target, "church.local:47810");
        assert_eq!(clean.send.bus, "stream");
        assert_eq!(clean.send.name.chars().count(), MAX_NAME_CHARS);
        assert_eq!(clean.passphrase, "secret-room");
        let unknown_bus = SendConfig { bus: "aux".to_string(), ..SendConfig::default() };
        let config = NetAudioConfig { send: unknown_bus, ..NetAudioConfig::default() };
        assert_eq!(config.sanitized().send.bus, "main");
    }

    #[test]
    fn a_short_passphrase_keeps_network_audio_off() {
        let mut config = NetAudioConfig::default();
        config.receive.enabled = true;
        config.send.enabled = true;
        config.send.target = "10.0.0.2:47810".to_string();
        config.passphrase = "short".to_string();
        assert!(config.passphrase_problem().is_some());
        assert!(!config.receive_ready() && !config.send_ready());
        assert_eq!(config.strips(), 0);
        config.passphrase = "long-enough".to_string();
        assert!(config.receive_ready() && config.send_ready());
        assert_eq!(config.strips(), DEFAULT_SLOTS);
    }

    #[test]
    fn sending_needs_a_target() {
        let mut config = NetAudioConfig::fresh();
        config.send.enabled = true;
        assert!(!config.send_ready());
    }

    #[test]
    fn generated_passphrases_are_long_and_differ() {
        let a = generate_passphrase();
        let b = generate_passphrase();
        assert_eq!(a.len(), 14);
        assert_eq!(a.matches('-').count(), 2);
        assert_ne!(a, b);
        assert!(NetAudioConfig { passphrase: a, ..NetAudioConfig::default() }
            .passphrase_problem()
            .is_none());
    }

    #[test]
    fn file_round_trips_and_falls_back() {
        let config = NetAudioConfig::fresh();
        let text = to_json(&config).unwrap();
        assert!(text.contains("\"bufferMs\""));
        assert_eq!(from_json(&text).unwrap(), config);
        assert_eq!(from_json("{}").unwrap(), NetAudioConfig::default());
        for bad in ["", "{", "[1]", r#"{"receive":{"port":"x"}}"#] {
            let (config, warning) = or_default(Some(bad));
            assert!(warning.is_some(), "{bad:?}");
            assert!(!config.receive.enabled && !config.passphrase.is_empty());
        }
        let (missing, warning) = or_default(None);
        assert!(warning.is_none() && !missing.passphrase.is_empty());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("tiwaton-net-test-{}", std::process::id()));
        let path = dir.join(SETTINGS_FILE);
        let mut config = NetAudioConfig::fresh();
        config.receive.enabled = true;
        save(&path, &config).unwrap();
        assert_eq!(load(&path), config);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
