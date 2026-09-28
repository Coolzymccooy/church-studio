//! Tiwaton Link settings, stored in `app_data_dir()/link.json`:
//! the per-install token, the enabled flag and the automation rules.
//!
//! The token is 32 random bytes as lowercase hex. It is never logged: the
//! `Debug` impl redacts it.
use crate::link::rules::{default_rules, validate_rules, Rule};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

pub const CONFIG_FILE: &str = "link.json";
pub const TOKEN_BYTES: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rules")]
    pub rules: Vec<Rule>,
}

fn default_true() -> bool {
    true
}

impl fmt::Debug for LinkConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkConfig")
            .field("token", &"<redacted>")
            .field("enabled", &self.enabled)
            .field("rules", &self.rules.len())
            .finish()
    }
}

impl LinkConfig {
    /// A new install: fresh token, enabled, default rules.
    pub fn fresh() -> Result<Self, String> {
        Ok(LinkConfig {
            token: generate_token()?,
            enabled: true,
            rules: default_rules(),
        })
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn generate_token() -> Result<String, String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("cannot generate a token: {e}"))?;
    Ok(to_hex(&bytes))
}

pub fn is_valid_token(token: &str) -> bool {
    token.len() == TOKEN_BYTES * 2 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Compare secrets without an early exit on the first differing byte.
/// (The length is not secret: every token is 64 characters.)
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    diff == 0
}

/// Parse a stored config, repairing what can be repaired: a bad or missing
/// token is regenerated and invalid rules fall back to the defaults.
/// Returns the config and whether it changed (so the caller re-saves it).
pub fn parse_config(text: &str) -> Result<(LinkConfig, bool), String> {
    let parsed: LinkConfig =
        serde_json::from_str(text).map_err(|e| format!("link.json is not valid: {e}"))?;
    let mut changed = false;
    let token = if is_valid_token(&parsed.token) {
        parsed.token.clone()
    } else {
        changed = true;
        generate_token()?
    };
    let rules = match validate_rules(&parsed.rules) {
        Ok(rules) => rules,
        Err(e) => {
            log::warn!("link: stored rules are invalid ({e}); using the defaults");
            changed = true;
            default_rules()
        }
    };
    Ok((
        LinkConfig {
            token,
            enabled: parsed.enabled,
            rules,
        },
        changed,
    ))
}

/// Load `path`, creating it with a fresh config when it is missing or
/// unreadable (an unreadable file is kept as `link.json.bad`).
pub fn load_or_create(path: &Path) -> Result<LinkConfig, String> {
    let loaded = match fs::read_to_string(path) {
        Ok(text) => match parse_config(&text) {
            Ok(result) => Some(result),
            Err(e) => {
                log::warn!("link: {e}; starting with a fresh config");
                let _ = fs::rename(path, path.with_extension("json.bad"));
                None
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read link.json: {e}")),
    };
    let (config, changed) = match loaded {
        Some(result) => result,
        None => (LinkConfig::fresh()?, true),
    };
    if changed {
        save(path, &config)?;
    }
    Ok(config)
}

/// Create `path` readable by the owner only on Unix (it holds the token).
/// On Windows the app data folder's inherited ACL is already per-user.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

/// Write via a temp file and rename, so a crash never leaves half a file.
pub fn save(path: &Path, config: &LinkConfig) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create the settings folder: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    // A leftover temp file would keep its old (possibly wider) permissions.
    let _ = fs::remove_file(&tmp);
    write_private(&tmp, json.as_bytes()).map_err(|e| format!("cannot write link.json: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot save link.json: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_hex_chars_and_unique() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        assert!(is_valid_token(&a));
        assert_ne!(a, b);
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }

    #[test]
    fn constant_time_eq_behaves() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn debug_redacts_token() {
        let config = LinkConfig::fresh().unwrap();
        let text = format!("{config:?}");
        assert!(!text.contains(&config.token));
        assert!(text.contains("redacted"));
    }

    #[test]
    fn parse_repairs_token_and_rules() {
        let (config, changed) = parse_config(r#"{"enabled": false}"#).unwrap();
        assert!(changed);
        assert!(is_valid_token(&config.token));
        assert!(!config.enabled);
        assert_eq!(config.rules, default_rules());

        let good = LinkConfig::fresh().unwrap();
        let text = serde_json::to_string(&good).unwrap();
        let (again, changed) = parse_config(&text).unwrap();
        assert!(!changed);
        assert_eq!(again.token, good.token);

        let bad_rules = format!(
            r#"{{"token":"{}","rules":[{{"id":"","when":{{"event":"x"}},"then":{{"loadScene":"A"}}}}]}}"#,
            good.token
        );
        let (fixed, changed) = parse_config(&bad_rules).unwrap();
        assert!(changed);
        assert_eq!(fixed.rules, default_rules());
    }

    #[test]
    fn load_or_create_round_trip() {
        let dir = std::env::temp_dir().join(format!("tiwaton-link-{}", generate_token().unwrap()));
        let path = dir.join(CONFIG_FILE);
        let first = load_or_create(&path).unwrap();
        let second = load_or_create(&path).unwrap();
        assert_eq!(first.token, second.token);

        fs::write(&path, "{broken").unwrap();
        let third = load_or_create(&path).unwrap();
        assert_ne!(third.token, first.token);
        assert!(path.with_extension("json.bad").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
