use super::tls::ListenerSettings;
use serde::Deserialize;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

pub(super) fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|_| "Cannot find app data".into())
}

pub(super) fn load(app: &AppHandle) -> Option<ListenerSettings> {
    let data = std::fs::read(data_dir(app).ok()?.join("remote-mixer.json")).ok()?;
    serde_json::from_slice(&data).ok()
}

pub(super) fn save(dir: &Path, settings: &ListenerSettings) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|_| "Cannot create app data folder")?;
    let data = serde_json::to_vec(settings).map_err(|_| "Cannot encode listener settings")?;
    let staging = dir.join("remote-mixer.json.tmp");
    std::fs::write(&staging, data).map_err(|_| "Cannot save listener settings")?;
    std::fs::rename(staging, dir.join("remote-mixer.json"))
        .map_err(|_| "Cannot replace listener settings".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TailnetIdentity {
    #[serde(rename = "DNSName")]
    host: String,
    #[serde(rename = "TailscaleIPs")]
    addresses: Vec<std::net::IpAddr>,
}

#[derive(Deserialize)]
struct Status {
    #[serde(rename = "Self")]
    own: Option<TailnetIdentity>,
    #[serde(rename = "BackendState")]
    backend: String,
}

fn tailscale() -> Command {
    let candidates = [
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
        r"C:\Program Files\Tailscale\tailscale.exe",
    ];
    let executable = candidates
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .unwrap_or("tailscale");
    let mut cmd = Command::new(executable);
    cmd.env("TAILSCALE_BE_CLI", "1").kill_on_drop(true);
    cmd
}

#[derive(serde::Serialize)]
pub struct TailnetAddress {
    pub host: String,
    pub ip: Ipv4Addr,
}

fn tailnet_address(status: Status) -> Option<TailnetAddress> {
    if status.backend != "Running" {
        return None;
    }
    let identity = status.own?;
    let host = identity.host.trim_end_matches('.').to_ascii_lowercase();
    if !host.ends_with(".ts.net") || !super::tls::valid_hostname(&host) {
        return None;
    }
    let ip = identity
        .addresses
        .into_iter()
        .find_map(|address| match address {
            std::net::IpAddr::V4(ip)
                if ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]) =>
            {
                Some(ip)
            }
            _ => None,
        })?;
    Some(TailnetAddress { host, ip })
}

pub(super) async fn detect() -> Option<TailnetAddress> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tailscale().args(["status", "--json"]).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    tailnet_address(serde_json::from_slice(&output.stdout).ok()?)
}

pub(super) async fn certificate(app: &AppHandle, port: u16) -> Result<ListenerSettings, String> {
    let address = detect().await.ok_or("Tailscale unavailable")?;
    let dir = data_dir(app)?.join("remote-tls");
    let (certificate, private_key) = tokio::task::spawn_blocking(move || tls_paths(&dir))
        .await
        .map_err(|_| "Cannot prepare TLS folder")??;
    let status = tokio::time::timeout(
        Duration::from_secs(60),
        tailscale()
            .arg("cert")
            .arg("--cert-file")
            .arg(&certificate)
            .arg("--key-file")
            .arg(&private_key)
            .arg(&address.host)
            .output(),
    )
    .await
    .map_err(|_| "Tailscale certificate timed out")?
    .map_err(|_| "Tailscale certificate unavailable")?;
    if !status.status.success() {
        return Err("Tailscale could not issue a certificate. Check HTTPS certificates in the Tailscale admin console.".into());
    }
    Ok(ListenerSettings {
        ip: address.ip.to_string(),
        host: address.host,
        port,
        certificate,
        private_key,
    })
}

fn tls_paths(dir: &Path) -> Result<(String, String), String> {
    std::fs::create_dir_all(dir).map_err(|_| "Cannot create TLS folder")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect TLS folder")?;
    }
    let encode = |path: PathBuf| {
        path.into_os_string()
            .into_string()
            .map_err(|_| "Invalid TLS path")
    };
    Ok((
        encode(dir.join("tailnet.crt"))?,
        encode(dir.join("tailnet.key"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_an_active_tailnet_ipv4_and_dns_name() {
        let parse = |state, host| {
            tailnet_address(serde_json::from_value(serde_json::json!({
            "BackendState": state, "Self": { "DNSName": host, "TailscaleIPs": ["fd7a::1", "100.80.1.2"] }
        })).unwrap())
        };
        assert!(parse("Stopped", "desk.example.ts.net.").is_none());
        assert!(parse("Running", "desk.example.invalid.").is_none());
        let address = parse("Running", "desk.example.ts.net.").unwrap();
        assert_eq!(address.host, "desk.example.ts.net");
        assert_eq!(address.ip, Ipv4Addr::new(100, 80, 1, 2));
    }

    #[test]
    fn listener_settings_replace_previous_values_and_store_paths_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = ListenerSettings {
            ip: "127.0.0.1".into(),
            port: 4475,
            host: "desk.example.test".into(),
            certificate: "/cert/path.pem".into(),
            private_key: "/key/path.pem".into(),
        };
        save(dir.path(), &settings).unwrap();
        settings.port = 4476;
        save(dir.path(), &settings).unwrap();
        let raw = std::fs::read(dir.path().join("remote-mixer.json")).unwrap();
        let saved: ListenerSettings = serde_json::from_slice(&raw).unwrap();
        assert_eq!(saved.port, 4476);
        assert_eq!(saved.private_key, "/key/path.pem");
    }
}
