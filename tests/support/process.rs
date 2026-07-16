use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout, Command};
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
    lines: Lines<BufReader<ChildStdout>>,
    stderr: JoinHandle<String>,
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
        let mut stderr = child.stderr.take().context("child stderr was not piped")?;
        let stderr = tokio::spawn(async move {
            let mut output = String::new();
            let _ = stderr.read_to_string(&mut output).await;
            output
        });
        Ok(Self {
            label,
            child,
            lines: BufReader::new(stdout).lines(),
            stderr,
            stdout: String::new(),
        })
    }

    pub async fn line_containing(&mut self, marker: &str) -> Result<String> {
        let label = self.label.clone();
        timeout(WAIT, async {
            loop {
                let Some(line) = self
                    .lines
                    .next_line()
                    .await
                    .with_context(|| format!("read {label} stdout"))?
                else {
                    bail!("{label} exited before {marker}; stdout:\n{}", self.stdout);
                };
                self.stdout.push_str(&line);
                self.stdout.push('\n');
                if line.contains(marker) {
                    return Ok(line);
                }
            }
        })
        .await
        .with_context(|| {
            format!(
                "timed out waiting for {marker} from {label}; stdout:\n{}",
                self.stdout
            )
        })?
    }

    #[allow(dead_code)] // Used by product-process tests, not every integration-test crate.
    pub async fn json_event(&mut self, event: &str) -> Result<Value> {
        let label = self.label.clone();
        timeout(WAIT, async {
            loop {
                let Some(line) = self
                    .lines
                    .next_line()
                    .await
                    .with_context(|| format!("read {label} stdout"))?
                else {
                    bail!(
                        "{label} exited before JSON event {event}; stdout:\n{}",
                        self.stdout
                    );
                };
                self.stdout.push_str(&line);
                self.stdout.push('\n');
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if value.get("event").and_then(Value::as_str) == Some(event) {
                    return Ok(value);
                }
            }
        })
        .await
        .with_context(|| {
            format!(
                "timed out waiting for JSON event {event} from {label}; stdout:\n{}",
                self.stdout
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
        let mut reader = self.lines.into_inner();
        let (rest, status) = timeout(WAIT, async {
            let mut rest = String::new();
            reader
                .read_to_string(&mut rest)
                .await
                .with_context(|| format!("drain {label} stdout"))?;
            let status = self
                .child
                .wait()
                .await
                .with_context(|| format!("wait for {label}"))?;
            Ok::<_, anyhow::Error>((rest, status))
        })
        .await
        .with_context(|| format!("timed out collecting {label}"))??;
        self.stdout.push_str(&rest);
        let stderr = timeout(WAIT, self.stderr)
            .await
            .with_context(|| format!("timed out collecting {label} stderr"))?
            .context("join stderr reader")?;
        Ok(CapturedProcess {
            status,
            stdout: self.stdout,
            stderr,
        })
    }
}
