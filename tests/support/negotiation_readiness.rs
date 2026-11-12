//! Checks only the controlled Drive-to-Chat startup proof, never a quiet interval.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) async fn bounded<T>(
    operation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    within(std::time::Duration::from_secs(120), operation).await
}

async fn within<T>(
    duration: std::time::Duration,
    operation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(duration, operation)
        .await
        .context("initial negotiation exceeded its absolute readiness deadline")?
}

pub(super) fn ready(
    snapshot: &Value,
    target: &str,
    transit: &str,
    window_end_ms: u64,
) -> Result<bool> {
    ensure!(
        serde_json::to_vec(snapshot)?.len() <= 8192,
        "negotiation snapshot exceeds cap"
    );
    ensure!(snapshot["version"] == 1, "missing negotiation schema");
    let observed = snapshot["observed_unix_ms"]
        .as_u64()
        .context("missing negotiation clock")?;
    ensure!(
        window_end_ms > observed,
        "readiness window must be in the future"
    );
    let proof = &snapshot["observations"];
    ensure!(
        proof["version"] == 2 && proof["expected_peer"] == target,
        "negotiation target/schema mismatch"
    );
    ensure!(
        proof["invalid"].as_bool() == Some(false),
        "ambiguous or malformed negotiation evidence"
    );
    let attempts = proof["attempts"]
        .as_array()
        .context("missing initiated attempts")?;
    ensure!(attempts.len() <= 32, "negotiation evidence exceeds bound");
    let mut complete = !attempts.is_empty();
    for (i, attempt) in attempts.iter().enumerate() {
        ensure!(
            attempt["ordinal"].as_u64() == Some(i as u64 + 1),
            "missing attempt ordinal"
        );
        let answered = attempt["answer_received"]
            .as_bool()
            .context("missing answer state")?;
        let terminal = attempt["terminal"]
            .as_str()
            .context("missing terminal outcome")?;
        ensure!(
            matches!(terminal, "pending" | "authenticated-unsupported"),
            "unknown terminal outcome"
        );
        if let Some(token) = attempt["session_prefix"].as_str() {
            ensure!(
                token.len() == 8 && token.bytes().all(|b| b.is_ascii_digit()),
                "malformed session display prefix"
            );
        } else {
            ensure!(
                attempt.get("session_prefix") == Some(&Value::Null)
                    && !answered
                    && terminal == "pending",
                "missing session correlation"
            );
        }
        if terminal == "authenticated-unsupported" {
            ensure!(answered, "terminal without answer");
            ensure!(
                attempt["terminal_failure_count"].as_u64() == Some(i as u64 + 1),
                "missing or out-of-order full-peer terminal counter"
            );
        } else {
            ensure!(i + 1 == attempts.len(), "overlapping serial attempts");
            ensure!(
                attempt.get("terminal_failure_count") == Some(&Value::Null),
                "pending attempt has terminal counter"
            );
        }
        complete &= terminal == "authenticated-unsupported";
    }
    let peers = snapshot["peers"]
        .as_array()
        .context("missing endpoint peers")?;
    ensure!(
        peers.len() <= 2,
        "unexpected endpoint peer in controlled fixture"
    );
    let mut seen = BTreeSet::new();
    let mut transit_ready = false;
    let mut target_ready = false;
    for peer in peers {
        let id = peer["npub"].as_str().context("missing peer identity")?;
        ensure!(
            (id == target || id == transit) && seen.insert(id),
            "unknown or duplicate endpoint peer"
        );
        let boolean = |key: &str| {
            peer[key]
                .as_bool()
                .with_context(|| format!("missing {key}"))
        };
        let connected = boolean("connected")?;
        let rekey = boolean("rekey_in_progress")? || boolean("rekey_draining")?;
        let pending = boolean("direct_probe_pending")?;
        let cooldown = boolean("nostr_traversal_in_cooldown")?;
        let failures = peer["nostr_traversal_consecutive_failures"]
            .as_u64()
            .context("missing failure state")?;
        peer["direct_probe_retry_count"]
            .as_u64()
            .context("missing retry state")?;
        let optional = |key: &str| -> Result<Option<u64>> {
            let value = peer.get(key).with_context(|| format!("missing {key}"))?;
            if value.is_null() {
                Ok(None)
            } else {
                Ok(Some(
                    value.as_u64().with_context(|| format!("invalid {key}"))?,
                ))
            }
        };
        let after = optional("direct_probe_after_ms")?;
        let until = optional("nostr_traversal_cooldown_until_ms")?;
        ensure!(cooldown == until.is_some(), "inconsistent cooldown state");
        if id == transit {
            transit_ready = connected && !rekey && !pending;
        } else {
            // The fixture deliberately disables Chat traversal; direct connection
            // to Chat would bypass transit and is not a valid alternate outcome.
            ensure!(!connected, "unexpected direct Chat link");
            target_ready = complete
                && !rekey
                && pending
                && cooldown
                && failures == attempts.len() as u64
                && until.is_some_and(|until| {
                    until > window_end_ms && after.is_some_and(|after| after >= until)
                });
        }
    }
    Ok(complete && target_ready && transit_ready)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn deadline_cancels_unfinished_observation_and_releases_its_resources() {
        struct Guard(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let owned = dropped.clone();
        let result: Result<()> = within(std::time::Duration::from_millis(1), async move {
            let _guard = Guard(owned);
            std::future::pending().await
        })
        .await;
        assert!(result.is_err());
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }
    fn snapshot() -> Value {
        let peer = |id, connected, pending, count, after: Option<u64>| {
            json!({"npub":id,
            "connected":connected,"rekey_in_progress":false,"rekey_draining":false,
            "direct_probe_pending":pending,"direct_probe_retry_count":count,
            "direct_probe_after_ms":after,"nostr_traversal_consecutive_failures":count,
            "nostr_traversal_in_cooldown":after.is_some(),"nostr_traversal_cooldown_until_ms":after})
        };
        json!({"version":1,"observed_unix_ms":1000,"observations":{"version":2,"expected_peer":"chat","invalid":false,
            "attempts":[{"ordinal":1,"session_prefix":"17945015","answer_received":true,"terminal":"authenticated-unsupported","terminal_failure_count":1}]},
            "peers":[peer("transit",true,false,0,None),peer("chat",false,true,1,Some(200000))]})
    }
    #[test]
    fn positive_terminal_proof_and_deferred_retry_are_both_required() {
        let s = snapshot();
        assert!(ready(&s, "chat", "transit", 66000).unwrap());
        for field in ["answer_received", "terminal"] {
            let mut changed = s.clone();
            changed["observations"]["attempts"][0][field] = Value::Null;
            assert!(ready(&changed, "chat", "transit", 66000).is_err());
        }
        let mut empty = s;
        empty["observations"]["attempts"] = json!([]);
        assert!(!ready(&empty, "chat", "transit", 66000).unwrap());
    }
    #[test]
    fn active_rekey_missing_peer_or_renewal_before_window_end_is_not_ready() {
        for change in 0..5 {
            let mut s = snapshot();
            match change {
                0 => s["peers"][0]["rekey_in_progress"] = json!(true),
                1 => s["peers"][1]["direct_probe_after_ms"] = json!(2000),
                2 => {
                    s["peers"][1]["nostr_traversal_cooldown_until_ms"] = json!(66000);
                    s["peers"][1]["direct_probe_after_ms"] = json!(66000);
                }
                3 => {
                    s["peers"].as_array_mut().unwrap().remove(0);
                }
                _ => {
                    s["observations"]["attempts"][0]["terminal"] = json!("pending");
                    s["observations"]["attempts"][0]["terminal_failure_count"] = Value::Null;
                }
            }
            assert!(
                !ready(&s, "chat", "transit", 66000).unwrap(),
                "case {change}"
            );
        }
    }
    #[test]
    fn unknown_malformed_or_ambiguous_evidence_fails_closed() {
        for change in 0..6 {
            let mut s = snapshot();
            match change {
                0 => s["observations"]["invalid"] = json!(true),
                1 => s["peers"][1]["npub"] = json!("unknown"),
                2 => s["observations"]["attempts"][0]["terminal"] = json!("unknown"),
                3 => s["peers"][0]["connected"] = Value::Null,
                4 => s["peers"][1]["nostr_traversal_in_cooldown"] = json!(false),
                _ => s["observations"]["expected_peer"] = json!("unknown"),
            }
            assert!(
                ready(&s, "chat", "transit", 66000).is_err(),
                "case {change}"
            );
        }
    }
    #[test]
    fn later_attempt_requires_new_terminal_proof() {
        let mut s = snapshot();
        assert!(ready(&s, "chat", "transit", 66000).unwrap());
        s["observations"]["attempts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"ordinal":2,
            "session_prefix":null,"answer_received":false,"terminal":"pending","terminal_failure_count":null}));
        assert!(!ready(&s, "chat", "transit", 66000).unwrap());
    }
    #[test]
    fn repeated_display_prefix_requires_ordered_full_peer_terminal_counters() {
        let mut s = snapshot();
        let mut second = s["observations"]["attempts"][0].clone();
        second["ordinal"] = json!(2);
        second["terminal_failure_count"] = json!(2);
        s["observations"]["attempts"]
            .as_array_mut()
            .unwrap()
            .push(second);
        s["peers"][1]["nostr_traversal_consecutive_failures"] = json!(2);
        assert!(ready(&s, "chat", "transit", 66000).unwrap());
        for bad in [Value::Null, json!(0), json!(1), json!(3), json!("2")] {
            let mut changed = s.clone();
            changed["observations"]["attempts"][1]["terminal_failure_count"] = bad;
            assert!(ready(&changed, "chat", "transit", 66000).is_err());
        }
        let mut lagging_endpoint = s.clone();
        lagging_endpoint["peers"][1]["nostr_traversal_consecutive_failures"] = json!(1);
        assert!(!ready(&lagging_endpoint, "chat", "transit", 66000).unwrap());
        s["observations"]["attempts"][0]["terminal"] = json!("pending");
        s["observations"]["attempts"][0]["terminal_failure_count"] = Value::Null;
        assert!(ready(&s, "chat", "transit", 66000).is_err());
    }
}
