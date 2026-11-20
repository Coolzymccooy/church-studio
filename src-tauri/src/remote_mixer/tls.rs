use super::Auth;
use axum::{Extension, Router};
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use parking_lot::Mutex;
use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, TcpListener};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::{net::TcpStream, task::JoinSet, time::timeout};
use tokio_rustls::TlsAcceptor;

const MAX_CONNECTIONS: usize = 64;
const RESERVED_CONNECTIONS: usize = 16;
const PER_IP: usize = 4;
const MAX_PEM_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListenerSettings {
    pub ip: String,
    pub port: u16,
    pub host: String,
    pub certificate: String,
    pub private_key: String,
}

pub(super) struct Prepared {
    pub host: String,
    pub listener: TcpListener,
    pub tls: TlsAcceptor,
    pub fingerprint: String,
}

fn private(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    ip.is_private() || ip.is_loopback() || (a == 100 && (64..128).contains(&b))
}

pub(super) fn valid_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn pem(path: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let path = Path::new(path);
    let meta = path.metadata().map_err(|_| "Cannot read TLS file")?;
    if !meta.is_file() || meta.len() > MAX_PEM_BYTES {
        return Err("Invalid TLS file".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "Cannot open TLS file")?;
    let mut data = Vec::new();
    file.take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| "Cannot read TLS file")?;
    if data.len() as u64 > MAX_PEM_BYTES {
        return Err("Invalid TLS file".into());
    }
    Ok(data)
}

impl ListenerSettings {
    pub(super) fn prepare(self) -> Result<Prepared, String> {
        let ip: Ipv4Addr = self
            .ip
            .trim()
            .parse()
            .map_err(|_| "Enter a local IPv4 address")?;
        if !private(ip) || self.port == 0 {
            return Err("Enter a local address and port".into());
        }
        let host = self.host.trim().to_ascii_lowercase();
        if !valid_hostname(&host) {
            return Err("Enter the certificate hostname".into());
        }
        let host = if self.port == 443 {
            host
        } else {
            format!("{host}:{}", self.port)
        };
        let certs: Vec<_> = CertificateDer::pem_slice_iter(&pem(&self.certificate)?)
            .collect::<Result<_, _>>()
            .map_err(|_| "Invalid TLS certificate")?;
        let leaf = certs.first().ok_or("Missing TLS certificate")?;
        let fingerprint = Sha256::digest(leaf.as_ref())
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let key = PrivateKeyDer::from_pem_slice(&pem(&self.private_key)?)
            .map_err(|_| "Invalid TLS private key")?;
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| "TLS unavailable")?
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|_| "TLS certificate and key rejected")?;
        let listener = TcpListener::bind(SocketAddrV4::new(ip, self.port))
            .map_err(|_| "Cannot bind remote mixer")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Cannot configure remote mixer listener")?;
        Ok(Prepared {
            host,
            listener,
            tls: TlsAcceptor::from(Arc::new(config)),
            fingerprint,
        })
    }
}

#[derive(Default)]
struct Connections {
    counts: HashMap<IpAddr, usize>,
    total: usize,
}

impl Connections {
    fn acquire(&mut self, ip: IpAddr, paired: bool) -> bool {
        let limit = if paired {
            MAX_CONNECTIONS
        } else {
            MAX_CONNECTIONS - RESERVED_CONNECTIONS
        };
        if self.total >= limit || self.counts.get(&ip).copied().unwrap_or(0) >= PER_IP {
            return false;
        }
        *self.counts.entry(ip).or_default() += 1;
        self.total += 1;
        true
    }

    fn release(&mut self, ip: IpAddr) {
        self.total -= 1;
        if let Some(count) = self.counts.get_mut(&ip) {
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&ip);
            }
        }
    }
}

struct Slot(Arc<Mutex<Connections>>, IpAddr);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.lock().release(self.1);
    }
}

pub(super) async fn serve(
    listener: tokio::net::TcpListener,
    tls: TlsAcceptor,
    app: Router,
    auth: Arc<Mutex<Auth>>,
) {
    let mut clients = JoinSet::new();
    let connections = Arc::new(Mutex::new(Connections::default()));
    loop {
        tokio::select! {
            _ = clients.join_next(), if !clients.is_empty() => {}
            result = listener.accept() => {
                let Ok((socket, peer)) = result else { break };
                let ip = peer.ip();
                let paired = auth.lock().paired_ip(ip);
                if !connections.lock().acquire(ip, paired) {
                    continue;
                }
                let slot = Slot(connections.clone(), ip);
                let _ = socket.set_nodelay(true);
                let tls = tls.clone();
                let app = app.clone().layer(Extension(ip));
                clients.spawn(async move {
                    let _slot = slot;
                    connection(socket, tls, app).await;
                });
            }
        }
    }
}

async fn connection(socket: TcpStream, tls: TlsAcceptor, app: Router) {
    let Ok(Ok(stream)) = timeout(Duration::from_secs(5), tls.accept(socket)).await else {
        return;
    };
    let mut http = http1::Builder::new();
    http.timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(10))
        .max_buf_size(8192);
    let service = TowerToHyperService::new(app);
    let _ = http.serve_connection(TokioIo::new(stream), service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_ip_is_capped_and_paired_clients_have_reserved_slots() {
        let mut connections = Connections::default();
        let address = |last| IpAddr::V4(Ipv4Addr::new(192, 168, 0, last));
        for last in 1..=12 {
            for _ in 0..PER_IP {
                assert!(connections.acquire(address(last), false));
            }
            assert!(!connections.acquire(address(last), true));
        }
        assert!(!connections.acquire(address(13), false));
        for last in 13..=16 {
            for _ in 0..PER_IP {
                assert!(connections.acquire(address(last), true));
            }
        }
        assert!(!connections.acquire(address(17), true));
        connections.release(address(13));
        assert!(connections.acquire(address(17), true));
        assert!(!connections.acquire(address(18), false));
    }
}
