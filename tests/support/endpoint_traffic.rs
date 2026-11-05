//! Pure checks for fixed fixture counters, never packet/payload classification.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const SNAPSHOT_CAP: usize = 8192;
const EVIDENCE_CAP: usize = 32768;
const PUBSUB: [&str; 15] = [
    "req_frames_received",
    "close_frames_received",
    "event_frames_received",
    "inv_frames_received",
    "want_frames_received",
    "want_frames_sent",
    "subscription_events_received",
    "expired_wants",
    "provider_cooldowns",
    "tcp_receive_batches",
    "tcp_datagrams_received",
    "tcp_datagrams_rejected",
    "tcp_poll_turns",
    "transport_errors",
    "reputation_errors",
];
const TRANSPORTS: [&str; 9] = [
    "udp",
    "ethernet",
    "tcp",
    "tor",
    "websocket",
    "webrtc",
    "ble",
    "sim",
    "other",
];

fn expected_keys() -> BTreeSet<String> {
    let mut keys = PUBSUB
        .into_iter()
        .map(|key| format!("pubsub.{key}"))
        .collect::<BTreeSet<_>>();
    for port in [7368, 39019] {
        for counter in ["ambiguous_port_datagrams", "discarded_outputs"] {
            keys.insert(format!("service.{port}.{counter}"));
        }
        for transport in TRANSPORTS {
            for counter in [
                "submitted_packets",
                "fips_payload_bytes",
                "ethernet_framing_bytes",
            ] {
                keys.insert(format!("service.{port}.{transport}.{counter}"));
            }
        }
    }
    keys
}

pub(super) fn validate(snapshot: &Value) -> Result<Value> {
    ensure!(
        serde_json::to_vec(snapshot)?.len() <= SNAPSHOT_CAP,
        "endpoint traffic snapshot exceeds cap"
    );
    let object = snapshot
        .as_object()
        .context("endpoint traffic snapshot missing")?;
    ensure!(
        object.len() == 2 && snapshot["version"] == 1,
        "unsupported endpoint traffic schema"
    );
    let counters = snapshot["counters"]
        .as_object()
        .context("endpoint traffic counters missing")?;
    ensure!(
        counters.keys().cloned().collect::<BTreeSet<_>>() == expected_keys(),
        "endpoint traffic field set changed"
    );
    ensure!(
        counters.values().all(|value| value.as_u64().is_some()),
        "endpoint traffic counter is not u64"
    );
    Ok(snapshot.clone())
}

fn delta(before: &Value, after: &Value) -> Result<Value> {
    validate(before)?;
    validate(after)?;
    let mut counters = BTreeMap::new();
    for key in expected_keys() {
        let first = before["counters"][&key]
            .as_u64()
            .context("missing first counter")?;
        let second = after["counters"][&key]
            .as_u64()
            .context("missing second counter")?;
        counters.insert(
            key.clone(),
            second
                .checked_sub(first)
                .with_context(|| format!("endpoint counter reset: {key}"))?,
        );
    }
    Ok(json!(counters))
}

pub(super) fn evidence(before: &[Value; 2], after: &[Value; 2]) -> Result<Value> {
    let mut output = serde_json::Map::new();
    for (index, role) in ["chat", "drive"].into_iter().enumerate() {
        output.insert(
            role.to_string(),
            json!({"before": before[index], "after": after[index],
            "delta": delta(&before[index]["snapshot"], &after[index]["snapshot"])?}),
        );
    }
    output.insert("scope".into(), json!("Local cumulative counters on the same fixture-held endpoints. Snapshots are non-atomic and RPC timestamps bound their skew from transit/CPU samples. Service bytes include submitted FIPS payloads and service retransmissions, but exclude shared handshake, MMP, rekey and opaque forwarding; message counters are not byte counts."));
    let value = Value::Object(output);
    ensure!(
        serde_json::to_vec(&value)?.len() <= EVIDENCE_CAP,
        "endpoint traffic evidence exceeds cap"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(value: u64) -> Value {
        json!({"version": 1, "counters": expected_keys().into_iter().map(|key| (key, value)).collect::<BTreeMap<_, _>>()})
    }
    #[test]
    fn fixed_counter_serialization_and_deltas_preserve_numeric_limits() {
        let before = snapshot(u64::MAX - 2);
        let after = snapshot(u64::MAX);
        assert_eq!(expected_keys().len(), 73);
        assert_eq!(
            validate(
                &serde_json::from_slice::<Value>(&serde_json::to_vec(&after).unwrap()).unwrap()
            )
            .unwrap(),
            after
        );
        let difference = delta(&before, &after).unwrap();
        assert!(
            difference
                .as_object()
                .unwrap()
                .values()
                .all(|value| value == 2)
        );
        let boundary =
            |s: Value| json!({"started_unix_ms": 1, "finished_unix_ms": 2, "snapshot": s});
        assert!(
            evidence(
                &[boundary(before.clone()), boundary(before)],
                &[boundary(after.clone()), boundary(after)]
            )
            .is_ok()
        );
    }
    #[test]
    fn changed_fields_counter_resets_and_oversize_fail_closed() {
        let first = snapshot(3);
        let mut reset = snapshot(4);
        reset["counters"]["service.39019.udp.submitted_packets"] = json!(2);
        assert!(
            delta(&first, &reset)
                .unwrap_err()
                .to_string()
                .contains("counter reset")
        );
        let mut changed = snapshot(4);
        changed["counters"]["peer_identity"] = json!(0);
        assert!(validate(&changed).is_err());
        let mut missing = snapshot(4);
        missing["counters"]
            .as_object_mut()
            .unwrap()
            .remove("pubsub.event_frames_received");
        assert!(validate(&missing).is_err());
        let mut oversized = snapshot(4);
        oversized["counters"]["pubsub.event_frames_received"] = json!("x".repeat(SNAPSHOT_CAP));
        assert!(
            validate(&oversized)
                .unwrap_err()
                .to_string()
                .contains("cap")
        );
        assert!(delta(&first, &oversized).is_err());
    }
}
