use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(45);
#[allow(dead_code)] // Used by product tests, while each integration crate recompiles this module.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)] // Used by product tests, while each integration crate recompiles this module.
pub struct TestRoot(PathBuf);

#[allow(dead_code)] // Used by product tests, while each integration crate recompiles this module.
impl TestRoot {
    pub fn new() -> Result<Self> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "iris-stack-product-{}-{sequence}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).context("remove stale product-lab directory")?;
        }
        fs::create_dir_all(&path).context("create product-lab directory")?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug)]
pub struct CapturedProcess {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

// Bound stderr independently of line delimiters while always draining its pipe.
// A retained prefix is useful evidence, but never a complete-output certificate.
const STDERR_CAPTURE_LIMIT: usize = 1024 * 1024;
const STDERR_LINE_LIMIT: usize = 16 * 1024;
const STDERR_READ_SIZE: usize = 4096;

#[derive(Default)]
struct StderrCapture {
    bytes: Vec<u8>,
    total_bytes: u64,
    complete_line_end: usize,
    line_bytes: usize,
    overflow: bool,
    line_overflow: bool,
    read_error: Option<String>,
}

#[allow(dead_code)] // Used by the mesh phase observer, not all test crates.
pub struct StderrObservation {
    pub text: String,
    pub offset: usize,
    pub retained_bytes: usize,
    pub total_bytes: u64,
    pub pending_line_bytes: usize,
    pub complete: bool,
    pub overflow: bool,
    pub line_overflow: bool,
    pub read_error: Option<String>,
    pub cursor_valid: bool,
    pub utf8_valid: bool,
}

impl StderrCapture {
    fn append(&mut self, chunk: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len() as u64);
        if self.overflow || self.line_overflow { return; }
        for &byte in chunk {
            if self.bytes.len() == STDERR_CAPTURE_LIMIT {
                self.overflow = true;
                break;
            }
            if self.line_bytes == STDERR_LINE_LIMIT {
                self.line_overflow = true;
                break;
            }
            self.bytes.push(byte);
            self.line_bytes += 1;
            if byte == b'\n' {
                self.line_bytes = 0;
                self.complete_line_end = self.bytes.len();
            }
        }
    }

    fn ensure_complete(&self) -> Result<()> {
        ensure!(!self.overflow && !self.line_overflow && self.read_error.is_none(),
            "incomplete stderr capture: retained={} total={} byte_limit={} line_limit={} overflow={} line_overflow={} read_error={:?}",
            self.bytes.len(), self.total_bytes, STDERR_CAPTURE_LIMIT, STDERR_LINE_LIMIT,
            self.overflow, self.line_overflow, self.read_error);
        std::str::from_utf8(&self.bytes).context("stderr contained invalid or incomplete UTF-8")?;
        Ok(())
    }

    fn observation(&self, offset: usize) -> StderrObservation {
        let range = self.bytes.get(offset..self.complete_line_end);
        let cursor_valid = range.is_some();
        let range = range.unwrap_or_default();
        let utf8_valid = std::str::from_utf8(range).is_ok();
        StderrObservation {
            text: String::from_utf8_lossy(range).into_owned(), offset,
            retained_bytes: self.bytes.len(), total_bytes: self.total_bytes,
            pending_line_bytes: self.bytes.len() - self.complete_line_end,
            complete: cursor_valid && utf8_valid && !self.overflow &&
                !self.line_overflow && self.read_error.is_none(),
            overflow: self.overflow, line_overflow: self.line_overflow,
            read_error: self.read_error.clone(), cursor_valid, utf8_valid,
        }
    }

    fn snapshot(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes).into_owned();
        if self.overflow || self.line_overflow || self.read_error.is_some() {
            text.push_str(&format!("\n[INCOMPLETE STDERR: retained={} total={} overflow={} line_overflow={} read_error={:?}]\n",
                self.bytes.len(), self.total_bytes, self.overflow, self.line_overflow, self.read_error));
        }
        text
    }
}

#[cfg(test)]
mod stderr_capture_tests {
    use super::*;

    #[test]
    fn normal_short_logs_preserve_forbidden_output_and_final_unterminated_line() {
        let mut capture = StderrCapture::default();
        capture.append(b"ready\nLAN mDNS discovery enabled\nlast");
        assert!(capture.ensure_complete().is_ok());
        assert!(capture.snapshot().contains("LAN mDNS discovery enabled"));
        assert!(capture.snapshot().ends_with("last"));
    }

    #[test]
    fn capture_cap_keeps_draining_counts_and_never_certifies_a_truncated_prefix() {
        let mut capture = StderrCapture::default();
        let lines = [b'\n'; STDERR_READ_SIZE];
        for _ in 0..STDERR_CAPTURE_LIMIT / STDERR_READ_SIZE { capture.append(&lines); }
        assert!(capture.ensure_complete().is_ok());
        let cursor = capture.complete_line_end;
        capture.append(b"LAN mDNS discovery enabled\n");
        capture.append(b"still drained\n");
        assert_eq!(capture.bytes.len(), STDERR_CAPTURE_LIMIT);
        assert!(capture.total_bytes > STDERR_CAPTURE_LIMIT as u64);
        assert!(capture.overflow);
        assert!(capture.ensure_complete().is_err());
        assert!(!capture.observation(cursor).complete);
        assert!(capture.snapshot().contains("INCOMPLETE STDERR"));
    }

    #[test]
    fn delimiter_free_lines_are_bounded_and_failure_survives_later_valid_text() {
        let mut capture = StderrCapture::default();
        for _ in 0..STDERR_LINE_LIMIT / STDERR_READ_SIZE + 1 {
            capture.append(&[b'x'; STDERR_READ_SIZE]);
        }
        capture.append(b"\nready\n");
        assert_eq!(capture.bytes.len(), STDERR_LINE_LIMIT);
        assert!(capture.line_overflow);
        assert!(capture.ensure_complete().is_err());
        assert!(!capture.observation(0).complete);
        assert!(!capture.observation(STDERR_LINE_LIMIT + 1).cursor_valid);
    }

    #[test]
    fn unicode_is_decoded_after_chunks_and_cursors_remain_raw_byte_offsets() {
        let mut capture = StderrCapture::default();
        capture.append(b"first\n");
        let cursor = capture.complete_line_end;
        for byte in "α🙂\n".as_bytes() { capture.append(&[*byte]); }
        assert_eq!(capture.observation(cursor).text, "α🙂\n");
        assert!(capture.observation(cursor).complete);
        assert!(capture.ensure_complete().is_ok());
        capture.append(&[0xf0, 0x9f]);
        assert_eq!(capture.observation(cursor).text, "α🙂\n");
        assert!(capture.ensure_complete().is_err());
    }

    #[test]
    fn reader_errors_and_invalid_utf8_propagate_after_drain() {
        let mut capture = StderrCapture::default();
        capture.append(b"ready\n");
        capture.read_error = Some("synthetic read error".to_string());
        assert!(capture.ensure_complete().is_err());
        assert!(!capture.observation(0).complete);
        let mut invalid = StderrCapture::default();
        invalid.append(&[0xff, b'\n']);
        assert!(invalid.ensure_complete().is_err());
        assert!(!invalid.observation(0).complete);
    }
}

pub struct ManagedProcess {
    label: String,
    child: Child,
    lines: mpsc::UnboundedReceiver<std::io::Result<String>>,
    stderr: JoinHandle<()>,
    stderr_output: Arc<Mutex<StderrCapture>>,
    stdout: String,
}

impl ManagedProcess {
    pub fn spawn(label: impl Into<String>, command: &mut Command) -> Result<Self> {
        let label = label.into();
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawn {label}"))?;
        let stdout = child.stdout.take().context("child stdout was not piped")?;
        let stderr = child.stderr.take().context("child stderr was not piped")?;
        let (stdout_tx, lines) = mpsc::unbounded_channel();
        // Drain independently of fixture commands: an idle provider's logs must
        // never fill its pipe and block the application's networking runtime.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if stdout_tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = stdout_tx.send(Err(error));
                        break;
                    }
                }
            }
        });
        let stderr_output = Arc::new(Mutex::new(StderrCapture::default()));
        let task_output = Arc::clone(&stderr_output);
        let stderr = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut chunk = [0u8; STDERR_READ_SIZE];
            loop {
                match stderr.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(count) => task_output.lock().unwrap().append(&chunk[..count]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        task_output.lock().unwrap().read_error = Some(error.to_string());
                        break;
                    }
                }
            }
        });
        Ok(Self {
            label,
            child,
            lines,
            stderr,
            stderr_output,
            stdout: String::new(),
        })
    }

    #[allow(dead_code)]
    pub fn stderr_snapshot(&self) -> String {
        self.stderr_output.lock().unwrap().snapshot()
    }

    #[allow(dead_code)]
    pub fn stderr_cursor(&self) -> usize {
        self.stderr_output.lock().unwrap().complete_line_end
    }

    #[allow(dead_code)]
    pub fn stderr_observation(&self, offset: usize) -> StderrObservation {
        self.stderr_output.lock().unwrap().observation(offset)
    }

    #[allow(dead_code)] // Optional resource sample in the native product gate.
    pub async fn cpu_seconds(&self) -> Result<Option<f64>> {
        if !cfg!(unix) {
            return Ok(None);
        }
        let pid = self
            .child
            .id()
            .context("process exited before CPU sample")?;
        let mut command = Command::new(if cfg!(target_os = "linux") {
            "getconf"
        } else {
            "ps"
        });
        if cfg!(target_os = "linux") {
            command.arg("CLK_TCK");
        } else {
            command.args(["-o", "time=", "-p", &pid.to_string()]);
        }
        let output = command.output().await;
        let output = match output {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            result => result.context("read cumulative process CPU time")?,
        };
        ensure!(
            output.status.success(),
            "CPU sampler failed for {}",
            self.label
        );
        let raw = String::from_utf8(output.stdout)?;
        #[cfg(target_os = "linux")]
        {
            // Linux ps rounds to whole seconds, too coarse for the idle budget.
            let ticks_per_second: f64 = raw.trim().parse()?;
            ensure!(
                ticks_per_second.is_finite() && ticks_per_second > 0.0,
                "invalid process clock tick rate"
            );
            if !Path::new("/proc/self/stat").exists() {
                return Ok(None);
            }
            let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
            let mut fields = stat
                .rsplit_once(") ")
                .context("parse process stat")?
                .1
                .split_whitespace();
            let user: u64 = fields.nth(11).context("missing user CPU ticks")?.parse()?;
            let system: u64 = fields.next().context("missing system CPU ticks")?.parse()?;
            Ok(Some((user + system) as f64 / ticks_per_second))
        }
        #[cfg(not(target_os = "linux"))]
        {
            // These freshly started processes report [hours:]minutes:seconds.
            raw.trim()
                .split(':')
                .try_fold(0.0, |total, part| {
                    Ok(total * 60.0 + part.parse::<f64>().context("parse CPU time")?)
                })
                .map(Some)
        }
    }

    #[allow(dead_code)]
    pub fn stdout_snapshot(&mut self) -> String {
        while let Ok(Ok(line)) = self.lines.try_recv() {
            self.stdout.push_str(&line);
            self.stdout.push('\n');
        }
        self.stdout.clone()
    }

    #[allow(dead_code)] // Used by other process-test crates sharing this helper.
    pub async fn line_containing(&mut self, marker: &str) -> Result<String> {
        self.wait_for_line(marker, |line| {
            line.contains(marker).then(|| line.to_string())
        })
        .await
    }

    #[allow(dead_code)] // Used by product-process tests, not every integration-test crate.
    pub async fn json_event(&mut self, event: &str) -> Result<Value> {
        self.wait_for_line(&format!("JSON event {event}"), |line| {
            let value: Value = serde_json::from_str(line).ok()?;
            (value.get("event").and_then(Value::as_str) == Some(event)).then_some(value)
        })
        .await
    }

    async fn wait_for_line<T>(
        &mut self,
        marker: &str,
        matches: impl Fn(&str) -> Option<T>,
    ) -> Result<T> {
        let label = self.label.clone();
        timeout(WAIT, async {
            loop {
                let Some(line) = self
                    .lines
                    .recv()
                    .await
                    .transpose()
                    .with_context(|| format!("read {label} stdout"))?
                else {
                    bail!(
                        "{label} exited before {marker}; stdout:\n{}\nstderr:\n{}",
                        self.stdout,
                        self.stderr_snapshot()
                    );
                };
                self.stdout.push_str(&line);
                self.stdout.push('\n');
                if let Some(value) = matches(&line) {
                    return Ok(value);
                }
            }
        })
        .await
        .with_context(|| {
            format!(
                "timed out waiting for {marker} from {label}; stdout:\n{}\nstderr:\n{}",
                self.stdout,
                self.stderr_snapshot()
            )
        })?
    }

    pub async fn send_line(&mut self, line: &str) -> Result<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .context("child stdin was closed")?;
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .with_context(|| format!("write to {}", self.label))?;
        stdin
            .flush()
            .await
            .with_context(|| format!("flush {} stdin", self.label))
    }

    pub async fn kill(mut self) -> Result<CapturedProcess> {
        if self
            .child
            .try_wait()
            .with_context(|| format!("inspect {}", self.label))?
            .is_none()
        {
            self.child
                .kill()
                .await
                .with_context(|| format!("kill {}", self.label))?;
        }
        self.collect().await
    }

    pub async fn finish(self) -> Result<CapturedProcess> {
        let label = self.label.clone();
        let output = self.collect().await?;
        if !output.status.success() {
            bail!(
                "{label} failed with {}; stdout:\n{}\nstderr:\n{}",
                output.status,
                output.stdout,
                output.stderr
            );
        }
        Ok(output)
    }

    async fn collect(mut self) -> Result<CapturedProcess> {
        let label = self.label.clone();
        let status = timeout(WAIT, async {
            while let Some(line) = self.lines.recv().await {
                self.stdout.push_str(&line.context("drain process stdout")?);
                self.stdout.push('\n');
            }
            let status = self
                .child
                .wait()
                .await
                .with_context(|| format!("wait for {label}"))?;
            Ok::<_, anyhow::Error>(status)
        })
        .await
        .with_context(|| format!("timed out collecting {label}"))??;
        timeout(WAIT, self.stderr)
            .await
            .with_context(|| format!("timed out collecting {label} stderr"))?
            .context("join stderr reader")?;
        // All own pipe readers and the child are closed before rejecting an
        // incomplete capture. Truncation can never certify no-LAN/privacy output.
        let capture = self.stderr_output.lock().unwrap();
        capture.ensure_complete().with_context(|| format!("{label} stderr evidence incomplete"))?;
        let stderr = capture.snapshot();
        Ok(CapturedProcess {
            status,
            stdout: self.stdout,
            stderr,
        })
    }
}
