use super::Role;
use crate::link::config::constant_time_eq;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::IpAddr,
    time::{Duration, Instant},
};

const COOKIE: &str = "__Host-tiwaton_remote";
const CODE_TTL: Duration = Duration::from_secs(120);
const DEVICE_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const MAX_DEVICES: usize = 32;
const MAX_FAILURE_RECORDS: usize = 256;
const WRITE_RATE: f32 = 30.0;
const WRITE_BURST: f32 = 8.0;

struct PairCode {
    code: String,
    role: Role,
    expires: Instant,
}

struct Failure {
    strikes: u8,
    until: Instant,
    last: Instant,
}

#[derive(Clone, Serialize)]
pub struct Device {
    pub id: u32,
    name: String,
    role: Role,
    #[serde(skip)]
    expires: Instant,
    #[serde(skip)]
    ip: IpAddr,
    #[serde(skip)]
    renewable: bool,
    #[serde(skip)]
    tokens: f32,
    #[serde(skip)]
    last_write: Instant,
}

#[derive(Default)]
pub struct Auth {
    pair: Option<PairCode>,
    devices: HashMap<[u8; 32], Device>,
    failures: HashMap<IpAddr, Failure>,
    next_id: u32,
}

#[derive(Debug, PartialEq)]
pub(super) enum PairError {
    Code,
    Name,
    Full,
    Random,
    Backoff,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStatus {
    pub state: &'static str,
    pub remaining_seconds: u64,
    pub locked_clients: usize,
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

fn token(cookie: &str) -> Option<&str> {
    let mut matches = cookie.split(';').filter_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == COOKIE).then_some(value)
    });
    let value = matches.next()?;
    (matches.next().is_none() && value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(value)
}

fn digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn set_cookie(token: &str) -> String {
    format!(
        "{COOKIE}={token}; Path=/; Max-Age={}; Secure; HttpOnly; SameSite=Strict",
        DEVICE_TTL.as_secs()
    )
}

impl Auth {
    pub fn issue(&mut self, role: Role) -> Result<String, String> {
        let code = pairing_code().map_err(|_| "Cannot create pairing code")?;
        self.pair = Some(PairCode {
            code: code.clone(),
            role,
            expires: Instant::now() + CODE_TTL,
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

    pub fn pair_status(&self) -> PairStatus {
        let now = Instant::now();
        let locked_clients = self
            .failures
            .values()
            .filter(|entry| entry.until > now)
            .count();
        let (state, remaining) = match &self.pair {
            None => ("idle", Duration::ZERO),
            Some(code) if code.expires <= now => ("expired", Duration::ZERO),
            Some(code) => ("ready", code.expires - now),
        };
        PairStatus {
            state,
            remaining_seconds: remaining.as_secs(),
            locked_clients,
        }
    }

    pub(super) fn paired_ip(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        self.devices
            .values()
            .any(|device| device.ip == ip && device.expires > now)
    }

    fn prune(&mut self) {
        let now = Instant::now();
        self.devices.retain(|_, device| device.expires > now);
    }

    pub(super) fn pair(
        &mut self,
        code: &str,
        name: &str,
        ip: IpAddr,
        renewable: bool,
    ) -> Result<String, PairError> {
        let now = Instant::now();
        self.failures
            .retain(|_, entry| now.duration_since(entry.last) < Duration::from_secs(600));
        if self
            .failures
            .get(&ip)
            .is_some_and(|entry| entry.until > now)
        {
            return Err(PairError::Backoff);
        }
        let valid = self
            .pair
            .as_ref()
            .filter(|pair| pair.expires > now)
            .filter(|pair| constant_time_eq(pair.code.as_bytes(), code.as_bytes()));
        let Some(role) = valid.map(|pair| pair.role) else {
            self.record_failure(ip, now);
            return Err(PairError::Code);
        };
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
        let value: String = secret::<32>()?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.next_id = self.next_id.checked_add(1).ok_or(PairError::Full)?;
        self.devices.insert(
            digest(&value),
            Device {
                id: self.next_id,
                name,
                role,
                expires: now + DEVICE_TTL,
                ip,
                renewable,
                tokens: WRITE_BURST,
                last_write: now,
            },
        );
        self.pair = None;
        self.failures.remove(&ip);
        Ok(set_cookie(&value))
    }

    fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        if self.failures.len() >= MAX_FAILURE_RECORDS && !self.failures.contains_key(&ip) {
            if let Some(oldest) = self
                .failures
                .iter()
                .min_by_key(|(_, entry)| entry.last)
                .map(|(ip, _)| *ip)
            {
                self.failures.remove(&oldest);
            }
        }
        let entry = self.failures.entry(ip).or_insert(Failure {
            strikes: 0,
            until: now,
            last: now,
        });
        entry.strikes = entry.strikes.saturating_add(1);
        entry.last = now;
        if entry.strikes >= 3 {
            let seconds = (2u64 << entry.strikes.saturating_sub(3).min(5)).min(60);
            entry.until = now + Duration::from_secs(seconds);
        }
    }

    fn device_mut(&mut self, cookie: &str, ip: IpAddr) -> Option<&mut Device> {
        let device = self.devices.get_mut(&digest(token(cookie)?))?;
        if device.expires <= Instant::now() {
            return None;
        }
        device.ip = ip;
        Some(device)
    }

    pub(super) fn role(&mut self, cookie: &str, ip: IpAddr) -> Option<Role> {
        self.device_mut(cookie, ip).map(|device| device.role)
    }

    pub(super) fn write_role(&mut self, cookie: &str, ip: IpAddr) -> Result<Role, super::Status> {
        let device = self
            .device_mut(cookie, ip)
            .ok_or(super::Status::UNAUTHORIZED)?;
        let now = Instant::now();
        device.tokens = (device.tokens
            + now.duration_since(device.last_write).as_secs_f32() * WRITE_RATE)
            .min(WRITE_BURST);
        device.last_write = now;
        if device.tokens < 1.0 {
            return Err(super::Status::TOO_MANY_REQUESTS);
        }
        device.tokens -= 1.0;
        Ok(device.role)
    }

    pub(super) fn renew(&mut self, cookie: &str, ip: IpAddr) -> Option<String> {
        let device = self.device_mut(cookie, ip)?;
        if !device.renewable {
            return None;
        }
        device.expires = Instant::now() + DEVICE_TTL;
        Some(set_cookie(token(cookie)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bound_to_ip_and_survives_code_rotation() {
        let mut auth = Auth::default();
        let ip = "127.0.0.1".parse().unwrap();
        let other = "127.0.0.2".parse().unwrap();
        let _ = auth.issue(Role::Desk).unwrap();
        for _ in 0..3 {
            assert_eq!(
                auth.pair("invalid", "Fixture", ip, false),
                Err(PairError::Code)
            );
        }
        assert_eq!(auth.pair_status().state, "ready");
        assert_eq!(auth.pair_status().locked_clients, 1);
        let code = auth.issue(Role::Monitor2).unwrap();
        assert_eq!(
            auth.pair(&code, "Fixture", ip, false),
            Err(PairError::Backoff)
        );
        let cookie = auth.pair(&code, "Fixture", other, true).unwrap();
        assert_eq!(auth.pair_status().locked_clients, 1);
        assert!(auth.renew(&cookie, other).is_some());
        for _ in 0..8 {
            assert_eq!(auth.write_role(&cookie, other), Ok(Role::Monitor2));
        }
        assert_eq!(
            auth.write_role(&cookie, other),
            Err(super::super::Status::TOO_MANY_REQUESTS)
        );
        let id = auth.devices()[0].id;
        auth.revoke(id);
        assert!(auth.renew(&cookie, other).is_none());
    }

    #[test]
    fn an_expired_code_is_reported_and_renewal_is_opt_in() {
        let mut auth = Auth::default();
        let ip = "127.0.0.1".parse().unwrap();
        let code = auth.issue(Role::Desk).unwrap();
        auth.pair.as_mut().unwrap().expires = Instant::now();
        assert_eq!(auth.pair_status().state, "expired");
        assert_eq!(auth.pair(&code, "Fixture", ip, false), Err(PairError::Code));
        let code = auth.issue(Role::Desk).unwrap();
        let cookie = auth.pair(&code, "Fixture", ip, false).unwrap();
        assert!(auth.renew(&cookie, ip).is_none());
    }

    #[test]
    fn a_full_failure_table_still_accepts_new_clients_and_evicts_the_oldest_failure() {
        use std::net::Ipv4Addr;
        let mut auth = Auth::default();
        let code = auth.issue(Role::Desk).unwrap();
        let now = Instant::now();
        let ip =
            |index: usize| IpAddr::V4(Ipv4Addr::new(192, 168, (index / 256) as u8, index as u8));
        for index in 0..MAX_FAILURE_RECORDS {
            auth.record_failure(ip(index), now + Duration::from_nanos(index as u64));
        }
        let next = ip(MAX_FAILURE_RECORDS);
        assert_eq!(
            auth.pair("invalid", "Fixture", next, false),
            Err(PairError::Code)
        );
        assert_eq!(auth.failures.len(), MAX_FAILURE_RECORDS);
        assert!(!auth.failures.contains_key(&ip(0)));
        assert!(auth.failures.contains_key(&next));
        assert!(auth
            .pair(&code, "Fixture", ip(MAX_FAILURE_RECORDS + 1), false)
            .is_ok());
    }
}
