use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, RecvTimeoutError, sync_channel},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

pub trait AtTransport: Send {
    fn execute(
        &mut self,
        command: &str,
        payload: Option<&str>,
        timeout: Option<Duration>,
    ) -> Result<String>;
    fn drain(&mut self, timeout: Duration);
    fn set_sms_notify(&mut self, _notify: Arc<Notify>) {}
    fn sms_list(&mut self) -> Result<String> {
        self.checked("AT+CMGF=0")?;
        self.checked("AT+CNMI=2,1,0,0,0")?;
        let supported = self.checked("AT+CPMS=?")?;
        let banks = supported
            .lines()
            .find_map(|line| {
                let part = line.trim().strip_prefix("+CPMS:")?;
                Some(crate::parser::fields(
                    part.split_once('(')?.1.split_once(')')?.0,
                ))
            })
            .ok_or_else(|| anyhow::anyhow!("SMS storage capabilities missing"))?;
        let mut raw = String::new();
        for storage in [crate::sms::Storage::ME, crate::sms::Storage::SM] {
            if banks.iter().any(|s| s == storage.name()) {
                let response = self.sms_in_storage(storage, "AT+CMGL=4")?;
                raw.push_str(&format!("+SASTORE: {}\n{response}\n", storage.name()));
            }
        }
        if raw.is_empty() {
            anyhow::bail!("no supported SMS storage")
        }
        Ok(raw)
    }
    fn sms_in_storage(&mut self, storage: crate::sms::Storage, command: &str) -> Result<String> {
        let current = self.checked("AT+CPMS?")?;
        let previous = current
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("+CPMS:")
                    .and_then(|v| crate::parser::fields(v).first().cloned())
            })
            .ok_or_else(|| anyhow::anyhow!("SMS storage query returned no storage"))?;
        let result = (|| {
            self.checked(&format!("AT+CPMS=\"{}\"", storage.name()))?;
            let mut raw = String::new();
            for part in crate::at::split(command) {
                raw.push_str(&self.checked(&part)?);
            }
            Ok(raw)
        })();
        let restored = self.checked(&format!("AT+CPMS=\"{previous}\""));
        match (result, restored) {
            (Ok(raw), Ok(_)) => Ok(raw),
            (Err(error), Ok(_)) => Err(error),
            (_, Err(error)) => Err(error.context("SMS storage restoration failed")),
        }
    }
    fn checked(&mut self, command: &str) -> Result<String> {
        let raw = self.execute(command, None, Some(Duration::from_secs(10)))?;
        if !crate::parser::ok(&raw) {
            anyhow::bail!("AT command rejected ({command}): {raw}")
        }
        Ok(raw)
    }
}

pub struct UsbAtTransport {
    path: PathBuf,
    writer: File,
    rx: Receiver<Vec<u8>>,
    timeout: Duration,
    sms_notify: Arc<Mutex<Option<Arc<Notify>>>>,
}

impl UsbAtTransport {
    pub fn open(path: &Path, timeout: Duration) -> Result<Self> {
        let mut reader = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(path)
            .with_context(|| format!("open USB AT device {}", path.display()))?;
        configure_tty(&mut reader)?;
        let writer = reader.try_clone()?;
        let (tx, rx) = sync_channel(16);
        let reader_notify: Arc<Mutex<Option<Arc<Notify>>>> = Arc::new(Mutex::new(None));
        let reader_sms_notify = reader_notify.clone();
        std::thread::Builder::new()
            .name("at-reader".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let mut buf = [0u8; 4096];
                let mut line = Vec::with_capacity(128);
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Some(notify) = reader_sms_notify.lock().unwrap().as_ref() {
                                for byte in &buf[..n] {
                                    if *byte == b'\n' {
                                        if line.starts_with(b"+CMTI:") {
                                            notify.notify_one();
                                        }
                                        line.clear();
                                    } else if *byte != b'\r' && line.len() < 512 {
                                        line.push(*byte);
                                    }
                                }
                            }
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            writer,
            rx,
            timeout,
            sms_notify: reader_notify,
        })
    }
    fn receive(&mut self, timeout: Duration, prompt: bool) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::new();
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.rx.recv_timeout(remaining) {
                Ok(chunk) => {
                    if bytes.len() + chunk.len() > 512 * 1024 {
                        bail!("AT response too large");
                    }
                    bytes.extend_from_slice(&chunk);
                    let raw = String::from_utf8_lossy(&bytes);
                    let terminal = raw.lines().any(|line| {
                        let line = line.trim();
                        matches!(line, "OK" | "ERROR")
                            || line.starts_with("+CME ERROR:")
                            || line.starts_with("+CMS ERROR:")
                    });
                    if (terminal && (!prompt || raw.contains("ERROR")))
                        || (prompt && raw.trim_end().ends_with('>'))
                    {
                        return Ok(raw.into_owned());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("AT device disconnected: {}", self.path.display())
                }
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        bail!("AT response timed out on {}", self.path.display())
    }
}

impl AtTransport for UsbAtTransport {
    fn execute(
        &mut self,
        command: &str,
        payload: Option<&str>,
        timeout: Option<Duration>,
    ) -> Result<String> {
        if let Some(payload) = payload {
            let parts = crate::at::split(command);
            if let Some((send, setup)) = parts.split_last()
                && !setup.is_empty()
            {
                for part in setup {
                    let response = self.execute(part, None, timeout)?;
                    if !crate::parser::ok(&response) {
                        bail!("SMS setup rejected ({part}): {response}");
                    }
                }
                return self.execute(send, Some(payload), timeout);
            }
        }

        self.drain(Duration::from_millis(100));
        self.writer.write_all(format!("{command}\r\n").as_bytes())?;
        let mut response = match self.receive(timeout.unwrap_or(self.timeout), payload.is_some()) {
            Ok(response) => response,
            Err(error) => {
                if payload.is_some() {
                    let _ = self.writer.write_all(b"\x1b");
                    self.drain(Duration::from_millis(100));
                }
                return Err(error);
            }
        };
        if let Some(payload) = payload {
            if !response.trim_end().ends_with('>') {
                bail!("SMS request rejected before PDU submission ({command}): {response}");
            }
            self.writer.write_all(format!("{payload}\x1a").as_bytes())?;
            response.push_str(&self.receive(Duration::from_secs(60), false)?);
        }
        Ok(response)
    }
    fn drain(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self
                .rx
                .recv_timeout(remaining.min(Duration::from_millis(30)))
            {
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
    fn set_sms_notify(&mut self, notify: Arc<Notify>) {
        *self.sms_notify.lock().unwrap() = Some(notify);
    }
}

fn configure_tty(file: &mut File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = file.as_raw_fd();
        unsafe {
            let mut term = std::mem::zeroed();
            if libc::tcgetattr(fd, &mut term) == 0 {
                libc::cfmakeraw(&mut term);
                libc::cfsetispeed(&mut term, libc::B115200);
                libc::cfsetospeed(&mut term, libc::B115200);
                if libc::tcsetattr(fd, libc::TCSANOW, &term) != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
    }
    Ok(())
}

pub struct MockAtTransport {
    pub responses: HashMap<String, String>,
    fixtures: Option<HashMap<String, String>>,
    overrides: Option<Arc<Mutex<HashMap<String, String>>>>,
}
impl MockAtTransport {
    pub fn new(responses: HashMap<String, String>) -> Self {
        Self {
            responses,
            fixtures: None,
            overrides: None,
        }
    }
    pub fn fixture(
        fixtures: HashMap<String, String>,
        overrides: Arc<Mutex<HashMap<String, String>>>,
    ) -> Self {
        Self {
            responses: HashMap::new(),
            fixtures: Some(fixtures),
            overrides: Some(overrides),
        }
    }
}
impl AtTransport for MockAtTransport {
    fn execute(
        &mut self,
        command: &str,
        _payload: Option<&str>,
        _timeout: Option<Duration>,
    ) -> Result<String> {
        if let (Some(fixtures), Some(overrides)) = (&self.fixtures, &self.overrides) {
            return Ok(crate::mock::response(
                command,
                fixtures,
                &overrides.lock().unwrap(),
            ));
        }
        Ok(self
            .responses
            .get(command)
            .cloned()
            .unwrap_or_else(|| "\r\nOK\r\n".into()))
    }
    fn drain(&mut self, _timeout: Duration) {}
    fn sms_list(&mut self) -> Result<String> {
        if let (Some(fixtures), Some(overrides)) = (&self.fixtures, &self.overrides) {
            return Ok(crate::mock::response(
                crate::at::SMS_LIST,
                fixtures,
                &overrides.lock().unwrap(),
            ));
        }
        AtTransport::sms_list(self)
    }
    fn sms_in_storage(&mut self, storage: crate::sms::Storage, command: &str) -> Result<String> {
        if let (Some(fixtures), Some(overrides)) = (&self.fixtures, &self.overrides) {
            return Ok(crate::mock::response(
                command,
                fixtures,
                &overrides.lock().unwrap(),
            ));
        }
        AtTransport::sms_in_storage(self, storage, command)
    }
}
