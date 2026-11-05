#[allow(dead_code)]
#[path = "support/product.rs"]
mod product;
mod support;

use std::fs;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use product::{
    HtreeNode, NodeConfig, TestRoot, add_blob, drive_identity, htree_identity, nhash_for_cid,
    payload, payload_sha256, required_binary, reserve_tcp_address, reserve_udp_address,
    spawn_htree, write_hashtree_read_config,
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
    // Keep the public provider separate from transit and Drive's private cache.
    let provider = HtreeNode::new(root.path(), "public-provider");
    let provider_http = reserve_tcp_address()?;
    let provider_udp = reserve_udp_address()?;
    let provider_rendezvous = reserve_udp_address()?;
    transit.write_config(NodeConfig {
        http_addr: &transit_http,
        udp_addr: &transit_udp,
        rendezvous_addr: &transit_rendezvous,
        peers: &[],
    })?;
    let transit_npub = htree_identity(&htree_bin, &transit).await?;
    provider.write_config(NodeConfig {
        http_addr: &provider_http,
        udp_addr: &provider_udp,
        rendezvous_addr: &provider_rendezvous,
        peers: &[(&transit_npub, &transit_udp)],
    })?;
    let provider_npub = htree_identity(&htree_bin, &provider).await?;
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
        .env(
            "IRIS_CHAT_FIPS_ROUTED_PEERS",
            format!("{provider_npub},{}", drive_identity.npub),
        );
    let mut chat = ManagedProcess::spawn("relayless Chat A", &mut chat_command)?;
    let chat_ready = chat.json_event("ready").await?;
    let chat_npub = chat_ready["npub"]
        .as_str()
        .context("Chat ready omitted its FIPS identity")?;
    let chat_owner = chat_ready["owner_npub"]
        .as_str()
        .context("Chat ready omitted its account identity")?;

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

    transit.write_config_with_fresh_clients(
        NodeConfig {
            http_addr: &transit_http,
            udp_addr: &transit_udp,
            rendezvous_addr: &transit_rendezvous,
            peers: &[
                (chat_npub, &chat_udp),
                (&drive_identity.npub, &drive_udp),
                (&provider_npub, &provider_udp),
            ],
        },
        1,
    )?;
    let mut transit_process = spawn_htree(&htree_bin, &transit, &transit_http, "transit B")?;
    transit_process.line_containing("FIPS: enabled").await?;
    let mut provider_process = spawn_htree(
        &htree_bin,
        &provider,
        &provider_http,
        "public blob provider P",
    )?;
    provider_process.line_containing("FIPS: enabled").await?;
    product::wait_for_htree_fips_peer(&provider_http, &transit_npub).await?;
    assert_provider_transit(&provider_http, &transit_npub).await?;
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
            ("provider", &mut provider_process),
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
    let roles = [
        chat_npub,
        drive_identity.npub.as_str(),
        provider_npub.as_str(),
    ];
    // Require each intended identified role and at most the separate anonymous reader.
    assert_transit_links(&transit_http, &roles, chat_owner, None).await?;
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
    let initial_blob = put_public_blob(&htree_bin, &provider, "initial").await?;
    let initial_blob_ms_result = fetch_blob(&mut chat, &initial_blob).await;
    let initial_blob_ms = with_mesh_diagnostics(
        initial_blob_ms_result,
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut transit_process),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await?;
    wait_for_reader(&transit_http, &roles, chat_owner, None).await?;
    // A known hash exists only in Drive's private cache. An unsuccessful read
    // proves bounded no-disclosure, not observation of an explicit ACL response.
    let private_blob = put_blob(&root, &mut drive, "drive-private-only").await?;
    let private_probe_result =
        assert_private_blob_unavailable(&mut chat, &mut drive, &private_blob).await;
    let private_probe_ms = with_mesh_diagnostics(
        private_probe_result,
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut transit_process),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await?;
    assert_direct_transit(&status(&mut drive).await?, &transit_npub)?;
    // Use uncached bytes for the positive control after the private-cache probe.
    let post_probe_blob = put_public_blob(&htree_bin, &provider, "after-private-probe").await?;
    let post_probe_blob_ms_result = fetch_blob(&mut chat, &post_probe_blob).await;
    let post_probe_blob_ms = with_mesh_diagnostics(
        post_probe_blob_ms_result,
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut transit_process),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await?;
    // Recovery may replace the anonymous endpoint. Pin its observed identity only
    // for this idle interval, not across a failed request or a transit restart.
    let reader = wait_for_reader(&transit_http, &roles, chat_owner, None).await?;
    let before_partition = idle_resources(
        "before partition",
        [&chat, &transit_process, &drive, &provider_process],
        &transit_http,
        &roles,
        chat_owner,
        &reader,
    )
    .await?;

    // Persist real partial evidence even if the subsequent recovery check fails.
    write_mesh_metrics(&json!({
        "status": "incomplete", "functional_recovery_complete": false,
        "wire_budget_passed": idle_wire_budget(&before_partition).is_ok(),
        "idle": [&before_partition],
    }))?;
    // Capture the failing phase while its processes are alive, but defer the
    // wire-budget error until bounded recovery and final evidence are complete.
    let _ = with_mesh_diagnostics(
        idle_wire_budget(&before_partition),
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut transit_process),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await;

    let stopped = transit_process.kill().await?;
    ensure!(
        !stopped.status.success(),
        "transit was not forcibly stopped"
    );
    assert_no_lan_discovery(&stopped.stdout, &stopped.stderr)?;
    let queued_chat = publish(&mut chat, 9, "chat during partition").await?;
    let queued_drive = publish(&mut drive, 1063, "control during partition").await?;
    let queued_blob = put_public_blob(&htree_bin, &provider, "during-partition").await?;

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
    product::wait_for_htree_fips_peer(&provider_http, &transit_npub).await?;
    assert_provider_transit(&provider_http, &transit_npub).await?;
    let recovered_blob_ms_result = fetch_blob(&mut chat, &queued_blob).await;
    let recovered_blob_ms = with_mesh_diagnostics(
        recovered_blob_ms_result,
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut replacement),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await?;
    assert_direct_transit(&status(&mut chat).await?, &transit_npub)?;
    assert_direct_transit(&status(&mut drive).await?, &transit_npub)?;
    let recovered_reader = wait_for_reader(&transit_http, &roles, chat_owner, None).await?;
    let after_recovery = idle_resources(
        "after recovery",
        [&chat, &replacement, &drive, &provider_process],
        &transit_http,
        &roles,
        chat_owner,
        &recovered_reader,
    )
    .await?;

    let _ = with_mesh_diagnostics(
        idle_wire_budget(&after_recovery),
        &mut chat,
        [
            ("Drive", &mut drive),
            ("transit", &mut replacement),
            ("provider", &mut provider_process),
        ],
        [("transit", &transit_http), ("provider", &provider_http)],
    )
    .await;
    let wire_result = idle_wire_budget(&before_partition).and(idle_wire_budget(&after_recovery));
    // Both functional phases completed. Cleanup still has to succeed before a
    // successful test can be reported; this file is diagnostic evidence only.
    write_mesh_metrics(&json!({
        "status": if wire_result.is_ok() { "measured" } else { "failed" },
        "functional_recovery_complete": true,
        "wire_budget_passed": wire_result.is_ok(),
        "signed_events": 4, "verified_blobs": 3,
        "private_blob_probe": {"disclosed": false, "explicit_acl_reply_observed": false},
        "anonymous_reader_adjacency": true,
        "provider_get_source_observed": false,
        "public_relays": 0, "direct_peers": [1, 4, 1, 1],
        "process_roles": ["chat", "transit", "drive", "public_provider"],
        "latency_ms": {"initial_events": initial_event_ms, "initial_blob": initial_blob_ms,
            "rejoin_events": recovery_ms, "rejoined_blob": recovered_blob_ms,
            "private_probe": private_probe_ms, "post_probe_blob": post_probe_blob_ms},
        "idle": [&before_partition, &after_recovery],
    }))?;

    for process in [&mut chat, &mut drive] {
        process.send_line("stop").await?;
        process.json_event("stopped").await?;
    }
    for output in [chat.finish().await?, drive.finish().await?] {
        assert_no_lan_discovery(&output.stdout, &output.stderr)?;
    }
    let provider_output = provider_process.kill().await?;
    assert_no_lan_discovery(&provider_output.stdout, &provider_output.stderr)?;
    let output = replacement.kill().await?;
    assert_no_lan_discovery(&output.stdout, &output.stderr)?;
    eprintln!(
        "relayless product mesh: signed events 4/4, verified public blobs 3/3, private Drive blob disclosed=0, direct roles A/B/C/P plus anonymous reader, public relays=0; initial events {initial_event_ms}ms, initial blob {initial_blob_ms}ms, rejoin events {recovery_ms}ms, rejoined blob {recovered_blob_ms}ms"
    );
    wire_result
}

fn write_mesh_metrics(metrics: &Value) -> Result<()> {
    if let Some(path) = std::env::var_os("IRIS_STACK_MESH_METRICS_PATH") {
        fs::write(path, serde_json::to_vec_pretty(metrics)?)?;
    }
    Ok(())
}

fn check_wire_budget(bytes_per_second: f64) -> Result<()> {
    ensure!(
        bytes_per_second.is_finite() && bytes_per_second >= 0.0 && bytes_per_second < 4096.0,
        "idle transit exceeded 4 KiB/s or had an invalid rate: {bytes_per_second:.1} B/s"
    );
    Ok(())
}

fn idle_wire_budget(sample: &Value) -> Result<()> {
    check_wire_budget(
        sample["combined_bytes_per_second"]
            .as_f64()
            .context("idle sample omitted its traffic rate")?,
    )
    .with_context(|| format!("idle phase {}", sample["phase"]))
}

#[test]
fn wire_budget_keeps_its_strict_boundary() {
    for accepted in [0.0, 4095.999] {
        assert!(check_wire_budget(accepted).is_ok());
    }
    for rejected in [4096.0, 5438.9, -1.0, f64::INFINITY, f64::NAN] {
        assert!(check_wire_budget(rejected).is_err());
    }
}

async fn status(process: &mut ManagedProcess) -> Result<Value> {
    process.send_line("status").await?;
    process.json_event("status").await
}

async fn idle_resources(
    phase: &str,
    processes: [&ManagedProcess; 4],
    transit_http: &str,
    roles: &[&str; 3],
    chat_owner: &str,
    reader: &str,
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
    let before_traffic = transit_traffic(transit_http, roles, chat_owner, reader).await?;
    let mut before_cpu = [None; 4];
    for (index, process) in processes.iter().enumerate() {
        before_cpu[index] = process.cpu_seconds().await?;
    }
    let started = Instant::now();
    tokio::time::sleep(Duration::from_secs(65)).await;
    let elapsed = started.elapsed().as_secs_f64();
    let after_traffic = transit_traffic(transit_http, roles, chat_owner, reader).await?;
    let mut cpu_percent = [None; 4];
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
        .with_context(|| format!("idle process {}", ["A", "B", "C", "P"][index]))?;
    }
    let (traffic, per_role) = traffic_delta(&before_traffic, &after_traffic)?;
    let [sent, received, sent_packets, received_packets] = traffic;
    let bytes_per_second = (sent + received) as f64 / elapsed;
    eprintln!(
        "relayless idle {phase}: {elapsed:.2}s; CPU % of one core A={} B={} C={} P={} (budget {cpu_budget:.2}%); transit sent={sent}B/{sent_packets}packets received={received}B/{received_packets}packets ({bytes_per_second:.1}B/s combined)",
        cpu_label(cpu_percent[0]),
        cpu_label(cpu_percent[1]),
        cpu_label(cpu_percent[2]),
        cpu_label(cpu_percent[3])
    );
    eprintln!("relayless idle {phase} per-role transit deltas: {per_role}");
    // Keep the same strict budget. The caller reports failure only after
    // recovery evidence and process cleanup, without classifying this traffic.
    let wire_budget_passed = check_wire_budget(bytes_per_second).is_ok();
    Ok(
        json!({"phase": phase, "seconds": elapsed, "cpu_percent": cpu_percent,
        "cpu_budget_percent": cpu_budget, "cpu_required": cpu_required,
        "sent_bytes": sent, "received_bytes": received,
        "sent_packets": sent_packets, "received_packets": received_packets,
        "combined_bytes_per_second": bytes_per_second, "wire_budget_bytes_per_second": 4096,
        "wire_budget_passed": wire_budget_passed, "per_role": per_role,
        "peer_boundaries": {"before": before_traffic.peer_statuses,
                            "after": after_traffic.peer_statuses}}),
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

#[derive(Clone)]
struct TrafficSnapshot {
    total: [u64; 4],
    roles: [(String, [u64; 4]); 4],
    peer_statuses: Vec<Value>,
}

const TRAFFIC_ROLES: [&str; 4] = ["chat", "drive", "public_provider", "anonymous_reader"];

async fn transit_traffic(
    address: &str,
    roles: &[&str; 3],
    chat_owner: &str,
    reader: &str,
) -> Result<TrafficSnapshot> {
    traffic_snapshot(&product::fetch_status(address).await?, roles, chat_owner, reader)
}

fn peer_counters(peer: &Value) -> Result<[u64; 4]> {
    let mut counters = [0; 4];
    for (index, name) in ["bytes_sent", "bytes_recv", "packets_sent", "packets_recv"]
        .into_iter()
        .enumerate()
    {
        counters[index] = peer[name]
            .as_u64()
            .with_context(|| format!("missing {name}"))?;
    }
    Ok(counters)
}

fn traffic_snapshot(
    status: &Value,
    roles: &[&str; 3],
    chat_owner: &str,
    reader: &str,
) -> Result<TrafficSnapshot> {
    validate_transit_links(status, roles, chat_owner, Some(reader))?;
    let peers = status["fips"]["peer_statuses"]
        .as_array()
        .context("transit omitted FIPS peer counters")?;
    let mut snapshot = TrafficSnapshot {
        total: [0; 4],
        roles: std::array::from_fn(|_| (String::new(), [0; 4])),
        peer_statuses: peers.iter().map(peer_boundary).collect(),
    };
    // Preserve the previous aggregate over all peer counters, including retained
    // disconnected entries. Role detail must not reduce the measured total.
    for peer in peers {
        for (index, count) in peer_counters(peer)?.into_iter().enumerate() {
            snapshot.total[index] = snapshot.total[index]
                .checked_add(count)
                .context("transit traffic counter overflow")?;
        }
    }
    for (index, identity) in [roles[0], roles[1], roles[2], reader].into_iter().enumerate() {
        let peer = peers.iter()
            .find(|peer| peer["connected"] == true && peer["npub"] == identity)
            .context("traffic role disappeared")?;
        snapshot.roles[index] = (identity.to_string(), peer_counters(peer)?);
    }
    Ok(snapshot)
}

fn peer_boundary(peer: &Value) -> Value {
    // The pinned Htree API currently exposes only identity/connection/transport
    // and byte/packet counters. Preserve its complete record; absent optional
    // link/RTT/rekey/probe observations must not look like measured zeroes.
    let missing = ["link_id", "srtt_ms", "srtt_age_ms", "rekey", "direct_probe"]
        .into_iter()
        .filter(|field| peer.get(*field).is_none())
        .collect::<Vec<_>>();
    json!({
        "key": {"npub": peer["npub"], "link_id": peer.get("link_id"),
                "connected": peer["connected"]},
        "status": peer,
        "missing_optional_fields": missing,
    })
}

#[test]
fn peer_boundaries_preserve_actual_fields_and_mark_missing_observations() {
    let peer = json!({"npub": "anonymous-reader", "connected": true,
        "transport_type": "udp", "transport_addr": "127.0.0.1:1234",
        "bytes_sent": 100, "bytes_recv": 200, "packets_sent": 3, "packets_recv": 4});
    let boundary = peer_boundary(&peer);
    assert_eq!(boundary["status"], peer);
    assert_eq!(boundary["key"], json!({"npub": "anonymous-reader", "link_id": null, "connected": true}));
    assert_eq!(boundary["missing_optional_fields"], json!(["link_id", "srtt_ms", "srtt_age_ms", "rekey", "direct_probe"]));
    // Future API fields, including an explicit null, are retained without
    // inventing a value or incorrectly labelling a present field as absent.
    let mut extended = peer.clone();
    extended["link_id"] = json!("observed-link");
    extended["srtt_ms"] = Value::Null;
    extended["other_counter"] = json!(7);
    let boundary = peer_boundary(&extended);
    assert_eq!(boundary["status"], extended);
    assert_eq!(boundary["key"]["link_id"], "observed-link");
    assert_eq!(boundary["missing_optional_fields"], json!(["srtt_age_ms", "rekey", "direct_probe"]));
}

fn counter_delta(before: &[u64; 4], after: &[u64; 4]) -> Result<[u64; 4]> {
    let mut delta = [0; 4];
    for index in 0..delta.len() {
        delta[index] = after[index]
            .checked_sub(before[index])
            .context("transit traffic counter reset")?;
    }
    Ok(delta)
}

fn traffic_delta(before: &TrafficSnapshot, after: &TrafficSnapshot) -> Result<([u64; 4], Value)> {
    let total = counter_delta(&before.total, &after.total)?;
    let mut per_role = serde_json::Map::new();
    for (index, role) in TRAFFIC_ROLES.into_iter().enumerate() {
        ensure!(before.roles[index].0 == after.roles[index].0, "{role} identity changed during idle");
        let [sent, received, sent_packets, received_packets] =
            counter_delta(&before.roles[index].1, &after.roles[index].1)
                .with_context(|| format!("idle {role}"))?;
        per_role.insert(role.to_string(), json!({
            "sent_bytes": sent, "received_bytes": received,
            "sent_packets": sent_packets, "received_packets": received_packets,
        }));
    }
    Ok((total, Value::Object(per_role)))
}

#[test]
fn traffic_samples_reject_role_changes_and_per_role_counter_resets() {
    let roles = ["chat-device", "drive-device", "public-provider"];
    let peers = [roles[0], roles[1], roles[2], "anonymous-reader"].map(|identity|
        json!({"npub": identity, "connected": true, "transport_type": "udp",
            "bytes_sent": 100, "bytes_recv": 200, "packets_sent": 3, "packets_recv": 4}));
    let status = json!({"fips": {"peer_statuses": peers}});
    let before = traffic_snapshot(&status, &roles, "chat-account", "anonymous-reader").unwrap();
    let mut after = before.clone();
    after.total = [440, 880, 16, 20];
    for (_, counters) in &mut after.roles {
        *counters = [110, 220, 4, 5];
    }
    let (total, detail) = traffic_delta(&before, &after).unwrap();
    assert_eq!(total, [40, 80, 4, 4]);
    for role in TRAFFIC_ROLES {
        assert_eq!(detail[role], json!({"sent_bytes": 10, "received_bytes": 20,
            "sent_packets": 1, "received_packets": 1}));
    }
    for role in 0..4 {
        let mut changed = after.clone();
        changed.roles[role].0 = "different-identity".to_string();
        assert!(traffic_delta(&before, &changed).is_err());
        for counter in 0..4 {
            let mut reset = after.clone();
            reset.roles[role].1[counter] = before.roles[role].1[counter] - 1;
            // Other peers' growth cannot mask one role's reset.
            assert!(traffic_delta(&before, &reset).is_err());
        }
    }
    let mut changed_status = status.clone();
    changed_status["fips"]["peer_statuses"][3]["npub"] = json!("new-reader");
    assert!(traffic_snapshot(&changed_status, &roles, "chat-account", "anonymous-reader").is_err());
    assert!(traffic_snapshot(&status, &roles, "chat-account", "chat-account").is_err());
    let mut extra_status = status.clone();
    let mut retired = extra_status["fips"]["peer_statuses"][0].clone();
    retired["npub"] = json!("retired-reader");
    retired["connected"] = json!(false);
    extra_status["fips"]["peer_statuses"].as_array_mut().unwrap().push(retired);
    let extra = traffic_snapshot(&extra_status, &roles, "chat-account", "anonymous-reader").unwrap();
    assert_eq!(extra.total, [500, 1000, 15, 20]);
    assert_eq!(extra.roles, before.roles);
}

async fn assert_provider_transit(address: &str, transit: &str) -> Result<()> {
    let status = product::fetch_status(address).await?;
    let connected = status["fips"]["peer_statuses"]
        .as_array()
        .context("public provider omitted FIPS peers")?
        .iter()
        .filter(|peer| peer["connected"] == true)
        .collect::<Vec<_>>();
    ensure!(
        connected.len() == 1
            && connected[0]["npub"] == transit
            && connected[0]["transport_type"] == "udp",
        "public provider bypassed the sole UDP transit"
    );
    Ok(())
}

async fn assert_transit_links(
    address: &str,
    roles: &[&str; 3],
    chat_owner: &str,
    reader: Option<&str>,
) -> Result<Option<String>> {
    validate_transit_links(
        &product::fetch_status(address).await?,
        roles,
        chat_owner,
        reader,
    )
}

fn validate_transit_links(
    status: &Value,
    roles: &[&str; 3],
    chat_owner: &str,
    reader: Option<&str>,
) -> Result<Option<String>> {
    let peers = status["fips"]["peer_statuses"]
        .as_array()
        .context("transit omitted FIPS peers")?;
    let connected = peers
        .iter()
        .filter(|peer| peer["connected"] == true)
        .collect::<Vec<_>>();
    for role in roles {
        ensure!(
            connected
                .iter()
                .filter(|peer| peer["npub"] == *role)
                .count()
                == 1,
            "intended identified role missing or duplicated at transit"
        );
    }
    ensure!(
        connected.iter().all(|peer| peer["transport_type"] == "udp"),
        "mesh used an unexpected carrier"
    );
    let extras = connected
        .iter()
        .filter(|peer| !roles.iter().any(|role| peer["npub"] == *role))
        .collect::<Vec<_>>();
    ensure!(
        extras.len() <= 1,
        "transit connected outside the isolated roles and reader"
    );
    let observed = extras
        .first()
        .map(|peer| {
            peer["npub"]
                .as_str()
                .context("anonymous adjacency omitted authenticated identity")
        })
        .transpose()?;
    if let Some(observed) = observed {
        ensure!(
            observed != chat_owner,
            "attachment reader used the Chat account identity"
        );
    }
    if let Some(reader) = reader {
        ensure!(
            observed == Some(reader),
            "anonymous reader adjacency changed or disappeared"
        );
    }
    Ok(observed.map(str::to_string))
}

#[test]
fn transit_roles_reject_missing_roles_identified_reader_and_extra_peers() {
    let roles = ["chat-device", "drive-device", "public-provider"];
    let status = |ids: &[&str]| {
        json!({"fips": {"peer_statuses": ids.iter().map(|id|
        json!({"npub": id, "connected": true, "transport_type": "udp"})).collect::<Vec<_>>()}})
    };
    assert!(
        validate_transit_links(&status(&roles), &roles, "chat-account", None)
            .unwrap()
            .is_none()
    );
    let valid = status(&[roles[0], roles[1], roles[2], "anonymous-reader"]);
    assert_eq!(
        validate_transit_links(&valid, &roles, "chat-account", None)
            .unwrap()
            .as_deref(),
        Some("anonymous-reader")
    );
    assert!(
        validate_transit_links(&valid, &roles, "chat-account", Some("anonymous-reader")).is_ok()
    );
    for invalid in [
        status(&[roles[0], roles[1], "anonymous-reader"]),
        status(&[roles[0], roles[1], roles[2], "chat-account"]),
        status(&[
            roles[0],
            roles[1],
            roles[2],
            "anonymous-reader",
            "unrelated",
        ]),
        status(&[roles[0], roles[1], roles[2], roles[0]]),
    ] {
        assert!(validate_transit_links(&invalid, &roles, "chat-account", None).is_err());
    }
    assert!(
        validate_transit_links(&valid, &roles, "chat-account", Some("different-reader")).is_err()
    );
}

async fn wait_for_reader(
    address: &str,
    roles: &[&str; 3],
    chat_owner: &str,
    previous: Option<&str>,
) -> Result<String> {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(reader) = assert_transit_links(address, roles, chat_owner, previous).await?
            {
                return Ok(reader);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("anonymous attachment reader did not join the transit")?
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
    cid: String,
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
        cid: stored["cid"]
            .as_str()
            .context("Drive put omitted CID")?
            .to_string(),
        nhash: nhash.to_string(),
        bytes,
    })
}

async fn put_public_blob(
    htree_bin: &std::path::Path,
    provider: &HtreeNode,
    label: &str,
) -> Result<Blob> {
    let bytes = payload(label, 192 * 1024 + 37);
    let cid = add_blob(htree_bin, provider, &format!("{label}.bin"), &bytes).await?;
    Ok(Blob {
        nhash: nhash_for_cid(&cid)?,
        cid,
        bytes,
    })
}

async fn assert_private_blob_unavailable(
    chat: &mut ManagedProcess,
    drive: &mut ManagedProcess,
    blob: &Blob,
) -> Result<u128> {
    // Positive cache control: the private owner can still read its exact bytes.
    drive.send_line(&format!("fetch {}", blob.cid)).await?;
    let local = drive.json_event("fetch").await?;
    ensure!(
        local.get("error").is_none()
            && local["root_cached"] == true
            && local["sha256"] == payload_sha256(&blob.bytes),
        "Drive private cache control failed"
    );
    let started = Instant::now();
    chat.send_line(&format!("fetch {}", blob.nhash)).await?;
    let denied = timeout(Duration::from_secs(45), chat.json_event("fetch"))
        .await
        .context("private Drive cache probe did not finish within its bound")??;
    ensure!(
        denied["fetched"] == 0
            && denied["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty()),
        "anonymous reader disclosed a Drive-only private blob: {denied}"
    );
    // The released interface cannot distinguish an ACL close from unavailable
    // routing. Do not claim this is an explicit-denial or per-GET identity trace.
    Ok(started.elapsed().as_millis())
}

async fn with_mesh_diagnostics<T>(
    result: Result<T>,
    chat: &mut ManagedProcess,
    other_processes: [(&str, &mut ManagedProcess); 3],
    status_addresses: [(&str, &str); 2],
) -> Result<T> {
    if let Err(error) = &result {
        eprintln!("mesh check failed: {error:#}");
        // Capture actual authenticated peer state and every process's logs while
        // the processes are alive, before TestRoot/ManagedProcess cleanup.
        for (label, address) in status_addresses {
            eprintln!(
                "{label} FIPS status (bounded): {}",
                diagnostic_tail(&format!("{:?}", product::fetch_status(address).await))
            );
        }
        for (label, process) in std::iter::once(("Chat", chat)).chain(other_processes) {
            for (stream, text) in [
                ("stderr", process.stderr_snapshot()),
                ("stdout", process.stdout_snapshot()),
            ] {
                let tail = diagnostic_tail(&text);
                eprintln!("{label} {stream} (last 80 lines, at most 16 KiB):\n{tail}");
            }
        }
    }
    result
}

fn diagnostic_tail(text: &str) -> String {
    let mut start = text.len().saturating_sub(16 * 1024);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].lines().rev().take(80).collect::<Vec<_>>()
        .into_iter().rev().collect::<Vec<_>>().join("\n")
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
