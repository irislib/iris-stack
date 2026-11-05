//! Pure checks for fixed fixture counters; no payloads, identities or dynamic event text.
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
        object.len() == 3 && snapshot["version"] == 2,
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
    validate_control(&snapshot["control"])?;
    Ok(snapshot.clone())
}

fn validate_control(control: &Value) -> Result<()> {
    let object = control
        .as_object()
        .context("fixture control counters missing")?;
    let expected = [
        "version",
        "initialized",
        "service_enabled_before_start",
        "invalid_events",
        "overflow",
        "submissions",
        "retries",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    ensure!(
        object.keys().map(String::as_str).collect::<BTreeSet<_>>() == expected,
        "fixture control field set changed"
    );
    for key in ["version", "initialized", "service_enabled_before_start"] {
        ensure!(
            control[key].as_u64() == Some(1),
            "fixture control initialization/schema missing"
        );
    }
    for key in ["invalid_events", "overflow"] {
        ensure!(
            control[key].as_u64() == Some(0),
            "fixture control evidence is incomplete"
        );
    }
    let rows = control["submissions"]
        .as_array()
        .context("fixture control submissions missing")?;
    ensure!(rows.len() == 8, "fixture control row set changed");
    for (index, row) in rows.iter().enumerate() {
        let fields = row
            .as_array()
            .context("fixture control row is not an array")?;
        ensure!(
            fields.len() == 4
                && fields.iter().all(|field| field.as_u64().is_some())
                && fields[0].as_u64() == Some(16 + index as u64),
            "fixture control row schema changed"
        );
        let attempts = fields[1].as_u64().unwrap();
        let submitted = fields[2].as_u64().unwrap();
        let bytes = fields[3].as_u64().unwrap();
        ensure!(
            submitted <= attempts && (submitted != 0 || bytes == 0),
            "fixture control submission totals are inconsistent"
        );
    }
    let retries = control["retries"]
        .as_array()
        .context("fixture retry counters missing")?;
    ensure!(
        retries.len() == 11 && retries.iter().all(|value| value.as_u64().is_some()),
        "fixture retry counter schema changed"
    );
    Ok(())
}

fn control_delta(before: &Value, after: &Value) -> Result<Value> {
    validate_control(before)?;
    validate_control(after)?;
    let mut rows = Vec::with_capacity(8);
    for index in 0..8 {
        let mut row = vec![16 + index as u64];
        for field in 1..4 {
            row.push(
                after["submissions"][index][field]
                    .as_u64()
                    .unwrap()
                    .checked_sub(before["submissions"][index][field].as_u64().unwrap())
                    .context("fixture control counter reset")?,
            );
        }
        rows.push(row);
    }
    let retries = (0..11)
        .map(|index| {
            after["retries"][index]
                .as_u64()
                .unwrap()
                .checked_sub(before["retries"][index].as_u64().unwrap())
                .context("fixture retry counter reset")
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({"submissions": rows, "retries": retries}))
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
            "delta": delta(&before[index]["snapshot"], &after[index]["snapshot"])?,
            "control_delta": control_delta(&before[index]["snapshot"]["control"], &after[index]["snapshot"]["control"])?}),
        );
    }
    output.insert("scope".into(), json!("Local cumulative counters on the same fixture-held endpoints. Snapshots are non-atomic and RPC timestamps bound their skew from transit/CPU samples. Service counters were enabled before application startup; they exclude shared control and transit. Control rows are process-wide [type, attempts, submitted, submitted body bytes], never complete wire-byte accounting. Retry buckets follow the pinned fixture schema; no event text or identities are retained."));
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
        json!({"version": 2, "counters": expected_keys().into_iter().map(|key| (key, value)).collect::<BTreeMap<_, _>>(),
            "control": {"version": 1, "initialized": 1, "service_enabled_before_start": 1,
                "invalid_events": 0, "overflow": 0,
                "submissions": (16..24).map(|kind| [kind, value, value, value]).collect::<Vec<_>>(),
                "retries": vec![value; 11]}})
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
    #[test]
    fn control_schema_and_resets_fail_closed_without_changing_legacy_counters() {
        let before = snapshot(3);
        let after = snapshot(5);
        let difference = control_delta(&before["control"], &after["control"]).unwrap();
        assert_eq!(difference["submissions"][0], json!([16, 2, 2, 2]));
        assert_eq!(difference["retries"], json!(vec![2; 11]));
        for key in ["initialized", "service_enabled_before_start"] {
            let mut changed = after.clone();
            changed["control"][key] = json!(0);
            assert!(validate(&changed).is_err());
        }
        for key in ["invalid_events", "overflow"] {
            let mut changed = after.clone();
            changed["control"][key] = json!(1);
            assert!(validate(&changed).is_err());
        }
        let mut changed = after.clone();
        changed["control"]["submissions"][0][0] = json!(31);
        assert!(validate(&changed).is_err());
        let mut changed = after.clone();
        changed["control"]["error_text"] = json!("must reject an added string field");
        assert!(validate(&changed).is_err());
        let mut changed = after.clone();
        changed["control"]["submissions"][0][1] = json!(2);
        changed["control"]["submissions"][0][2] = json!(2);
        assert!(control_delta(&before["control"], &changed["control"]).is_err());
        let mut changed = after.clone();
        changed["control"]["retries"][0] = json!(2);
        assert!(control_delta(&before["control"], &changed["control"]).is_err());
        let mut old = after;
        old["version"] = json!(1);
        assert!(validate(&old).is_err());
    }
}
