#[allow(dead_code)]
#[path = "support/product.rs"]
mod product;
mod support;

use std::fs;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use product::{
    HtreeNode, NodeConfig, TestRoot, drive_identity, htree_identity, payload, payload_sha256,
    required_binary, reserve_tcp_address, reserve_udp_address, spawn_htree,
    write_hashtree_read_config,
};
use serde_json::{Value, json};
use support::process::ManagedProcess;
use tokio::process::Command;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires released htree and Iris Chat/Drive fixtures; run scripts/product-lab.sh"]
async fn signed_events_and_blobs_cross_uninterested_transit_and_recover_after_rejoin() {
    run_mesh().await.unwrap();
}

async fn run_mesh() -> Result<()> {
    let htree_bin = required_binary("IRIS_STACK_HTREE_BIN")?;
    let chat_bin = required_binary("IRIS_STACK_CHAT_FIXTURE_BIN")?;
    let drive_bin = required_binary("IRIS_STACK_DRIVE_FIXTURE_BIN")?;
    let root = TestRoot::new()?;
    let transit = HtreeNode::new(root.path(), "transit");
    let transit_http = reserve_tcp_address()?;
    let transit_udp = reserve_udp_address()?;
    let transit_rendezvous = reserve_udp_address()?;
    let chat_udp = reserve_udp_address()?;
    let chat_rendezvous = reserve_udp_address()?;
    let drive_udp = reserve_udp_address()?;
    let drive_rendezvous = reserve_udp_address()?;
    transit.write_config(NodeConfig {
        http_addr: &transit_http,
        udp_addr: &transit_udp,
        rendezvous_addr: &transit_rendezvous,
        peers: &[],
    })?;
    let transit_npub = htree_identity(&htree_bin, &transit).await?;
    let drive_key = root.path().join("drive-key");
    let drive_identity = drive_identity(&drive_bin, &drive_key).await?;
    let chat_config = root.path().join("chat-config");
    write_hashtree_read_config(&chat_config, "http://127.0.0.1:1")?;

    // Identity hints tell applications who they intend to reach. Only B's UDP
    // address is configured; separate rendezvous addresses prevent a local shortcut.
    let mut chat_command = Command::new(&chat_bin);
    chat_command
        .arg("run")
        .arg(root.path().join("chat-data"))
        .env("HTREE_CONFIG_DIR", &chat_config)
        .env("IRIS_CHAT_SAME_HOST_HASHTREE", "1")
        .env("IRIS_CHAT_FIPS_LOCAL_RENDEZVOUS_ADDR", &chat_rendezvous)
        .env("IRIS_CHAT_FIPS_UDP_BIND_ADDR", &chat_udp)
        // Direct upgrades would bypass the transit node this test restarts.
        .env("IRIS_CHAT_FIPS_ENABLE_WEBRTC", "false")
        .env("IRIS_DEMO_RELAYS", "")
        .env("IRIS_FIPS_WEBSOCKET_SEED_URLS", "")
        .env(
            "IRIS_CHAT_FIPS_STATIC_PEERS",
            format!("{transit_npub}=udp:{transit_udp}"),
        )
        .env("IRIS_CHAT_FIPS_ROUTED_PEERS", &drive_identity.npub);
    let mut chat = ManagedProcess::spawn("relayless Chat A", &mut chat_command)?;
    let chat_ready = chat.json_event("ready").await?;
    let chat_npub = chat_ready["npub"]
        .as_str()
        .context("Chat ready omitted its FIPS identity")?;

    let mut drive_command = Command::new(&drive_bin);
    drive_command
        .arg("run")
        .arg(chat_npub)
        .arg(&drive_key)
        .arg(&drive_identity.profile_id)
        .env("HTREE_CONFIG_DIR", root.path().join("drive-config"))
        .env("HTREE_DATA_DIR", root.path().join("drive-data"))
        .env("IRIS_DRIVE_FIPS_LOCAL_RENDEZVOUS_ADDR", &drive_rendezvous)
        .env("IRIS_DRIVE_FIPS_UDP_BIND_ADDR", &drive_udp)
        .env("IRIS_DRIVE_FIPS_UDP_PUBLIC", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_UDP", "true")
        .env("IRIS_DRIVE_FIPS_ENABLE_WEBRTC", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_LAN_DISCOVERY", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_MESH_PUBSUB", "true")
        .env("IRIS_DRIVE_FIPS_SHARE_LOCAL_CANDIDATES", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_BOOTSTRAP", "true")
        .env("IRIS_FIPS_WEBSOCKET_SEED_URLS", "")
        .env(
            "IRIS_DRIVE_FIPS_BOOTSTRAP_PEERS",
            format!("{transit_npub}={transit_udp}"),
        );
    let mut drive = ManagedProcess::spawn("relayless Drive C", &mut drive_command)?;
    let drive_ready = drive.json_event("ready").await?;
    ensure!(drive_ready["npub"] == drive_identity.npub);
    ensure!(drive_ready["remote_npub"] == chat_npub);

    transit.write_config(NodeConfig {
        http_addr: &transit_http,
        udp_addr: &transit_udp,
        rendezvous_addr: &transit_rendezvous,
        peers: &[(chat_npub, &chat_udp), (&drive_identity.npub, &drive_udp)],
    })?;
    let mut transit_process = spawn_htree(&htree_bin, &transit, &transit_http, "transit B")?;
    transit_process.line_containing("FIPS: enabled").await?;
    let (chat_mesh, drive_mesh) = tokio::join!(
        wait_for_mesh(&mut chat, &transit_npub),
        wait_for_mesh(&mut drive, &transit_npub)
    );
    if chat_mesh.is_err() || drive_mesh.is_err() {
        eprintln!("Chat topology: {chat_mesh:?}\nDrive topology: {drive_mesh:?}");
        eprintln!(
            "Transit status: {:?}",
            product::fetch_status(&transit_http).await
        );
        for (label, process) in [
            ("Chat", &mut chat),
            ("Drive", &mut drive),
            ("transit", &mut transit_process),
        ] {
            let log = format!(
                "{}\n{}",
                process.stderr_snapshot(),
                process.stdout_snapshot()
            );
            eprintln!(
                "{label} stderr:\n{}",
                log.lines()
                    .rev()
                    .take(80)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
    chat_mesh?;
    drive_mesh?;
    assert_transit_links(&transit_http, chat_npub, &drive_identity.npub).await?;
    let drive_status = status(&mut drive).await?;
    let authorized = drive_status["authorized_peers"]
        .as_array()
        .context("Drive omitted its application authorization snapshot")?;
    ensure!(authorized.iter().any(|peer| peer == chat_npub));
    ensure!(
        !authorized.iter().any(|peer| peer == &transit_npub),
        "transit B entered Drive's application authorization list"
    );

    let initial = Instant::now();
    let chat_event = publish(&mut chat, 9, "chat before partition").await?;
    let drive_event = publish(&mut drive, 1063, "control before partition").await?;
    receive(&mut drive, &chat_event).await?;
    receive(&mut chat, &drive_event).await?;
    let initial_event_ms = initial.elapsed().as_millis();
    let initial_blob = put_blob(&root, &mut drive, "initial").await?;
    let initial_blob_ms = fetch_blob(&mut chat, &initial_blob).await?;
    let before_partition = idle_resources(
        "before partition",
        [&chat, &transit_process, &drive],
        &transit_http,
    )
    .await?;

    let stopped = transit_process.kill().await?;
    ensure!(
        !stopped.status.success(),
        "transit was not forcibly stopped"
    );
    assert_no_lan_discovery(&stopped.stdout, &stopped.stderr)?;
    let queued_chat = publish(&mut chat, 9, "chat during partition").await?;
    let queued_drive = publish(&mut drive, 1063, "control during partition").await?;
    let queued_blob = put_blob(&root, &mut drive, "during-partition").await?;

    // Both endpoints keep their original subscription and publish each event
    // exactly once. Only the transport process is replaced.
    let recovered = Instant::now();
    let mut replacement = spawn_htree(&htree_bin, &transit, &transit_http, "replacement B")?;
    replacement.line_containing("FIPS: enabled").await?;
    wait_for_mesh(&mut chat, &transit_npub).await?;
    wait_for_mesh(&mut drive, &transit_npub).await?;
    receive(&mut drive, &queued_chat).await?;
    receive(&mut chat, &queued_drive).await?;
    let recovery_ms = recovered.elapsed().as_millis();
    let recovered_blob_ms = fetch_blob(&mut chat, &queued_blob).await?;
    assert_direct_transit(&status(&mut chat).await?, &transit_npub)?;
    assert_direct_transit(&status(&mut drive).await?, &transit_npub)?;
    assert_transit_links(&transit_http, chat_npub, &drive_identity.npub).await?;
    let after_recovery = idle_resources(
        "after recovery",
        [&chat, &replacement, &drive],
        &transit_http,
    )
    .await?;

    for process in [&mut chat, &mut drive] {
        process.send_line("stop").await?;
        process.json_event("stopped").await?;
    }
    for output in [chat.finish().await?, drive.finish().await?] {
        assert_no_lan_discovery(&output.stdout, &output.stderr)?;
    }
    let output = replacement.kill().await?;
    assert_no_lan_discovery(&output.stdout, &output.stderr)?;
    eprintln!(
        "relayless product mesh: signed events 4/4, verified blobs 2/2, direct peers A=1 B=2 C=1, public relays=0; initial events {initial_event_ms}ms, initial blob {initial_blob_ms}ms, rejoin events {recovery_ms}ms, rejoined blob {recovered_blob_ms}ms"
    );
    if let Some(path) = std::env::var_os("IRIS_STACK_MESH_METRICS_PATH") {
        fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
                "signed_events": 4, "verified_blobs": 2,
                "public_relays": 0, "direct_peers": [1, 2, 1],
                "latency_ms": {"initial_events": initial_event_ms, "initial_blob": initial_blob_ms,
                    "rejoin_events": recovery_ms, "rejoined_blob": recovered_blob_ms},
                "idle": [before_partition, after_recovery],
            }))?,
        )?;
    }
    Ok(())
}

async fn status(process: &mut ManagedProcess) -> Result<Value> {
    process.send_line("status").await?;
    process.json_event("status").await
}

async fn idle_resources(
    phase: &str,
    processes: [&ManagedProcess; 3],
    transit_http: &str,
) -> Result<Value> {
    let cpu_budget = std::env::var("IRIS_STACK_IDLE_MAX_CPU_PERCENT")
        .ok()
        .map(|value| value.parse::<f64>())
        .transpose()
        .context("parse idle CPU budget")?
        .unwrap_or(5.0);
    ensure!(
        cpu_budget.is_finite() && cpu_budget > 0.0,
        "idle CPU budget must be positive and finite"
    );
    // Let final ACKs settle before sampling. The fixed 65-second window
    // spans at least one 60-second managed reputation maintenance interval,
    // regardless of the phase at which sampling begins.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let before_traffic = transit_traffic(transit_http).await?;
    let mut before_cpu = [None; 3];
    for (index, process) in processes.iter().enumerate() {
        before_cpu[index] = process.cpu_seconds().await?;
    }
    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(65)).await;
    let elapsed = started.elapsed().as_secs_f64();
    let after_traffic = transit_traffic(transit_http).await?;
    let mut cpu_percent = [None; 3];
    let cpu_required =
        cfg!(target_os = "linux") || std::env::var("IRIS_STACK_REQUIRE_CPU").as_deref() == Ok("1");
    for (index, process) in processes.iter().enumerate() {
        cpu_percent[index] = checked_cpu_percent(
            before_cpu[index],
            process.cpu_seconds().await?,
            elapsed,
            cpu_budget,
            cpu_required,
        )
        .with_context(|| format!("idle process {}", ["A", "B", "C"][index]))?;
    }
    let mut traffic = [0; 4];
    for index in 0..traffic.len() {
        traffic[index] = after_traffic[index]
            .checked_sub(before_traffic[index])
            .context("transit traffic counter reset")?;
    }
    let [sent, received, sent_packets, received_packets] = traffic;
    let bytes_per_second = (sent + received) as f64 / elapsed;
    eprintln!(
        "relayless idle {phase}: {elapsed:.2}s; CPU % of one core A={} B={} C={} (budget {cpu_budget:.2}%); transit sent={sent}B/{sent_packets}packets received={received}B/{received_packets}packets ({bytes_per_second:.1}B/s combined)",
        cpu_label(cpu_percent[0]),
        cpu_label(cpu_percent[1]),
        cpu_label(cpu_percent[2])
    );
    // This generous wire budget tolerates keepalives and route maintenance,
    // while catching a continuing payload/retry storm after completed work.
    ensure!(
        bytes_per_second < 4096.0,
        "idle transit exceeded 4 KiB/s: {bytes_per_second:.1} B/s"
    );
    Ok(
        json!({"phase": phase, "seconds": elapsed, "cpu_percent": cpu_percent,
        "cpu_budget_percent": cpu_budget, "cpu_required": cpu_required,
        "sent_bytes": sent, "received_bytes": received,
        "sent_packets": sent_packets, "received_packets": received_packets,
        "combined_bytes_per_second": bytes_per_second, "wire_budget_bytes_per_second": 4096}),
    )
}

fn checked_cpu_percent(
    before: Option<f64>,
    after: Option<f64>,
    elapsed: f64,
    budget: f64,
    required: bool,
) -> Result<Option<f64>> {
    let (Some(before), Some(after)) = (before, after) else {
        ensure!(!required, "CPU measurement unavailable");
        return Ok(None);
    };
    ensure!(
        before.is_finite() && after.is_finite() && before >= 0.0 && after >= before,
        "invalid or decreasing cumulative CPU time"
    );
    ensure!(
        elapsed.is_finite() && elapsed > 0.0,
        "invalid CPU sample duration"
    );
    let used = 100.0 * (after - before) / elapsed;
    ensure!(
        used <= budget,
        "CPU {used:.2}% exceeded {budget:.2}% of one core"
    );
    Ok(Some(used))
}

#[test]
fn resource_gate_rejects_unavailable_reset_and_excess_cpu() {
    for (before, after) in [
        (None, Some(1.0)),
        (Some(1.0), None),
        (None, None),
        (Some(2.0), Some(1.0)),
        (Some(f64::NAN), Some(1.0)),
        (Some(0.0), Some(f64::INFINITY)),
        (Some(0.0), Some(0.051)),
    ] {
        assert!(checked_cpu_percent(before, after, 1.0, 5.0, true).is_err());
    }
    assert_eq!(
        checked_cpu_percent(Some(0.0), Some(0.05), 1.0, 5.0, true).unwrap(),
        Some(5.0)
    );
    assert_eq!(
        checked_cpu_percent(None, None, 1.0, 5.0, false).unwrap(),
        None
    );
}

fn cpu_label(value: Option<f64>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.2}"))
}

async fn transit_traffic(address: &str) -> Result<[u64; 4]> {
    let status = product::fetch_status(address).await?;
    let peers = status["fips"]["peer_statuses"]
        .as_array()
        .context("transit omitted FIPS peer counters")?;
    ensure!(
        peers
            .iter()
            .filter(|peer| peer["connected"] == true)
            .count()
            == 2,
        "idle sampling requires two live transit peers"
    );
    peers.iter().try_fold([0; 4], |mut total, peer| {
        for (index, name) in ["bytes_sent", "bytes_recv", "packets_sent", "packets_recv"]
            .into_iter()
            .enumerate()
        {
            total[index] += peer[name]
                .as_u64()
                .with_context(|| format!("missing {name}"))?;
        }
        Ok(total)
    })
}

async fn assert_transit_links(address: &str, chat: &str, drive: &str) -> Result<()> {
    let status = product::fetch_status(address).await?;
    let mut connected = status["fips"]["peer_statuses"]
        .as_array()
        .context("transit omitted FIPS peers")?
        .iter()
        .filter(|peer| peer["connected"] == true)
        .filter_map(|peer| peer["npub"].as_str())
        .collect::<Vec<_>>();
    connected.sort_unstable();
    let mut expected = [chat, drive];
    expected.sort_unstable();
    ensure!(
        connected == expected,
        "transit connected outside the isolated mesh"
    );
    Ok(())
}

fn assert_direct_transit(status: &Value, transit: &str) -> Result<()> {
    ensure!(
        status["relay_count"] == 0,
        "endpoint configured public relays"
    );
    ensure!(
        status["lan_discovery_enabled"] == false,
        "endpoint enabled host-LAN discovery"
    );
    let peers = status["direct_peers"]
        .as_array()
        .context("fixture status omitted direct FIPS peers")?;
    let connected = peers
        .iter()
        .filter(|peer| peer["connected"] == true)
        .collect::<Vec<_>>();
    ensure!(
        connected.len() == 1 && connected[0]["npub"] == transit,
        "expected only transit B as a direct FIPS peer: {status}"
    );
    ensure!(connected[0]["transport_type"] == "udp");
    Ok(())
}

async fn wait_for_mesh(process: &mut ManagedProcess, transit: &str) -> Result<()> {
    let mut last = Value::Null;
    timeout(Duration::from_secs(60), async {
        loop {
            let current = status(process).await?;
            if let Ok(limit) = std::env::var("IRIS_STACK_FIXTURE_PUBSUB_MAX_PEERS") {
                ensure!(
                    current["pubsub_max_peers"].as_u64() == Some(limit.parse()?),
                    "fixture did not apply diagnostic pubsub peer limit: {current}"
                );
            }
            if assert_direct_transit(&current, transit).is_ok()
                && current["pubsub_peer_count"].as_u64().unwrap_or(0) > 0
            {
                return Ok::<_, anyhow::Error>(());
            }
            last = current;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .with_context(|| {
        format!("endpoint did not establish pubsub through the sole transit peer: {last}")
    })?
}

async fn publish(process: &mut ManagedProcess, kind: u16, content: &str) -> Result<Value> {
    process
        .send_line(&format!("publish {kind} {content}"))
        .await?;
    let published = process.json_event("published").await?;
    ensure!(published["id"].as_str().is_some());
    ensure!(published["pubkey"].as_str().is_some());
    ensure!(published["kind"] == kind && published["content"] == content);
    Ok(published)
}

async fn receive(process: &mut ManagedProcess, expected: &Value) -> Result<()> {
    let id = expected["id"]
        .as_str()
        .context("published event omitted id")?;
    process.send_line(&format!("receive {id}")).await?;
    let received = process.json_event("received").await?;
    ensure!(
        received.get("error").is_none(),
        "event delivery failed: {received}"
    );
    for field in ["id", "pubkey", "kind", "content"] {
        ensure!(
            received[field] == expected[field],
            "event {field} changed in transit"
        );
    }
    ensure!(
        received["verified"] == true,
        "receiver did not verify the event signature"
    );
    Ok(())
}

struct Blob {
    nhash: String,
    bytes: Vec<u8>,
}

async fn put_blob(root: &TestRoot, drive: &mut ManagedProcess, label: &str) -> Result<Blob> {
    let bytes = payload(label, 192 * 1024 + 37);
    let path = root.path().join(format!("{label}.bin"));
    fs::write(&path, &bytes)?;
    drive.send_line(&format!("put {}", path.display())).await?;
    let stored = drive.json_event("put").await?;
    ensure!(stored["sha256"] == payload_sha256(&bytes));
    let nhash = stored["nhash"]
        .as_str()
        .context("Drive put omitted nhash")?;
    Ok(Blob {
        nhash: nhash.to_string(),
        bytes,
    })
}

async fn fetch_blob(chat: &mut ManagedProcess, blob: &Blob) -> Result<u128> {
    let started = Instant::now();
    chat.send_line(&format!("fetch {}", blob.nhash)).await?;
    let fetched = chat.json_event("fetch").await?;
    ensure!(
        fetched.get("error").is_none(),
        "blob retrieval failed: {fetched}"
    );
    ensure!(fetched["fetched"] == blob.bytes.len() as u64);
    ensure!(fetched["sha256"] == payload_sha256(&blob.bytes));
    Ok(started.elapsed().as_millis())
}

fn assert_no_lan_discovery(stdout: &str, stderr: &str) -> Result<()> {
    for output in [stdout, stderr] {
        ensure!(
            !output.contains("mDNS discovery started")
                && !output.contains("LAN mDNS discovery enabled"),
            "product enabled host-LAN discovery"
        );
    }
    Ok(())
}
