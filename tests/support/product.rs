use std::fs;
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;

use crate::support::process::ManagedProcess;
pub use crate::support::process::TestRoot;

pub fn required_binary(name: &str) -> Result<PathBuf> {
    let value = std::env::var_os(name).with_context(|| format!("{name} is not set"))?;
    let path = PathBuf::from(value);
    ensure!(
        path.is_file(),
        "{name} does not name a file: {}",
        path.display()
    );
    Ok(path)
}

pub struct HtreeNode {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

pub struct NodeConfig<'a> {
    pub http_addr: &'a str,
    pub udp_addr: &'a str,
    pub rendezvous_addr: &'a str,
    pub peers: &'a [(&'a str, &'a str)],
}

impl HtreeNode {
    pub fn new(root: &Path, name: &str) -> Self {
        Self {
            config_dir: root.join(format!("{name}-config")),
            data_dir: root.join(format!("{name}-data")),
        }
    }

    pub fn write_config(&self, config: NodeConfig<'_>) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.data_dir)?;
        let peers = config
            .peers
            .iter()
            .map(|(npub, addr)| {
                format!("{{ npub = \"{npub}\", udp_addresses = [\"udp:{addr}\"] }}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let body = format!(
            r#"[storage]
data_dir = "{}"
max_size_gb = 1
evict_orphans = false

[server]
bind_address = "{}"
enable_auth = false
stun_port = 0
enable_webrtc = false
enable_fips = true
fips_discovery_scope = "fips-overlay-v1"
fips_relays = []
fips_peers = [{}]
enable_fips_udp = true
fips_udp_bind_addr = "{}"
fips_udp_public = false
fips_local_rendezvous_addr = "{}"
enable_fips_webrtc = false
enable_fips_lan_discovery = false
fips_ethernet_interfaces = []
fetch_from_fips_peers = true
fips_request_timeout_ms = 2500
enable_multicast = false
enable_bluetooth = false

[nostr]
enabled = false
relays = []
bootstrap_follows = []

[blossom]
enabled = false
servers = []
read_servers = ["http://127.0.0.1:1"]
write_servers = []

[updater]
auto_check = false
"#,
            toml_path(&self.data_dir),
            config.http_addr,
            peers,
            config.udp_addr,
            config.rendezvous_addr,
        );
        fs::write(self.config_dir.join("config.toml"), body).context("write htree config")
    }

    fn command(&self, htree_bin: &Path) -> Command {
        let mut command = Command::new(htree_bin);
        let requested_log =
            std::env::var("IRIS_STACK_PRODUCT_LOG").unwrap_or_else(|_| "warn".to_string());
        command
            .env("HTREE_CONFIG_DIR", &self.config_dir)
            .env(
                "RUST_LOG",
                format!(
                    "{requested_log},fips_core::discovery::lan=info,\
                     fips_core::node::lifecycle::runtime=info"
                ),
            )
            .arg("--data-dir")
            .arg(&self.data_dir);
        command
    }
}

pub async fn htree_identity(htree_bin: &Path, node: &HtreeNode) -> Result<String> {
    let output = node
        .command(htree_bin)
        .arg("user")
        .output()
        .await
        .context("run htree user")?;
    ensure!(
        output.status.success(),
        "htree user failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)?
        .lines()
        .find(|line| line.starts_with("npub1"))
        .and_then(|line| line.split_whitespace().next())
        .map(str::to_string)
        .context("htree user did not print an npub")
}

pub struct DriveIdentity {
    pub npub: String,
    pub profile_id: String,
    pub discovery_scope: String,
}

pub async fn drive_identity(drive_bin: &Path, key_path: &Path) -> Result<DriveIdentity> {
    let output = Command::new(drive_bin)
        .arg("identity")
        .arg(key_path)
        .output()
        .await
        .context("generate Iris Drive fixture identity")?;
    ensure!(
        output.status.success(),
        "Drive identity fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)
        .context("Drive identity fixture did not return JSON")?;
    Ok(DriveIdentity {
        npub: value["npub"]
            .as_str()
            .context("Drive identity fixture omitted npub")?
            .to_string(),
        profile_id: value["profile_id"]
            .as_str()
            .context("Drive identity fixture omitted profile id")?
            .to_string(),
        discovery_scope: value["discovery_scope"]
            .as_str()
            .context("Drive identity fixture omitted discovery scope")?
            .to_string(),
    })
}

pub async fn add_blob(
    htree_bin: &Path,
    node: &HtreeNode,
    name: &str,
    bytes: &[u8],
) -> Result<String> {
    let path = node
        .data_dir
        .parent()
        .context("node data parent")?
        .join(name);
    fs::write(&path, bytes).context("write blob fixture")?;
    let output = node
        .command(htree_bin)
        .arg("add")
        .arg(&path)
        .arg("--unencrypted")
        .output()
        .await
        .context("run htree add")?;
    ensure!(
        output.status.success(),
        "htree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)?
        .lines()
        .find_map(|line| line.trim().strip_prefix("hash:  ").map(str::to_string))
        .context("htree add did not print a hash")
}

pub async fn cat_blob(htree_bin: &Path, node: &HtreeNode, cid: &str) -> Result<Vec<u8>> {
    let output = node
        .command(htree_bin)
        .arg("cat")
        .arg(cid)
        .output()
        .await
        .context("run htree cat")?;
    ensure!(
        output.status.success(),
        "htree cat failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

pub fn spawn_htree(
    htree_bin: &Path,
    node: &HtreeNode,
    http_addr: &str,
    label: &str,
) -> Result<ManagedProcess> {
    let mut command = node.command(htree_bin);
    command.arg("start").arg("--addr").arg(http_addr);
    ManagedProcess::spawn(label, &mut command)
}

pub async fn wait_for_fips_peer_connection(http_addr: &str, npub: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    while tokio::time::Instant::now() < deadline {
        match fetch_status(http_addr).await {
            Ok(status) => {
                if fips_udp_peer_connected(&status, npub) {
                    return Ok(());
                }
                last = status.to_string();
            }
            Err(error) => last = format!("{error:#}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    bail!("provider did not authenticate its remote UDP FIPS route; last status: {last}")
}

pub fn fips_udp_peer_connected(status: &Value, npub: &str) -> bool {
    status["fips"]["peer_statuses"]
        .as_array()
        .is_some_and(|peers| {
            peers.iter().any(|peer| {
                peer["npub"] == npub && peer["connected"] == true && peer["transport_type"] == "udp"
            })
        })
}

pub async fn fetch_status(addr: &str) -> Result<Value> {
    let mut stream = TcpStream::connect(addr).await?;
    stream
        .write_all(
            format!("GET /api/status HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("HTTP status response omitted headers")?;
    let headers = String::from_utf8_lossy(&response[..split]);
    ensure!(headers.starts_with("HTTP/1.1 200") || headers.starts_with("HTTP/1.0 200"));
    serde_json::from_slice(&response[split + 4..]).context("decode htree status JSON")
}

pub fn payload(label: &str, len: usize) -> Vec<u8> {
    label.as_bytes().iter().copied().cycle().take(len).collect()
}

pub fn reserve_udp_address() -> Result<String> {
    Ok(UdpSocket::bind("127.0.0.1:0")?.local_addr()?.to_string())
}

pub fn reserve_tcp_address() -> Result<String> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.to_string())
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
