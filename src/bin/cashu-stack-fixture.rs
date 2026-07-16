use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use cashu_credit::{
    AcceptanceMode, AccountPolicy, BackingDeposit, CreditAccount, ExternalSettlementRequest,
    IssuerPolicy, ServiceReceiptClaim, ValueClass,
};
use cashu_service::{
    CashuCrossMintTransfer, CashuIssuerRoute, CashuSentPayment, CreditAccountStore,
    execute_cashu_settlement, load_mint_balance, normalize_mint_url, receive_payment_token,
    send_payment_token,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

const START_TIME: u64 = 1_700_000_000;
const ACCOUNT_ID: &str = "provider";
const SETTLEMENT_ID: &str = "iris-stack-service-payment";
const FUNDING_SAT: u64 = 64;
const SERVICE_SAT: u64 = 32;
const MAX_FEE_SAT: u64 = 1;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let role = text_arg(&mut args, "role")?;
    match role.as_str() {
        "payer" => {
            let root = path_arg(&mut args, "root")?;
            let payer_wallet = path_arg(&mut args, "payer wallet")?;
            let source_url = text_arg(&mut args, "source mint URL")?;
            let destination_url = text_arg(&mut args, "destination mint URL")?;
            ensure_no_args(args)?;
            run_payer(&root, &payer_wallet, &source_url, &destination_url).await
        }
        "receiver" => {
            let receiver_wallet = path_arg(&mut args, "receiver wallet")?;
            let payout_journal = path_arg(&mut args, "payout journal")?;
            let expectation = text_arg(&mut args, "accept or reject")?;
            ensure_no_args(args)?;
            run_receiver(&receiver_wallet, &payout_journal, &expectation).await
        }
        _ => bail!("unknown fixture role {role}"),
    }
}

async fn run_payer(
    root: &Path,
    payer_wallet: &Path,
    source_url: &str,
    destination_url: &str,
) -> Result<()> {
    let source_url = normalize_mint_url(source_url)?;
    let destination_url = normalize_mint_url(destination_url)?;
    let store_path = root.join("credit.sqlite3");
    let payout_path = root.join("payout.json");
    initialize_credit_account(&store_path, &source_url, &destination_url)?;
    let pending = CreditAccountStore::open(&store_path)?
        .load(ACCOUNT_ID)?
        .context("credit account disappeared")?
        .pending_external_settlement_authorizations()
        .len();
    emit(json!({"event": "ready", "role": "payer", "pending_settlements": pending}))?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        match line.trim() {
            "settle" => match settle_and_journal(
                &store_path,
                payer_wallet,
                &payout_path,
                &source_url,
                START_TIME,
            )
            .await
            {
                Ok(event) => emit(event)?,
                Err(_) => emit(json!({
                    "event": "settlement_failed",
                    "attempt_scope": "this_route_attempt",
                    "will_retry": true,
                }))?,
            },
            "resume" => emit(
                settle_and_journal(
                    &store_path,
                    payer_wallet,
                    &payout_path,
                    &source_url,
                    START_TIME + 61,
                )
                .await?,
            )?,
            "ack" => emit(
                complete_after_receiver_ack(
                    &store_path,
                    payer_wallet,
                    &source_url,
                    START_TIME + 61,
                )
                .await?,
            )?,
            "stop" => {
                emit(json!({"event": "stopped", "role": "payer"}))?;
                return Ok(());
            }
            command => bail!("unknown payer command {command}"),
        }
    }
    bail!("payer stdin closed without stop")
}

fn initialize_credit_account(
    store_path: &Path,
    source_url: &str,
    destination_url: &str,
) -> Result<()> {
    let mut store = CreditAccountStore::open(store_path)?;
    if store.load(ACCOUNT_ID)?.is_some() {
        return Ok(());
    }
    let mut account = CreditAccount::new(AccountPolicy {
        counterparty: ACCOUNT_ID.to_string(),
        max_total_peer_credit_sat: 0,
        issuers: vec![IssuerPolicy {
            issuer: source_url.to_string(),
            max_peer_credit_sat: 0,
            max_offline_peer_credit_sat: 0,
            max_closed_loop_sat: 0,
            max_withdrawable_sat: FUNDING_SAT,
            expires_at_unix: Some(START_TIME + 60),
        }],
    })?;
    account.record_backing_deposit(
        &BackingDeposit {
            deposit_id: "source-funding".to_string(),
            issuer: source_url.to_string(),
            amount_sat: FUNDING_SAT,
            value_class: ValueClass::ReserveBackedWithdrawable,
        },
        source_url,
    )?;
    account.apply_receipt(
        &ServiceReceiptClaim {
            receipt_id: "verified-fixture-service".to_string(),
            issuer: source_url.to_string(),
            counterparty: ACCOUNT_ID.to_string(),
            service: "verified_fixture_service".to_string(),
            resource: "iris-stack:cashu-process-gate".to_string(),
            useful_service_units: 32_768,
            amount_sat: SERVICE_SAT + MAX_FEE_SAT,
            value_class: ValueClass::ReserveBackedWithdrawable,
            issued_at_unix: START_TIME,
            expires_at_unix: START_TIME + 60,
        },
        source_url,
        AcceptanceMode::Online,
        START_TIME,
    )?;
    account.authorize_external_settlement(
        &ExternalSettlementRequest {
            settlement_id: SETTLEMENT_ID.to_string(),
            issuer: source_url.to_string(),
            counterparty: ACCOUNT_ID.to_string(),
            payout_destination: destination_url.to_string(),
            amount_sat: SERVICE_SAT,
            max_fee_sat: MAX_FEE_SAT,
            expires_at_unix: START_TIME + 60,
        },
        ACCOUNT_ID,
        START_TIME,
    )?;
    store.create(ACCOUNT_ID, &account)?;
    Ok(())
}

async fn settle_and_journal(
    store_path: &Path,
    payer_wallet: &Path,
    payout_path: &Path,
    source_url: &str,
    now_unix: u64,
) -> Result<Value> {
    let account = CreditAccountStore::open(store_path)?
        .load(ACCOUNT_ID)?
        .context("credit account disappeared")?;
    let authorizations = account.pending_external_settlement_authorizations();
    ensure!(authorizations.len() == 1, "expected one pending settlement");
    let authorization = &authorizations[0];
    let route = CashuIssuerRoute {
        issuer: source_url.to_string(),
        source_mint_url: source_url.to_string(),
    };
    let transfer = execute_cashu_settlement(payer_wallet, authorization, &route, now_unix).await?;
    let (payment, journal_reused) = if payout_path.is_file() {
        (
            serde_json::from_reader(File::open(payout_path)?)
                .context("decode durable payout journal")?,
            true,
        )
    } else {
        let payment = send_payment_token(
            payer_wallet,
            &transfer.destination_mint_url,
            transfer.amount_sat,
        )
        .await?;
        write_json_atomic(payout_path, &payment)?;
        (payment, false)
    };
    validate_payment(&payment, &transfer)?;
    Ok(json!({
        "event": "payout_ready",
        "transfer_id": transfer.transfer_id,
        "source_melt_quote_id": transfer.source_melt_quote_id,
        "destination_mint_quote_id": transfer.destination_mint_quote_id,
        "fee_paid_sat": transfer.fee_paid_sat,
        "payout_operation_id": payment.operation_id,
        "journal_reused": journal_reused,
    }))
}

fn validate_payment(payment: &CashuSentPayment, transfer: &CashuCrossMintTransfer) -> Result<()> {
    ensure!(payment.mint_url == transfer.destination_mint_url);
    ensure!(payment.amount_sat == transfer.amount_sat);
    ensure!(payment.send_fee_sat == 0);
    ensure!(payment.unit == "sat");
    Ok(())
}

async fn complete_after_receiver_ack(
    store_path: &Path,
    payer_wallet: &Path,
    source_url: &str,
    now_unix: u64,
) -> Result<Value> {
    let mut store = CreditAccountStore::open(store_path)?;
    let mut account = store
        .load(ACCOUNT_ID)?
        .context("credit account disappeared")?;
    let authorizations = account.pending_external_settlement_authorizations();
    ensure!(authorizations.len() == 1, "expected one pending settlement");
    let route = CashuIssuerRoute {
        issuer: source_url.to_string(),
        source_mint_url: source_url.to_string(),
    };
    let transfer =
        execute_cashu_settlement(payer_wallet, &authorizations[0], &route, now_unix).await?;
    let revision = account.revision();
    account.complete_external_settlement(SETTLEMENT_ID, transfer.fee_paid_sat)?;
    store.save(ACCOUNT_ID, revision, &account)?;
    let reserve = account
        .sat_reserve(source_url)
        .context("source reserve disappeared")?;
    Ok(json!({
        "event": "settlement_complete",
        "pending_settlements": account.pending_external_settlement_authorizations().len(),
        "total_deposited_sat": reserve.total_deposited_sat(),
        "available_sat": reserve.available_sat(),
        "pending_external_sat": reserve.pending_external_sat(),
        "settled_external_sat": reserve.settled_external_sat(),
        "conserved_sat": reserve.conserved_sat()?,
    }))
}

async fn run_receiver(
    receiver_wallet: &Path,
    payout_journal: &Path,
    expectation: &str,
) -> Result<()> {
    let payment: CashuSentPayment = serde_json::from_reader(File::open(payout_journal)?)
        .context("decode payer payout journal")?;
    match expectation {
        "accept" => {
            let received = receive_payment_token(receiver_wallet, &payment.token).await?;
            ensure!(received.amount_sat == SERVICE_SAT);
            let balance = load_mint_balance(receiver_wallet, &payment.mint_url).await?;
            ensure!(balance.balance_sat == SERVICE_SAT);
            emit(json!({
                "event": "payment_accepted",
                "amount_sat": received.amount_sat,
                "balance_sat": balance.balance_sat,
            }))
        }
        "reject" => {
            let error = receive_payment_token(receiver_wallet, &payment.token)
                .await
                .expect_err("a second receiver accepted spent Cashu proofs");
            ensure!(
                format!("{error:#}").contains("Failed to receive Cashu payment token"),
                "unexpected replay error: {error:#}"
            );
            let balance = load_mint_balance(receiver_wallet, &payment.mint_url).await?;
            ensure!(balance.balance_sat == 0);
            emit(json!({
                "event": "replay_rejected",
                "balance_sat": balance.balance_sat,
                "reason": "mint rejected already-spent proofs",
            }))
        }
        _ => bail!("receiver expectation must be accept or reject"),
    }
}

fn write_json_atomic(path: &Path, value: &CashuSentPayment) -> Result<()> {
    let parent = path.parent().context("payout journal has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temporary);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn emit(value: Value) -> Result<()> {
    println!("{value}");
    io::stdout().flush().context("flush fixture event")
}

fn text_arg(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing {name}"))?
        .into_string()
        .map_err(|_| anyhow::anyhow!("{name} is not UTF-8"))
}

fn path_arg(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .with_context(|| format!("missing {name}"))
}

fn ensure_no_args(mut args: impl Iterator<Item = OsString>) -> Result<()> {
    ensure!(args.next().is_none(), "unexpected extra fixture argument");
    Ok(())
}
