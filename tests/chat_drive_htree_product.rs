#[allow(dead_code)]
#[path = "support/product.rs"]
mod product;
mod support;

use std::time::Duration;

use anyhow::{Context, Result, ensure};
use hashtree_core::BLOB_DEFAULT_HTL;
use product::{
    HtreeNode, NodeConfig, TestRoot, add_blob, cat_blob, drive_identity, htree_identity,
    nhash_for_cid, payload, payload_sha256, required_binary, reserve_tcp_address,
    reserve_udp_address, spawn_htree, wait_for_htree_fips_peer, write_hashtree_read_config,
};
use serde_json::Value;
use support::process::ManagedProcess;
use tokio::process::Command;
use tokio::time::timeout;

static PRODUCT_MATRIX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires released htree plus Iris Drive and Chat product fixtures; run scripts/product-lab.sh"]
async fn chat_and_drive_share_htree_without_losing_their_standalone_routes() {
    let _guard = PRODUCT_MATRIX_LOCK.lock().await;
    run_product_scenario().await.unwrap();
}

async fn run_product_scenario() -> Result<()> {
    let htree_bin = required_binary("IRIS_STACK_HTREE_BIN")?;
    let drive_bin = required_binary("IRIS_STACK_DRIVE_FIXTURE_BIN")?;
    let chat_bin = required_binary("IRIS_STACK_CHAT_FIXTURE_BIN")?;
    let root = TestRoot::new()?;
    let remote = HtreeNode::new(root.path(), "remote");
    let mesh_remote = HtreeNode::new(root.path(), "mesh-remote");
    let provider = HtreeNode::new(root.path(), "provider");
    let remote_rendezvous = reserve_udp_address()?;
    let mesh_rendezvous = reserve_udp_address()?;
    let local_rendezvous = reserve_udp_address()?;
    let remote_udp = reserve_udp_address()?;
    let mesh_remote_udp = reserve_udp_address()?;
    let provider_udp = reserve_udp_address()?;
    let drive_udp = reserve_udp_address()?;
    let remote_http = reserve_tcp_address()?;
    let mesh_remote_http = reserve_tcp_address()?;
    let provider_http = reserve_tcp_address()?;
    let chat_config = root.path().join("chat-hashtree-config");
    let chat_data = root.path().join("chat-data");

    remote.write_config(NodeConfig {
        http_addr: &remote_http,
        udp_addr: &remote_udp,
        rendezvous_addr: &remote_rendezvous,
        peers: &[],
    })?;
    let remote_npub = htree_identity(&htree_bin, &remote).await?;
    mesh_remote.write_config(NodeConfig {
        http_addr: &mesh_remote_http,
        udp_addr: &mesh_remote_udp,
        rendezvous_addr: &mesh_rendezvous,
        peers: &[],
    })?;
    let mesh_remote_npub = htree_identity(&htree_bin, &mesh_remote).await?;
    provider.write_config(NodeConfig {
        http_addr: &provider_http,
        udp_addr: &provider_udp,
        rendezvous_addr: &local_rendezvous,
        peers: &[(&mesh_remote_npub, &mesh_remote_udp)],
    })?;
    let provider_npub = htree_identity(&htree_bin, &provider).await?;
    mesh_remote.write_config(NodeConfig {
        http_addr: &mesh_remote_http,
        udp_addr: &mesh_remote_udp,
        rendezvous_addr: &mesh_rendezvous,
        peers: &[(&provider_npub, &provider_udp)],
    })?;
    let drive_key = root.path().join("drive-app-key");
    let drive_identity = drive_identity(&drive_bin, &drive_key).await?;
    remote.write_config(NodeConfig {
        http_addr: &remote_http,
        udp_addr: &remote_udp,
        rendezvous_addr: &remote_rendezvous,
        peers: &[(&drive_identity.npub, &drive_udp)],
    })?;

    let provider_bytes = payload("shared-provider", 192 * 1024 + 17);
    let mesh_bytes = payload("provider-routed-htl", 192 * 1024 + 23);
    let standalone_bytes = payload("standalone-after-miss", 192 * 1024 + 31);
    let after_death_bytes = payload("standalone-after-death", 192 * 1024 + 47);
    let provider_cid = add_blob(&htree_bin, &provider, "provider.bin", &provider_bytes).await?;
    let mesh_cid = add_blob(&htree_bin, &mesh_remote, "mesh.bin", &mesh_bytes).await?;
    let standalone_cid = add_blob(&htree_bin, &remote, "standalone.bin", &standalone_bytes).await?;
    let after_death_cid =
        add_blob(&htree_bin, &remote, "after-death.bin", &after_death_bytes).await?;
    let provider_nhash = nhash_for_cid(&provider_cid)?;
    let standalone_nhash = nhash_for_cid(&standalone_cid)?;
    let after_death_nhash = nhash_for_cid(&after_death_cid)?;

    let mut remote_process = spawn_htree(&htree_bin, &remote, &remote_http, "remote htree")?;
    remote_process.line_containing("FIPS: enabled").await?;
    let mut mesh_remote_process = spawn_htree(
        &htree_bin,
        &mesh_remote,
        &mesh_remote_http,
        "mesh-only remote htree",
    )?;
    mesh_remote_process.line_containing("FIPS: enabled").await?;
    let mut provider_process =
        spawn_htree(&htree_bin, &provider, &provider_http, "provider htree")?;
    provider_process.line_containing("FIPS: enabled").await?;
    wait_for_htree_fips_peer(&provider_http, &mesh_remote_npub).await?;

    write_hashtree_read_config(&chat_config, &format!("http://{remote_http}"))?;
    let mut chat_command = Command::new(&chat_bin);
    chat_command
        .arg("run")
        .arg(&chat_data)
        .env("IRIS_CHAT_SAME_HOST_HASHTREE", "1")
        .env("IRIS_CHAT_FIPS_LOCAL_RENDEZVOUS_ADDR", &local_rendezvous)
        .env("HTREE_CONFIG_DIR", &chat_config)
        .env(
            "RUST_LOG",
            std::env::var("IRIS_STACK_PRODUCT_LOG").unwrap_or_else(|_| "warn".to_string()),
        );
    let mut chat = ManagedProcess::spawn("Iris Chat fixture", &mut chat_command)?;
    let chat_ready = chat.json_event("ready").await?;
    ensure!(
        chat_ready["npub"]
            .as_str()
            .is_some_and(|npub| npub.starts_with("npub1")),
        "Chat fixture did not report its authenticated device identity"
    );

    let mut drive_command = Command::new(&drive_bin);
    drive_command
        .arg("run")
        .arg(&remote_npub)
        .arg(&drive_key)
        .arg(&drive_identity.profile_id)
        .env("IRIS_DRIVE_FIPS_LOCAL_RENDEZVOUS_ADDR", &local_rendezvous)
        .env("IRIS_DRIVE_FIPS_UDP_BIND_ADDR", &drive_udp)
        .env("IRIS_DRIVE_FIPS_UDP_PUBLIC", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_UDP", "true")
        .env("IRIS_DRIVE_FIPS_ENABLE_WEBRTC", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_LAN_DISCOVERY", "false")
        .env("IRIS_DRIVE_FIPS_ENABLE_MESH_PUBSUB", "false")
        .env("IRIS_DRIVE_FIPS_SHARE_LOCAL_CANDIDATES", "false")
        .env(
            "IRIS_DRIVE_FIPS_STATIC_PEERS",
            format!("{remote_npub}={remote_udp}"),
        )
        .env("IRIS_DRIVE_FIPS_ENABLE_BOOTSTRAP", "false")
        .env(
            "RUST_LOG",
            std::env::var("IRIS_STACK_PRODUCT_LOG").unwrap_or_else(|_| "warn".to_string()),
        );
    let mut drive = ManagedProcess::spawn("Iris Drive fixture", &mut drive_command)?;
    let ready = drive.json_event("ready").await?;
    ensure!(ready["remote_npub"] == remote_npub);
    ensure!(ready["npub"] == drive_identity.npub);
    ensure!(ready["discovery_scope"] == drive_identity.discovery_scope);

    wait_for_drive_topology(&mut drive, &remote_udp, &provider_npub).await?;
    let chat_provider = wait_for_chat_fetch(&mut chat, &provider_nhash, &provider_bytes).await?;
    assert_chat_fetch(&chat_provider, &provider_nhash, &provider_bytes)?;
    let drive_provider = fetch_drive(&mut drive, &provider_cid).await?;
    assert_drive_fetch(&drive_provider, &provider_cid, &remote_udp)?;

    ensure!(
        BLOB_DEFAULT_HTL == 10 && BLOB_DEFAULT_HTL.saturating_sub(1) == 9,
        "released Hashtree default HTL no longer exposes the expected one-hop budget"
    );
    let drive_mesh = fetch_drive(&mut drive, &mesh_cid).await?;
    assert_drive_fetch(&drive_mesh, &mesh_cid, &remote_udp)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &mesh_cid).await? == mesh_bytes,
        "the same-host provider did not cache its one-hop Hashtree result"
    );
    ensure!(
        cat_blob(&htree_bin, &remote, &mesh_cid).await.is_err(),
        "the Drive-owned standalone route unexpectedly contained the HTL-only blob"
    );

    let chat_standalone = fetch_chat(&mut chat, &standalone_nhash).await?;
    assert_chat_fetch(&chat_standalone, &standalone_nhash, &standalone_bytes)?;
    let drive_standalone = fetch_drive(&mut drive, &standalone_cid).await?;
    assert_drive_fetch(&drive_standalone, &standalone_cid, &remote_udp)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &standalone_cid)
            .await
            .is_err(),
        "the same-host provider cached a blob that only its standalone route supplied"
    );

    let provider_exit = provider_process.kill().await?;
    ensure!(
        !provider_exit.status.success(),
        "forced provider exit succeeded"
    );
    assert_no_lan_discovery(&provider_exit.stdout, &provider_exit.stderr)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &provider_cid).await? == provider_bytes,
        "the shared provider lost its local blob"
    );

    let chat_after_death = fetch_chat(&mut chat, &after_death_nhash).await?;
    assert_chat_fetch(&chat_after_death, &after_death_nhash, &after_death_bytes)?;
    let drive_after_death = fetch_drive(&mut drive, &after_death_cid).await?;
    assert_drive_fetch(&drive_after_death, &after_death_cid, &remote_udp)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &after_death_cid)
            .await
            .is_err(),
        "the dead provider unexpectedly contained the post-death blob"
    );

    chat.send_line("status").await?;
    let chat_status = chat.json_event("status").await?;
    ensure!(chat_status["npub"] == chat_ready["npub"]);

    drive.send_line("status").await?;
    let final_status = drive.json_event("status").await?;
    assert_drive_udp(&final_status, &remote_udp)?;
    drive.send_line("stop").await?;
    drive.json_event("stopped").await?;
    let drive_output = drive.finish().await?;
    ensure!(drive_output.status.success());

    chat.send_line("stop").await?;
    chat.json_event("stopped").await?;
    let chat_output = chat.finish().await?;
    ensure!(chat_output.status.success());
    assert_no_lan_discovery(&chat_output.stdout, &chat_output.stderr)?;

    let remote_exit = remote_process.kill().await?;
    ensure!(
        !remote_exit.status.success(),
        "forced remote exit succeeded"
    );
    assert_no_lan_discovery(&remote_exit.stdout, &remote_exit.stderr)?;
    let mesh_remote_exit = mesh_remote_process.kill().await?;
    ensure!(
        !mesh_remote_exit.status.success(),
        "forced mesh-only remote exit succeeded"
    );
    assert_no_lan_discovery(&mesh_remote_exit.stdout, &mesh_remote_exit.stderr)?;
    eprintln!(
        "product lab passed: Chat {}, Drive {}, shared htree {}, HTL remote {}, standalone remote {}",
        chat_ready["npub"], ready["npub"], provider_npub, mesh_remote_npub, remote_npub
    );
    Ok(())
}

async fn wait_for_drive_topology(
    drive: &mut ManagedProcess,
    remote_udp: &str,
    provider_npub: &str,
) -> Result<()> {
    timeout(Duration::from_secs(30), async {
        loop {
            drive.send_line("status").await?;
            let status = drive.json_event("status").await?;
            let provider_visible =
                status["same_host_blob_providers"]
                    .as_array()
                    .is_some_and(|providers| {
                        providers.iter().any(|provider| provider == provider_npub)
                    });
            if assert_drive_udp(&status, remote_udp).is_ok() && provider_visible {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("Drive topology did not converge to its UDP route and same-host blob provider")?
}

async fn wait_for_chat_fetch(
    chat: &mut ManagedProcess,
    nhash: &str,
    expected: &[u8],
) -> Result<Value> {
    timeout(Duration::from_secs(30), async {
        loop {
            let value = fetch_chat(chat, nhash).await?;
            if assert_chat_fetch(&value, nhash, expected).is_ok() {
                return Ok::<_, anyhow::Error>(value);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("Chat did not discover and reuse the same-host blob provider")?
}

async fn fetch_chat(chat: &mut ManagedProcess, nhash: &str) -> Result<Value> {
    chat.send_line(&format!("fetch {nhash}")).await?;
    chat.json_event("fetch").await
}

fn assert_chat_fetch(value: &Value, nhash: &str, expected: &[u8]) -> Result<()> {
    ensure!(value["nhash"] == nhash);
    ensure!(value["fetched"] == expected.len() as u64);
    ensure!(value["sha256"] == payload_sha256(expected));
    ensure!(value.get("error").is_none());
    Ok(())
}

async fn fetch_drive(drive: &mut ManagedProcess, cid: &str) -> Result<Value> {
    drive.send_line(&format!("fetch {cid}")).await?;
    drive.json_event("fetch").await
}

fn assert_drive_fetch(value: &Value, cid: &str, remote_udp: &str) -> Result<()> {
    ensure!(value["cid"] == cid);
    ensure!(value["fetched"].as_u64().unwrap_or(0) > 0);
    ensure!(value["root_cached"] == true);
    assert_drive_udp(value, remote_udp)
}

fn assert_drive_udp(value: &Value, remote_udp: &str) -> Result<()> {
    ensure!(value["remote_connected"] == true);
    ensure!(value["remote_transport"] == "udp");
    ensure!(value["remote_addr"] == remote_udp);
    Ok(())
}

fn assert_no_lan_discovery(stdout: &str, stderr: &str) -> Result<()> {
    ensure!(
        !stdout.contains("mDNS discovery started")
            && !stdout.contains("LAN mDNS discovery enabled")
            && !stderr.contains("mDNS discovery started")
            && !stderr.contains("LAN mDNS discovery enabled"),
        "product gate used host-LAN discovery instead of isolated loopback links"
    );
    Ok(())
}
