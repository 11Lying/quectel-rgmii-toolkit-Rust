use crate::at_transport::AtTransport;
use anyhow::{Context, Result};
use serde::Serialize;
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    ById,
    Sysfs,
    DeviceScan,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub source: DiscoverySource,
    pub usb_id: Option<String>,
    pub interface: Option<String>,
    pub verified: bool,
    pub reason: String,
}

pub fn discover() -> Vec<Candidate> {
    let mut result = Vec::new();
    let mut add = |path: PathBuf, source: DiscoverySource, reason: String| {
        if !path.exists()
            || result
                .iter()
                .any(|candidate: &Candidate| candidate.path == path)
        {
            return;
        }
        result.push(Candidate {
            path,
            source,
            usb_id: None,
            interface: None,
            verified: false,
            reason,
        });
    };
    if let Ok(entries) = fs::read_dir("/dev/serial/by-id") {
        let mut paths: Vec<_> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        for path in paths {
            add(
                path,
                DiscoverySource::ById,
                "stable serial alias; modem identity is unverified".into(),
            );
        }
    }
    for pattern in ["/dev/ttyUSB", "/dev/ttyACM"] {
        for index in 0..32 {
            add(
                PathBuf::from(format!("{pattern}{index}")),
                DiscoverySource::DeviceScan,
                "serial device scan; modem identity is unverified".into(),
            );
        }
    }
    result
}

pub fn probe_candidate(candidate: &Candidate, timeout: std::time::Duration) -> Result<ProbeResult> {
    let mut transport = crate::at_transport::UsbAtTransport::open(&candidate.path, timeout)
        .with_context(|| format!("open candidate {}", candidate.path.display()))?;
    let at = transport.execute("AT", None, Some(timeout))?;
    let ati = transport.execute("ATI", None, Some(timeout))?;
    let manufacturer = transport.execute("AT+CGMI", None, Some(timeout))?;
    let model = transport.execute("AT+CGMM", None, Some(timeout))?;
    Ok(ProbeResult {
        path: candidate.path.clone(),
        at,
        ati,
        manufacturer,
        model,
        verified: false,
        reason: "safe probe completed; RM502Q-AE identity requires explicit profile matching"
            .into(),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ProbeResult {
    pub path: PathBuf,
    pub at: String,
    pub ati: String,
    pub manufacturer: String,
    pub model: String,
    pub verified: bool,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_does_not_invent_fixed_tty() {
        assert!(
            !discover()
                .iter()
                .any(|candidate| candidate.path == PathBuf::from("/dev/smd11"))
        );
    }
}
