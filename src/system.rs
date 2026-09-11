use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;
pub fn uptime(mock: bool) -> (Value, String) {
    let seconds = if mock {
        93000
    } else {
        std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0) as u64
    };
    let (d, h, m, s) = (
        seconds / 86400,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
    );
    (
        json!({"days":d,"hours":h,"minutes":m,"seconds":s}),
        format!("{d} days, {h} hours, {m} minutes, {s} seconds"),
    )
}
pub async fn command(name: &str, args: &[&str]) -> Result<String> {
    let mut cmd = tokio::process::Command::new(name);
    cmd.args(args)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .context("system command timeout")??;
    if !output.status.success() {
        bail!(
            "{name} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
