use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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

pub struct ManagedProcess {
    label: String,
    child: Child,
    lines: mpsc::UnboundedReceiver<std::io::Result<String>>,
    stderr: JoinHandle<()>,
    stderr_output: Arc<Mutex<String>>,
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
        let stderr_output = Arc::new(Mutex::new(String::new()));
        let task_output = Arc::clone(&stderr_output);
        let stderr = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut output = task_output.lock().unwrap();
                output.push_str(&line);
                output.push('\n');
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
        self.stderr_output.lock().unwrap().clone()
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
            ensure!(ticks_per_second > 0.0, "invalid process clock tick rate");
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
        let stderr = self.stderr_output.lock().unwrap().clone();
        Ok(CapturedProcess {
            status,
            stdout: self.stdout,
            stderr,
        })
    }
}
