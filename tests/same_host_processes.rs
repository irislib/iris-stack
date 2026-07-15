use std::collections::HashSet;
use std::net::UdpSocket;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(30);
static PROCESS_MATRIX_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Copy)]
enum ProcessExit {
    Graceful,
    Forced,
}

struct Process {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
    output: String,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_blob_store_survives_graceful_and_forced_anchor_replacement() {
    let _guard = PROCESS_MATRIX_LOCK.lock().await;
    for anchor_exit in [ProcessExit::Graceful, ProcessExit::Forced] {
        exercise_anchor_replacement(anchor_exit).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_blob_store_recovers_from_graceful_and_forced_provider_churn() {
    let _guard = PROCESS_MATRIX_LOCK.lock().await;
    for provider_exit in [ProcessExit::Graceful, ProcessExit::Forced] {
        exercise_provider_churn(provider_exit).await;
    }
}

async fn exercise_anchor_replacement(anchor_exit: ProcessExit) {
    let rendezvous_addr = reserve_udp_address();
    let external_addr = reserve_udp_address();

    let mut external = spawn(&["external", &external_addr]).await;
    let external_npub = external.identity("EXTERNAL_READY ").await;

    let mut anchor = spawn_product(
        "anchor",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &[],
    )
    .await;
    let anchor_npub = anchor.identity("ANCHOR_READY ").await;

    let mut provider = spawn_product(
        "provider",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &[],
    )
    .await;
    let provider_npub = provider.identity("PROVIDER_READY ").await;

    let mut consumer = spawn_product(
        "consumer",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &[&provider_npub],
    )
    .await;
    consumer.line_containing("CONSUMER_READY ").await;

    anchor
        .command_until("probe-outbound", "ANCHOR_OUTBOUND ")
        .await;
    let anchor_output = terminate(anchor, anchor_exit, "ANCHOR_DONE").await;

    provider
        .command_until("probe-outbound", "PROVIDER_OUTBOUND_AFTER ")
        .await;
    consumer
        .command_until("after-anchor-exit", "CONSUMER_AFTER_FAILOVER ")
        .await;

    consumer.stop().await;
    let consumer_output = consumer.finish().await;
    provider.stop().await;
    let provider_output = provider.finish().await;
    external.stop().await;
    let external_output = external.finish().await;

    assert_markers(
        &provider_output,
        &[&format!(
            "LOCAL_AUTH role=provider configured=false peer={anchor_npub}"
        )],
    );
    assert_markers(
        &consumer_output,
        &[
            &format!("LOCAL_AUTH role=consumer configured=false peer={anchor_npub}"),
            "CAPABILITY_AUTHENTICATED hashtree.blob/1",
            "BLOB_FETCH phase=before verified=true cached=true",
            "BLOB_FETCH phase=after verified=true cached=true",
        ],
    );

    let consumer_npub = field_containing(&consumer_output, "CONSUMER_READY ", 1);
    assert_direct_probes(
        &external_output,
        &[
            ("anchor", "before", &anchor_npub),
            ("provider", "before", &provider_npub),
            ("provider", "after", &provider_npub),
            ("consumer", "before", consumer_npub),
            ("consumer", "after", consumer_npub),
        ],
    );
    assert_markers(&external_output, &["EXTERNAL_DONE"]);
    assert_markers(&anchor_output, &["ANCHOR_OUTBOUND "]);
}

async fn exercise_provider_churn(provider_exit: ProcessExit) {
    let rendezvous_addr = reserve_udp_address();
    let external_addr = reserve_udp_address();

    let mut external = spawn(&["external", &external_addr]).await;
    let external_npub = external.identity("EXTERNAL_READY ").await;

    let mut anchor = spawn_product(
        "anchor",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &[],
    )
    .await;
    let anchor_npub = anchor.identity("ANCHOR_READY ").await;
    anchor
        .command_until("probe-outbound", "ANCHOR_OUTBOUND ")
        .await;

    let mut consumer = spawn_product(
        "consumer",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &[],
    )
    .await;
    consumer
        .line_containing("BLOB_MISS phase=no-provider truthful=true cached=false")
        .await;
    let consumer_npub = consumer.identity("CONSUMER_READY ").await;

    let mut provider = spawn_product(
        "provider",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &["first"],
    )
    .await;
    let provider_npub = provider.identity("PROVIDER_READY ").await;
    consumer
        .command_until(
            &format!("fetch first {provider_npub}"),
            "CONSUMER_PROVIDER_ACTIVE ",
        )
        .await;

    terminate(provider, provider_exit, "PROVIDER_DONE").await;
    consumer
        .command_until(
            &format!("provider-gone {provider_npub}"),
            "CONSUMER_PROVIDER_GONE ",
        )
        .await;

    let mut replacement = spawn_product(
        "provider",
        &rendezvous_addr,
        &external_npub,
        &external_addr,
        &["replacement"],
    )
    .await;
    let replacement_npub = replacement.identity("PROVIDER_READY ").await;
    assert_ne!(provider_npub, replacement_npub);
    consumer
        .command_until(
            &format!("fetch replacement {replacement_npub}"),
            "CONSUMER_PROVIDER_ACTIVE ",
        )
        .await;

    anchor
        .command_until("probe-outbound-after", "ANCHOR_OUTBOUND_AFTER ")
        .await;

    consumer.stop().await;
    let consumer_output = consumer.finish().await;
    replacement.stop().await;
    let replacement_output = replacement.finish().await;
    anchor.stop().await;
    let anchor_output = anchor.finish().await;
    external.stop().await;
    let external_output = external.finish().await;

    assert_markers(
        &consumer_output,
        &[
            "BLOB_FETCH phase=first verified=true cached=true",
            "BLOB_CACHE phase=provider-gone verified=true",
            "BLOB_MISS phase=provider-gone truthful=true cached=false",
            "BLOB_FETCH phase=replacement verified=true cached=true",
            "CONSUMER_DONE",
        ],
    );
    assert_markers(&replacement_output, &["PROVIDER_DONE"]);
    assert_markers(&anchor_output, &["ANCHOR_DONE"]);
    assert_markers(&external_output, &["EXTERNAL_DONE"]);

    assert_direct_probes(
        &external_output,
        &[
            ("anchor", "before", &anchor_npub),
            ("anchor", "after", &anchor_npub),
            ("consumer", "no-provider", &consumer_npub),
            ("consumer", "active", &consumer_npub),
            ("consumer", "gone", &consumer_npub),
            ("consumer", "replacement", &consumer_npub),
            ("provider", "active", &provider_npub),
            ("provider", "replacement", &replacement_npub),
        ],
    );
}

async fn terminate(mut process: Process, exit: ProcessExit, done_marker: &str) -> String {
    match exit {
        ProcessExit::Graceful => {
            process.stop().await;
            let output = process.finish().await;
            assert!(output.contains(done_marker), "{output}");
            output
        }
        ProcessExit::Forced => {
            let output = process.kill().await;
            assert!(!output.contains(done_marker), "{output}");
            output
        }
    }
}

impl Process {
    async fn identity(&mut self, marker: &str) -> String {
        let line = self.line_containing(marker).await;
        field(&line, 1).to_string()
    }

    async fn command_until(&mut self, command: &str, marker: &str) {
        self.command(command).await;
        self.line_containing(marker).await;
    }

    async fn line_containing(&mut self, marker: &str) -> String {
        timeout(WAIT, async {
            loop {
                let line = self
                    .lines
                    .next_line()
                    .await
                    .expect("read child stdout")
                    .expect("child exited before marker");
                self.output.push_str(&line);
                self.output.push('\n');
                if line.contains(marker) {
                    return line;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {marker}; output:\n{}", self.output))
    }

    async fn command(&mut self, command: &str) {
        let stdin = self.child.stdin.as_mut().expect("child stdin");
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .expect("write command");
        stdin.flush().await.expect("flush command");
    }

    async fn stop(&mut self) {
        self.command("stop").await;
    }

    async fn finish(self) -> String {
        let (output, status) = self.collect().await;
        assert!(status.success(), "child {status}; output:\n{output}");
        output
    }

    async fn kill(mut self) -> String {
        self.child.kill().await.expect("kill child");
        let (output, status) = self.collect().await;
        assert!(!status.success(), "killed child unexpectedly succeeded");
        output
    }

    async fn collect(self) -> (String, std::process::ExitStatus) {
        let mut output = self.output;
        let mut reader = self.lines.into_inner();
        let mut child = self.child;
        let (rest, status) = timeout(WAIT, async move {
            let mut rest = String::new();
            reader
                .read_to_string(&mut rest)
                .await
                .expect("read remaining child stdout");
            let status = child.wait().await.expect("wait for child");
            (rest, status)
        })
        .await
        .expect("child exit timed out");
        output.push_str(&rest);
        (output, status)
    }
}

async fn spawn(args: &[&str]) -> Process {
    let mut child = Command::new(env!("CARGO_BIN_EXE_iris-stack-lab"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn role process");
    let stdout = child.stdout.take().expect("child stdout");
    Process {
        child,
        lines: BufReader::new(stdout).lines(),
        output: String::new(),
    }
}

async fn spawn_product(
    role: &str,
    rendezvous_addr: &str,
    external_npub: &str,
    external_addr: &str,
    extra: &[&str],
) -> Process {
    let mut args = vec![role, rendezvous_addr, external_npub, external_addr];
    args.extend_from_slice(extra);
    spawn(&args).await
}

fn reserve_udp_address() -> String {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("reserve loopback UDP port");
    socket
        .local_addr()
        .expect("reserved UDP address")
        .to_string()
}

fn field(line: &str, index: usize) -> &str {
    line.split_whitespace().nth(index).expect("output field")
}

fn field_containing<'a>(output: &'a str, marker: &str, index: usize) -> &'a str {
    field(
        output
            .lines()
            .find(|line| line.contains(marker))
            .unwrap_or_else(|| panic!("missing {marker} in:\n{output}")),
        index,
    )
}

fn parse_external_request(line: &str) -> Option<(&str, &str, &str)> {
    if !line.starts_with("EXTERNAL_REQUEST ") {
        return None;
    }
    let mut role = None;
    let mut phase = None;
    let mut source = None;
    for field in line.split_whitespace().skip(1) {
        let (name, value) = field.split_once('=')?;
        match name {
            "role" => role = Some(value),
            "phase" => phase = Some(value),
            "source" => source = Some(value),
            _ => {}
        }
    }
    Some((role?, phase?, source?))
}

fn assert_direct_probes(output: &str, expected: &[(&str, &str, &str)]) {
    let observed = output
        .lines()
        .filter_map(parse_external_request)
        .collect::<HashSet<_>>();
    for &(role, phase, identity) in expected {
        assert!(
            observed.contains(&(role, phase, identity)),
            "missing direct UDP probe {role}/{phase}/{identity}; output:\n{output}"
        );
    }
}

fn assert_markers(output: &str, markers: &[&str]) {
    for marker in markers {
        assert!(
            output.contains(marker),
            "missing {marker} in output:\n{output}"
        );
    }
}
