use std::io::Write as _;
use std::net::{SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use fips_core::config::{PeerConfig, RoutingMode, TransportInstances, UdpConfig};
use fips_core::{Config, FipsEndpoint, FipsEndpointServiceReceiver, PeerIdentity};
use hashtree_core::{Store, StoreBlobRoute, sha256};
use hashtree_lmdb::{ConfiguredLmdbBlobStore, open_shared_lmdb_blob_store};
use hashtree_network::{BlobRouteEntry, BlobRouteIdentity, BlobRouter, BlobRouterConfig};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::timeout;

const EXTERNAL_SERVICE_PORT: u16 = 39_018;
const BLOB_BYTES: usize = 96 * 1024 + 37;
const SHARED_STORAGE_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;
const WAIT: Duration = Duration::from_secs(20);

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("external") => {
            let bind_addr = required(&mut args, "external UDP address")?;
            no_more(args)?;
            run_external(&bind_addr).await
        }
        Some("anchor") => {
            let product = ProductArgs::parse(&mut args)?;
            no_more(args)?;
            run_anchor(product).await
        }
        Some("provider") => {
            let product = ProductArgs::parse(&mut args)?;
            let data_dir = PathBuf::from(required(&mut args, "shared Hashtree data directory")?);
            let blob_phase = args.next();
            no_more(args)?;
            run_provider(product, &data_dir, blob_phase.as_deref()).await
        }
        Some("consumer") => {
            let product = ProductArgs::parse(&mut args)?;
            let data_dir = PathBuf::from(required(&mut args, "shared Hashtree data directory")?);
            let initial_state = args.next();
            no_more(args)?;
            run_consumer(product, &data_dir, initial_state.as_deref()).await
        }
        _ => bail!(
            "usage: iris-stack-lab external <udp-addr> | \
             anchor <rendezvous-addr> <external-npub> <external-addr> | \
             <provider|consumer> <rendezvous-addr> <external-npub> <external-addr> \
             <shared-hashtree-data-dir> [provider-phase|initial-state]"
        ),
    }
}

struct ProductArgs {
    rendezvous_addr: SocketAddrV4,
    external_npub: String,
    external_addr: SocketAddr,
}

impl ProductArgs {
    fn parse(args: &mut impl Iterator<Item = String>) -> Result<Self> {
        Ok(Self {
            rendezvous_addr: required(args, "rendezvous UDP address")?.parse()?,
            external_npub: required(args, "external npub")?,
            external_addr: required(args, "external UDP address")?.parse()?,
        })
    }
}

fn required(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String> {
    args.next().with_context(|| format!("missing {name}"))
}

fn no_more(mut args: impl Iterator<Item = String>) -> Result<()> {
    ensure!(args.next().is_none(), "too many arguments");
    Ok(())
}

async fn run_external(bind_addr: &str) -> Result<()> {
    let endpoint = Arc::new(bind_endpoint(external_config(bind_addr)).await?);
    let service = endpoint
        .register_service_receiver(EXTERNAL_SERVICE_PORT)
        .await?;
    report(format!("EXTERNAL_READY {}", endpoint.npub()));

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        let mut datagrams = Vec::new();
        tokio::select! {
            line = lines.next_line() => {
                if line?.as_deref().is_none_or(|line| line == "stop") {
                    break;
                }
            }
            received = service.recv_batch_into(&mut datagrams, 16) => {
                ensure!(received.is_some(), "external FSP service closed");
                for request in datagrams {
                    let message = std::str::from_utf8(request.data.as_slice())
                        .context("external probe was not UTF-8")?;
                    let (role, phase) = message
                        .split_once(':')
                        .context("external probe omitted role or phase")?;
                    ensure!(matches!(role, "anchor" | "provider" | "consumer"));
                    ensure!(matches!(
                        phase,
                        "before" | "after" | "no-provider" | "active" | "gone" | "replacement"
                    ));
                    let source = request.source_peer.npub();
                    let peer = wait_for_udp_peer(&endpoint, &source).await?;
                    ensure!(
                        peer.transport_addr
                            .as_deref()
                            .and_then(|addr| addr.parse::<SocketAddr>().ok())
                            .is_some_and(|addr| addr.ip().is_loopback()),
                        "external probe did not arrive over the configured UDP transport"
                    );
                    endpoint.send_datagram(
                        request.source_peer,
                        EXTERNAL_SERVICE_PORT,
                        request.source_port,
                        format!("ack:{message}").into_bytes(),
                    ).await?;
                    report(format!(
                        "EXTERNAL_REQUEST role={role} phase={phase} source={source}"
                    ));
                }
            }
        }
    }
    endpoint.shutdown().await?;
    report("EXTERNAL_DONE".to_string());
    Ok(())
}

async fn run_anchor(args: ProductArgs) -> Result<()> {
    let endpoint = Arc::new(bind_product(&args).await?);
    wait_for_direct_peer(&endpoint, &args.external_npub, args.external_addr).await?;
    report(format!("ANCHOR_READY {} outbound=udp", endpoint.npub()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        match line.as_str() {
            "probe-outbound" => {
                prove_outbound(&endpoint, &args, "anchor", "before", 49_100).await?;
                report(format!("ANCHOR_OUTBOUND {}", endpoint.npub()));
            }
            "probe-outbound-after" => {
                prove_outbound(&endpoint, &args, "anchor", "after", 49_110).await?;
                report(format!("ANCHOR_OUTBOUND_AFTER {}", endpoint.npub()));
            }
            "stop" => break,
            command => bail!("unknown anchor command: {command}"),
        }
    }
    endpoint.shutdown().await?;
    report("ANCHOR_DONE".to_string());
    Ok(())
}

async fn run_provider(args: ProductArgs, data_dir: &Path, blob_phase: Option<&str>) -> Result<()> {
    let endpoint = Arc::new(bind_product(&args).await?);
    let store = Arc::new(open_shared_lmdb_blob_store(
        data_dir,
        SHARED_STORAGE_BUDGET_BYTES,
    )?);
    let (blob_phases, outbound_phase, reply_port): (&[&str], &str, u16) = match blob_phase {
        None => (&["before", "after"], "before", 49_101),
        Some("first") => (&["first"], "active", 49_111),
        Some("replacement") => (&["replacement"], "replacement", 49_112),
        Some(phase) => bail!("unknown provider phase: {phase}"),
    };
    for phase in blob_phases {
        let data = scenario_blob(phase);
        store.put(sha256(&data), data).await?;
    }
    let anchor = wait_for_fixed_local_peer(&endpoint, args.rendezvous_addr).await?;
    report(format!(
        "LOCAL_AUTH role=provider configured=false peer={}",
        anchor.npub
    ));
    prove_outbound(&endpoint, &args, "provider", outbound_phase, reply_port).await?;
    report(format!(
        "PROVIDER_READY {} phase={} shared_lmdb=true outbound=udp",
        endpoint.npub(),
        blob_phase.unwrap_or("legacy")
    ));

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        match line.as_str() {
            "probe-outbound" => {
                prove_outbound(&endpoint, &args, "provider", "after", 49_104).await?;
                report(format!("PROVIDER_OUTBOUND_AFTER {}", endpoint.npub()));
            }
            "stop" => break,
            command => bail!("unknown provider command: {command}"),
        }
    }
    drop(store);
    endpoint.shutdown().await?;
    report("PROVIDER_DONE".to_string());
    Ok(())
}

async fn run_consumer(
    args: ProductArgs,
    data_dir: &Path,
    initial_state: Option<&str>,
) -> Result<()> {
    let endpoint = Arc::new(bind_product(&args).await?);
    let store = Arc::new(open_shared_lmdb_blob_store(
        data_dir,
        SHARED_STORAGE_BUDGET_BYTES,
    )?);
    let router = shared_blob_router(store.clone())?;
    let anchor = wait_for_fixed_local_peer(&endpoint, args.rendezvous_addr).await?;
    report(format!(
        "LOCAL_AUTH role=consumer configured=false peer={}",
        anchor.npub
    ));
    match initial_state {
        Some("ready") => {
            prove_outbound(&endpoint, &args, "consumer", "before", 49_102).await?;
            fetch_blob(&router, "before").await?;
        }
        Some("empty") => {
            prove_outbound(&endpoint, &args, "consumer", "no-provider", 49_113).await?;
            expect_missing_blob(&router, store.as_ref(), "first", "no-provider").await?;
        }
        Some(state) => bail!("unknown consumer initial state: {state}"),
        None => bail!("consumer initial state must be ready or empty"),
    }
    report(format!(
        "CONSUMER_READY {} shared_lmdb=true outbound=udp",
        endpoint.npub()
    ));

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let mut command = line.split_whitespace();
        match (command.next(), command.next(), command.next()) {
            (Some("after-anchor-exit"), None, None) => {
                ensure!(initial_state == Some("ready"));
                fetch_blob(&router, "after").await?;
                prove_outbound(&endpoint, &args, "consumer", "after", 49_105).await?;
                report(format!(
                    "CONSUMER_AFTER_FAILOVER {} shared_lmdb=true outbound=udp",
                    endpoint.npub()
                ));
            }
            (Some("fetch"), Some(phase), None) => {
                ensure!(initial_state == Some("empty"));
                let (outbound_phase, reply_port) = match phase {
                    "first" => ("active", 49_114),
                    "replacement" => ("replacement", 49_116),
                    _ => bail!("unknown consumer fetch phase: {phase}"),
                };
                fetch_blob(&router, phase).await?;
                prove_outbound(&endpoint, &args, "consumer", outbound_phase, reply_port).await?;
                report(format!(
                    "CONSUMER_PROVIDER_ACTIVE {} phase={phase} shared_lmdb=true",
                    endpoint.npub(),
                ));
            }
            (Some("provider-gone"), None, None) => {
                ensure!(initial_state == Some("empty"));
                let shared = scenario_blob("first");
                let shared_hash = sha256(&shared);
                ensure!(
                    store.get(&shared_hash).await?.as_deref() == Some(shared.as_slice())
                        && router.get(&shared_hash, None).await?.as_deref()
                            == Some(shared.as_slice()),
                    "shared blob did not survive provider loss"
                );
                report("BLOB_SHARED phase=provider-gone verified=true".to_string());
                expect_missing_blob(&router, store.as_ref(), "replacement", "provider-gone")
                    .await?;
                prove_outbound(&endpoint, &args, "consumer", "gone", 49_115).await?;
                report(format!(
                    "CONSUMER_PROVIDER_GONE {} shared_lmdb=true",
                    endpoint.npub()
                ));
            }
            (Some("stop"), None, None) => break,
            _ => bail!("unknown consumer command: {line}"),
        }
    }
    drop(store);
    endpoint.shutdown().await?;
    report("CONSUMER_DONE".to_string());
    Ok(())
}

fn shared_blob_router(store: Arc<ConfiguredLmdbBlobStore>) -> Result<BlobRouter> {
    let identity = BlobRouteIdentity::new("shared-lmdb");
    let route = Arc::new(StoreBlobRoute::new(store));
    Ok(BlobRouter::new(
        vec![BlobRouteEntry::new(identity, route)],
        None,
        BlobRouterConfig::default(),
    )?)
}

fn external_config(bind_addr: &str) -> Config {
    let mut config = Config::new();
    config.node.routing.mode = RoutingMode::ReplyLearned;
    config.node.discovery.nostr.enabled = false;
    config.node.discovery.lan.enabled = false;
    config.transports.udp = TransportInstances::Single(application_udp(bind_addr));
    config
}

fn product_config(args: &ProductArgs) -> Config {
    let mut config = external_config("127.0.0.1:0");
    config.node.discovery.local.enabled = true;
    config.node.discovery.local.rendezvous_addr = args.rendezvous_addr;
    config.node.discovery.local.retry_interval_ms = 20;
    config.peers.push(PeerConfig::new(
        &args.external_npub,
        "udp",
        args.external_addr.to_string(),
    ));
    config
}

fn application_udp(bind_addr: &str) -> UdpConfig {
    UdpConfig {
        bind_addr: Some(bind_addr.to_string()),
        advertise_on_nostr: Some(false),
        public: Some(false),
        ..UdpConfig::default()
    }
}

async fn bind_endpoint(config: Config) -> Result<FipsEndpoint> {
    Ok(FipsEndpoint::builder()
        .config(config)
        .without_system_tun()
        .bind()
        .await?)
}

async fn bind_product(args: &ProductArgs) -> Result<FipsEndpoint> {
    Ok(FipsEndpoint::builder()
        .config(product_config(args))
        .local_rendezvous()
        .without_system_tun()
        .bind()
        .await?)
}

async fn wait_for_direct_peer(
    endpoint: &FipsEndpoint,
    npub: &str,
    expected_addr: SocketAddr,
) -> Result<fips_core::endpoint::FipsEndpointPeer> {
    wait_for_udp_peer_matching(
        endpoint,
        format!("direct peer {npub} at {expected_addr}"),
        |peer| peer.npub == npub && peer_addr(peer) == Some(expected_addr),
    )
    .await
}

async fn wait_for_fixed_local_peer(
    endpoint: &FipsEndpoint,
    rendezvous_addr: SocketAddrV4,
) -> Result<fips_core::endpoint::FipsEndpointPeer> {
    wait_for_udp_peer_matching(endpoint, "fixed loopback owner".to_string(), |peer| {
        peer_addr(peer) == Some(SocketAddr::V4(rendezvous_addr))
    })
    .await
}

async fn wait_for_udp_peer(
    endpoint: &FipsEndpoint,
    npub: &str,
) -> Result<fips_core::endpoint::FipsEndpointPeer> {
    wait_for_udp_peer_matching(endpoint, format!("UDP peer {npub}"), |peer| {
        peer.npub == npub
    })
    .await
}

async fn wait_for_udp_peer_matching(
    endpoint: &FipsEndpoint,
    description: String,
    matches: impl Fn(&fips_core::endpoint::FipsEndpointPeer) -> bool,
) -> Result<fips_core::endpoint::FipsEndpointPeer> {
    timeout(WAIT, async {
        loop {
            if let Some(peer) = endpoint.peers().await?.into_iter().find(|peer| {
                peer.connected && peer.transport_type.as_deref() == Some("udp") && matches(peer)
            }) {
                return Ok::<_, anyhow::Error>(peer);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .with_context(|| format!("{description} did not authenticate"))?
}

fn peer_addr(peer: &fips_core::endpoint::FipsEndpointPeer) -> Option<SocketAddr> {
    peer.transport_addr.as_deref()?.parse().ok()
}

async fn outbound_probe(
    endpoint: &FipsEndpoint,
    external_npub: &str,
    role: &str,
    phase: &str,
    reply_port: u16,
) -> Result<()> {
    let receiver = endpoint.register_service_receiver(reply_port).await?;
    endpoint
        .send_datagram(
            PeerIdentity::from_npub(external_npub)?,
            reply_port,
            EXTERNAL_SERVICE_PORT,
            format!("{role}:{phase}").into_bytes(),
        )
        .await?;
    receive_matching(
        &receiver,
        external_npub,
        format!("ack:{role}:{phase}").as_bytes(),
    )
    .await
}

async fn prove_outbound(
    endpoint: &FipsEndpoint,
    args: &ProductArgs,
    role: &str,
    phase: &str,
    reply_port: u16,
) -> Result<()> {
    wait_for_direct_peer(endpoint, &args.external_npub, args.external_addr).await?;
    outbound_probe(endpoint, &args.external_npub, role, phase, reply_port).await
}

async fn fetch_blob(router: &BlobRouter, phase: &str) -> Result<()> {
    let expected = scenario_blob(phase);
    let hash = sha256(&expected);
    let fetched = router
        .get(&hash, None)
        .await?
        .with_context(|| format!("shared LMDB missed the {phase} blob"))?;
    ensure!(fetched == expected, "{phase} blob bytes changed");
    ensure!(
        sha256(&fetched) == hash,
        "{phase} blob hash verification failed"
    );
    report(format!(
        "BLOB_FETCH phase={phase} verified=true shared=true"
    ));
    Ok(())
}

async fn expect_missing_blob(
    router: &BlobRouter,
    store: &ConfiguredLmdbBlobStore,
    blob_phase: &str,
    report_phase: &str,
) -> Result<()> {
    let hash = sha256(&scenario_blob(blob_phase));
    ensure!(store.get(&hash).await?.is_none(), "blob was already stored");
    ensure!(
        router.get(&hash, None).await?.is_none(),
        "missing blob was returned"
    );
    ensure!(
        store.get(&hash).await?.is_none(),
        "missing blob appeared in the shared store"
    );
    report(format!(
        "BLOB_MISS phase={report_phase} truthful=true shared=false"
    ));
    Ok(())
}

fn scenario_blob(phase: &str) -> Vec<u8> {
    format!("Iris Stack production Hashtree blob for {phase} failover. ")
        .bytes()
        .cycle()
        .take(BLOB_BYTES)
        .collect()
}

async fn receive_matching(
    receiver: &FipsEndpointServiceReceiver,
    expected_npub: &str,
    expected_data: &[u8],
) -> Result<()> {
    timeout(WAIT, async {
        loop {
            let mut replies = Vec::new();
            receiver
                .recv_batch_into(&mut replies, 8)
                .await
                .context("FSP reply service closed")?;
            if replies.iter().any(|reply| {
                reply.source_peer.npub() == expected_npub && reply.data.as_slice() == expected_data
            }) {
                return Ok::<_, anyhow::Error>(());
            }
        }
    })
    .await
    .context("FSP reply timed out")??;
    Ok(())
}

fn report(line: String) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}
