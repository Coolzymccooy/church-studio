use super::Role;
use crate::link::config::constant_time_eq;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(super) const COOKIE: &str = "__Host-tiwaton_remote";
const CODE_TTL: Duration = Duration::from_secs(120);
const DEVICE_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const CODE_ATTEMPTS: u8 = 8;
const MAX_DEVICES: usize = 32;

struct PairCode {
    code: String,
    role: Role,
    expires: Instant,
    remaining: u8,
}

#[derive(Clone, Serialize)]
pub struct Device {
    pub id: u32,
    name: String,
    role: Role,
    #[serde(skip)]
    expires: Instant,
}

#[derive(Default)]
pub struct Auth {
    pair: Option<PairCode>,
    devices: HashMap<[u8; 32], Device>,
    next_id: u32,
}

pub(super) enum PairError {
    Code,
    Name,
    Full,
    Random,
}

fn secret<const N: usize>() -> Result<[u8; N], PairError> {
    let mut value = [0; N];
    getrandom::getrandom(&mut value).map_err(|_| PairError::Random)?;
    Ok(value)
}

fn pairing_code() -> Result<String, PairError> {
    const RANGE: u32 = 1_000_000;
    loop {
        let number = u32::from_le_bytes(secret()?);
        if number < u32::MAX - u32::MAX % RANGE {
            return Ok(format!("{:06}", number % RANGE));
        }
    }
}

impl Auth {
    pub fn issue(&mut self, role: Role) -> Result<String, String> {
        let code = pairing_code().map_err(|_| "Cannot create pairing code")?;
        self.pair = Some(PairCode {
            code: code.clone(),
            role,
            expires: Instant::now() + CODE_TTL,
            remaining: CODE_ATTEMPTS,
        });
        Ok(code)
    }

    pub fn cancel(&mut self) {
        self.pair = None;
    }

    pub fn devices(&mut self) -> Vec<Device> {
        self.prune();
        let mut devices: Vec<_> = self.devices.values().cloned().collect();
        devices.sort_by_key(|device| device.id);
        devices
    }

    pub fn revoke(&mut self, id: u32) {
        self.devices.retain(|_, device| device.id != id);
    }

    fn prune(&mut self) {
        let now = Instant::now();
        self.devices.retain(|_, device| device.expires > now);
    }

    pub(super) fn pair(&mut self, code: &str, name: &str) -> Result<String, PairError> {
        let pair = self.pair.as_mut().ok_or(PairError::Code)?;
        if pair.expires <= Instant::now() || pair.remaining == 0 {
            self.pair = None;
            return Err(PairError::Code);
        }
        pair.remaining -= 1;
        if !constant_time_eq(pair.code.as_bytes(), code.as_bytes()) {
            return Err(PairError::Code);
        }
        let role = pair.role;
        let name: String = name
            .trim()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(24)
            .collect();
        if name.is_empty() {
            return Err(PairError::Name);
        }
        self.prune();
        if self.devices.len() >= MAX_DEVICES {
            return Err(PairError::Full);
        }
        let token: String = secret::<32>()?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.next_id = self.next_id.checked_add(1).ok_or(PairError::Full)?;
        self.devices.insert(
            Sha256::digest(token.as_bytes()).into(),
            Device {
                id: self.next_id,
                name,
                role,
                expires: Instant::now() + DEVICE_TTL,
            },
        );
        self.pair = None;
        Ok(format!(
            "{COOKIE}={token}; Path=/; Max-Age={}; Secure; HttpOnly; SameSite=Strict",
            DEVICE_TTL.as_secs()
        ))
    }

    pub(super) fn role(&self, cookie: &str) -> Option<Role> {
        let prefix = format!("{COOKIE}=");
        let mut tokens = cookie
            .split(';')
            .filter_map(|part| part.trim().strip_prefix(&prefix));
        let token = tokens.next()?;
        if tokens.next().is_some()
            || token.len() != 64
            || !token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.devices
            .get(&digest)
            .filter(|device| device.expires > Instant::now())
            .map(|device| device.role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_expire_and_lock_after_eight_failed_attempts() {
        let mut auth = Auth::default();
        let code = auth.issue(Role::Desk).unwrap();
        for _ in 0..CODE_ATTEMPTS {
            assert!(matches!(
                auth.pair("invalid", "Fixture"),
                Err(PairError::Code)
            ));
        }
        assert!(matches!(auth.pair(&code, "Fixture"), Err(PairError::Code)));
        let code = auth.issue(Role::Desk).unwrap();
        auth.pair.as_mut().unwrap().expires = Instant::now();
        assert!(matches!(auth.pair(&code, "Fixture"), Err(PairError::Code)));
    }
}
