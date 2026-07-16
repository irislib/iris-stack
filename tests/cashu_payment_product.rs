mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use cashu_service::simulation::{
    IssuerMode, LocalMint, PaymentNetwork, SettlementAccountingSnapshot, VirtualClock,
};
use cashu_service::{create_topup_quote, load_mint_balance, open_wallet_repository};
use support::process::{ManagedProcess, TestRoot};
use tokio::process::Command;
use tokio::time::timeout;

static CASHU_MATRIX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_cashu_service_payment_recovers_and_rejects_replay_across_processes() {
    let _guard = CASHU_MATRIX_LOCK.lock().await;
    timeout(Duration::from_secs(120), run_scenario())
        .await
        .context("Cashu product scenario exceeded its bounded deadline")
        .unwrap()
        .unwrap();
}

async fn run_scenario() -> Result<()> {
    let fixture = PathBuf::from(env!("CARGO_BIN_EXE_cashu-stack-fixture"));
    let root = TestRoot::new()?;
    let payer_wallet = root.path().join("payer-wallet");
    let payout_journal = root.path().join("payout.json");
    let clock = Arc::new(VirtualClock::new(1_700_000_000));
    let network = PaymentNetwork::new(42, 1, clock);
    let source = LocalMint::start(
        root.path(),
        network.clone(),
        "source",
        IssuerMode::Withdrawable,
    )
    .await?;
    let destination = LocalMint::start(
        root.path(),
        network.clone(),
        "destination",
        IssuerMode::ClosedLoop,
    )
    .await?;
    fund_payer(&payer_wallet, &source, &network).await?;
    assert_accounting(&network.accounting()?, 64, 0, 0, 64)?;

    network.set_online("destination", false)?;

    let mut failed_payer = spawn_payer(
        &fixture,
        root.path(),
        &payer_wallet,
        source.url(),
        destination.url(),
        "payer before outage",
    )?;
    ensure!(failed_payer.json_event("ready").await?["pending_settlements"] == 1);
    failed_payer.send_line("settle").await?;
    let failed = failed_payer.json_event("settlement_failed").await?;
    ensure!(failed["attempt_scope"] == "this_route_attempt");
    ensure!(failed["will_retry"] == true);
    ensure!(!payout_journal.exists());
    let failed_exit = failed_payer.kill().await?;
    ensure!(!failed_exit.status.success());

    assert_accounting(&network.accounting()?, 64, 0, 0, 64)?;
    network.set_online("destination", true)?;

    let mut payer = spawn_payer(
        &fixture,
        root.path(),
        &payer_wallet,
        source.url(),
        destination.url(),
        "payer after outage",
    )?;
    payer.json_event("ready").await?;
    payer.send_line("settle").await?;
    let first_payout = payer.json_event("payout_ready").await?;
    ensure!(first_payout["journal_reused"] == false);
    ensure!(first_payout["fee_paid_sat"] == 1);
    ensure!(payout_journal.is_file());
    let crashed_exit = payer.kill().await?;
    ensure!(!crashed_exit.status.success());

    let mut replacement = spawn_payer(
        &fixture,
        root.path(),
        &payer_wallet,
        source.url(),
        destination.url(),
        "replacement payer",
    )?;
    ensure!(replacement.json_event("ready").await?["pending_settlements"] == 1);
    replacement.send_line("resume").await?;
    let resumed = replacement.json_event("payout_ready").await?;
    ensure!(resumed["journal_reused"] == true);
    for field in [
        "transfer_id",
        "source_melt_quote_id",
        "destination_mint_quote_id",
        "fee_paid_sat",
        "payout_operation_id",
    ] {
        ensure!(
            resumed[field] == first_payout[field],
            "changed {field} after restart"
        );
    }

    let provider_wallet = root.path().join("provider-wallet");
    let mut provider = spawn_receiver(
        &fixture,
        &provider_wallet,
        &payout_journal,
        "accept",
        "service provider",
    )?;
    let accepted = provider.json_event("payment_accepted").await?;
    ensure!(accepted["amount_sat"] == 32);
    ensure!(accepted["balance_sat"] == 32);
    ensure!(provider.finish().await?.status.success());

    let replay_wallet = root.path().join("replay-wallet");
    let mut replay = spawn_receiver(
        &fixture,
        &replay_wallet,
        &payout_journal,
        "reject",
        "replay receiver",
    )?;
    let rejected = replay.json_event("replay_rejected").await?;
    ensure!(rejected["balance_sat"] == 0);
    ensure!(rejected["reason"] == "mint rejected already-spent proofs");
    ensure!(replay.finish().await?.status.success());

    replacement.send_line("ack").await?;
    let complete = replacement.json_event("settlement_complete").await?;
    ensure!(complete["pending_settlements"] == 0);
    ensure!(complete["total_deposited_sat"] == 64);
    ensure!(complete["available_sat"] == 31);
    ensure!(complete["pending_external_sat"] == 0);
    ensure!(complete["settled_external_sat"] == 33);
    ensure!(complete["conserved_sat"] == 64);

    assert_accounting(&network.accounting()?, 31, 32, 1, 64)?;

    replacement.send_line("stop").await?;
    replacement.json_event("stopped").await?;
    ensure!(replacement.finish().await?.status.success());
    Ok(())
}

async fn fund_payer(
    payer_wallet: &Path,
    source: &LocalMint,
    network: &PaymentNetwork,
) -> Result<()> {
    let quote = create_topup_quote(payer_wallet, source.url(), 64).await?;
    network
        .orchestrator_funding()
        .settle_external(&quote.payment_request)
        .context("fund payer through the simulated Lightning network")?;
    let repository = open_wallet_repository(payer_wallet).await?;
    let wallet = repository
        .get_wallets()
        .await
        .into_iter()
        .find(|wallet| wallet.mint_url.to_string() == source.url())
        .context("source wallet was not created")?;
    wallet.check_mint_quote_status(&quote.quote_id).await?;
    wallet
        .mint(&quote.quote_id, Default::default(), None)
        .await?;
    ensure!(
        load_mint_balance(payer_wallet, source.url())
            .await?
            .balance_sat
            == 64
    );
    Ok(())
}

fn spawn_payer(
    fixture: &Path,
    root: &Path,
    payer_wallet: &Path,
    source_url: &str,
    destination_url: &str,
    label: &str,
) -> Result<ManagedProcess> {
    spawn(
        fixture,
        label,
        &[
            "payer".into(),
            root.as_os_str().into(),
            payer_wallet.as_os_str().into(),
            source_url.into(),
            destination_url.into(),
        ],
    )
}

fn spawn_receiver(
    fixture: &Path,
    wallet: &Path,
    journal: &Path,
    expectation: &str,
    label: &str,
) -> Result<ManagedProcess> {
    spawn(
        fixture,
        label,
        &[
            "receiver".into(),
            wallet.as_os_str().into(),
            journal.as_os_str().into(),
            expectation.into(),
        ],
    )
}

fn spawn(fixture: &Path, label: &str, args: &[std::ffi::OsString]) -> Result<ManagedProcess> {
    let mut command = Command::new(fixture);
    command.args(args);
    ManagedProcess::spawn(label, &mut command)
}

fn assert_accounting(
    value: &SettlementAccountingSnapshot,
    source: u64,
    destination: u64,
    fee: u64,
    total: u64,
) -> Result<()> {
    ensure!(value.mint_reserve("source") == Some(source));
    ensure!(value.mint_reserve("destination") == Some(destination));
    ensure!(value.fee_sink_sat == fee);
    ensure!(value.total_accounted_sat == total);
    ensure!(value.external_funding_sat == total);
    ensure!(value.is_conserved());
    Ok(())
}
