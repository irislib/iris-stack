#[path = "support/product.rs"]
mod product;
mod support;

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use product::{
    HtreeNode, NodeConfig, TestRoot, add_blob, cat_blob, drive_identity, fetch_status,
    fips_udp_peer_connected, htree_identity, payload, required_binary, reserve_tcp_address,
    reserve_udp_address, spawn_htree, wait_for_fips_peer_connection,
};
use serde_json::Value;
use support::process::ManagedProcess;
use tokio::process::Command;
use tokio::time::timeout;

static PRODUCT_MATRIX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires released htree and the Iris Drive product fixture; run scripts/product-lab.sh"]
async fn drive_survives_provider_replacement_on_its_owned_udp_route() {
    let _guard = PRODUCT_MATRIX_LOCK.lock().await;
    run_product_scenario().await.unwrap();
}

async fn run_product_scenario() -> Result<()> {
    let htree_bin = required_binary("IRIS_STACK_HTREE_BIN")?;
    let drive_bin = required_binary("IRIS_STACK_DRIVE_FIXTURE_BIN")?;
    let root = TestRoot::new()?;
    let remote = HtreeNode::new(root.path(), "remote");
    let provider = HtreeNode::new(root.path(), "provider");
    let replacement = HtreeNode::new(root.path(), "replacement");
    let remote_rendezvous = reserve_udp_address()?;
    let local_rendezvous = reserve_udp_address()?;
    let remote_udp = reserve_udp_address()?;
    let provider_udp = reserve_udp_address()?;
    let replacement_udp = reserve_udp_address()?;
    let drive_udp = reserve_udp_address()?;
    let remote_http = reserve_tcp_address()?;
    let provider_http = reserve_tcp_address()?;
    let replacement_http = reserve_tcp_address()?;

    remote.write_config(NodeConfig {
        http_addr: &remote_http,
        udp_addr: &remote_udp,
        rendezvous_addr: &remote_rendezvous,
        peers: &[],
    })?;
    let remote_npub = htree_identity(&htree_bin, &remote).await?;
    provider.write_config(NodeConfig {
        http_addr: &provider_http,
        udp_addr: &provider_udp,
        rendezvous_addr: &local_rendezvous,
        peers: &[(&remote_npub, &remote_udp)],
    })?;
    let provider_npub = htree_identity(&htree_bin, &provider).await?;
    let drive_key = root.path().join("drive-app-key");
    let drive_identity = drive_identity(&drive_bin, &drive_key).await?;
    remote.write_config(NodeConfig {
        http_addr: &remote_http,
        udp_addr: &remote_udp,
        rendezvous_addr: &remote_rendezvous,
        peers: &[
            (&provider_npub, &provider_udp),
            (&drive_identity.npub, &drive_udp),
        ],
    })?;

    let first_bytes = payload("via-provider", 192 * 1024 + 17);
    let second_bytes = payload("after-provider-death", 192 * 1024 + 31);
    let replacement_bytes = payload("replacement-provider", 192 * 1024 + 47);
    let first_cid = add_blob(&htree_bin, &provider, "first.bin", &first_bytes).await?;
    let second_cid = add_blob(&htree_bin, &remote, "second.bin", &second_bytes).await?;
    replacement.write_config(NodeConfig {
        http_addr: &replacement_http,
        udp_addr: &replacement_udp,
        rendezvous_addr: &local_rendezvous,
        peers: &[],
    })?;
    let replacement_npub = htree_identity(&htree_bin, &replacement).await?;
    ensure!(replacement_npub != provider_npub);
    let replacement_cid = add_blob(
        &htree_bin,
        &replacement,
        "replacement.bin",
        &replacement_bytes,
    )
    .await?;

    let mut remote_process = spawn_htree(&htree_bin, &remote, &remote_http, "remote htree")?;
    remote_process.line_containing("FIPS: enabled").await?;
    let mut provider_process =
        spawn_htree(&htree_bin, &provider, &provider_http, "provider htree")?;
    provider_process.line_containing("FIPS: enabled").await?;
    if let Err(error) = wait_for_fips_peer_connection(&provider_http, &remote_npub).await {
        let output = provider_process.kill().await?;
        bail!(
            "{error:#}; provider stdout:\n{}\nprovider stderr:\n{}",
            output.stdout,
            output.stderr
        );
    }

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
    drive.send_line(&format!("fetch {first_cid}")).await?;
    let first = match drive.json_event("fetch").await {
        Ok(value) => value,
        Err(error) => {
            let diagnostics = drive
                .finish()
                .await
                .expect_err("Drive exited cleanly without its fetch event");
            let provider_output = provider_process.kill().await?;
            let remote_output = remote_process.kill().await?;
            bail!(
                "{error:#}; Drive diagnostics: {diagnostics:#}; provider stdout:\n{}\nprovider stderr:\n{}\nremote stdout:\n{}\nremote stderr:\n{}",
                provider_output.stdout,
                provider_output.stderr,
                remote_output.stdout,
                remote_output.stderr,
            );
        }
    };
    assert_fetch(&first, &first_cid, &remote_udp)?;

    let provider_status = fetch_status(&provider_http).await?;
    ensure!(
        fips_udp_peer_connected(&provider_status, &remote_npub),
        "provider lost its authenticated remote UDP FIPS route during the HTL retrieval"
    );

    let provider_exit = provider_process.kill().await?;
    ensure!(
        !provider_exit.status.success(),
        "forced provider exit succeeded"
    );
    assert_no_lan_discovery(&provider_exit.stdout, &provider_exit.stderr)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &first_cid).await? == first_bytes,
        "the actual htree provider did not cache the blob it resolved over HTL"
    );

    drive.send_line(&format!("fetch {second_cid}")).await?;
    let second = match drive.json_event("fetch").await {
        Ok(value) => value,
        Err(error) => {
            let diagnostics = drive
                .finish()
                .await
                .expect_err("Drive exited cleanly without its fallback fetch event");
            let remote_output = remote_process.kill().await?;
            bail!(
                "{error:#}; Drive diagnostics: {diagnostics:#}; remote stdout:\n{}\nremote stderr:\n{}",
                remote_output.stdout,
                remote_output.stderr,
            );
        }
    };
    assert_fetch(&second, &second_cid, &remote_udp)?;
    ensure!(
        cat_blob(&htree_bin, &provider, &second_cid).await.is_err(),
        "the dead provider unexpectedly contained the post-death blob"
    );

    let mut replacement_process = spawn_htree(
        &htree_bin,
        &replacement,
        &replacement_http,
        "replacement htree",
    )?;
    replacement_process.line_containing("FIPS: enabled").await?;
    wait_for_drive_provider_replacement(&mut drive, &remote_udp, &provider_npub, &replacement_npub)
        .await?;
    drive.send_line(&format!("fetch {replacement_cid}")).await?;
    let replacement_fetch = drive.json_event("fetch").await?;
    assert_fetch(&replacement_fetch, &replacement_cid, &remote_udp)?;
    ensure!(
        cat_blob(&htree_bin, &replacement, &replacement_cid).await? == replacement_bytes,
        "the replacement provider lost its local blob"
    );
    ensure!(
        cat_blob(&htree_bin, &remote, &replacement_cid)
            .await
            .is_err(),
        "the standalone remote unexpectedly contained the replacement-only blob"
    );

    drive.send_line("status").await?;
    let final_status = drive.json_event("status").await?;
    assert_drive_udp(&final_status, &remote_udp)?;
    drive.send_line("stop").await?;
    drive.json_event("stopped").await?;
    let drive_output = drive.finish().await?;
    ensure!(drive_output.status.success());

    let replacement_exit = replacement_process.kill().await?;
    ensure!(
        !replacement_exit.status.success(),
        "forced replacement provider exit succeeded"
    );
    assert_no_lan_discovery(&replacement_exit.stdout, &replacement_exit.stderr)?;

    let remote_exit = remote_process.kill().await?;
    ensure!(
        !remote_exit.status.success(),
        "forced remote exit succeeded"
    );
    assert_no_lan_discovery(&remote_exit.stdout, &remote_exit.stderr)?;
    eprintln!(
        "product lab passed: Drive {}, local htree {} -> {}, remote htree {}",
        ready["npub"], provider_npub, replacement_npub, remote_npub
    );
    Ok(())
}

async fn wait_for_drive_provider_replacement(
    drive: &mut ManagedProcess,
    remote_udp: &str,
    gone_provider_npub: &str,
    replacement_npub: &str,
) -> Result<()> {
    timeout(Duration::from_secs(30), async {
        loop {
            drive.send_line("status").await?;
            let status = drive.json_event("status").await?;
            let providers = status["same_host_blob_providers"]
                .as_array()
                .context("Drive status omitted same-host blob providers")?;
            let gone = providers
                .iter()
                .any(|provider| provider == gone_provider_npub);
            let replacement = providers
                .iter()
                .any(|provider| provider == replacement_npub);
            if assert_drive_udp(&status, remote_udp).is_ok() && !gone && replacement {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("Drive did not withdraw the dead provider and adopt its replacement")?
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

fn assert_fetch(value: &Value, cid: &str, remote_udp: &str) -> Result<()> {
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
