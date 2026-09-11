use crate::{
    at_policy as policy,
    at_transport::{AtTransport, MockAtTransport, UsbAtTransport},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

pub const DASHBOARD: &str = "AT+QSIMSTAT?;+CSQ;+QTEMP;+QUIMSLOT?;+QSPN;+QENG=\"servingcell\";+QCAINFO;+QGDNRCNT?;+QGDCNT?;+CGCONTRDP=1;+QRSRP";
pub const SIGNAL: &str = "AT+QTEMP;+QENG=\"servingcell\"";
pub const SMS_LIST: &str = "AT+CMGF=0;+CNMI=2,1,0,0,0;+CMGL=4";
pub fn commands(page: &str) -> Vec<&'static str> {
    match page {
        "dashboard" => vec![DASHBOARD],
        "device" => vec![
            "AT+CGMI;+CGSN;+QGMR;+CIMI;+ICCID;+CNUM",
            "AT+QSIMSTAT?;+CPIN?;+QUIMSLOT?",
        ],
        "network" => vec![
            "AT+QUIMSLOT?;+QNWPREFCFG=\"mode_pref\";+QNWPREFCFG=\"nr5g_disable_mode\";+CGDCONT?;+CGCONTRDP=1;+QNWLOCK=\"common/4g\";+QNWLOCK=\"common/5g\"",
            "AT+QCAINFO",
        ],
        "bands" => vec![
            "AT+QNWPREFCFG=\"lte_band\";+QNWPREFCFG=\"nsa_nr5g_band\";+QNWPREFCFG=\"nr5g_band\"",
        ],
        "settings" => vec!["AT+CGSN"],
        "sms" => vec![SMS_LIST],
        "model" => vec!["AT+CGMM"],
        _ => vec![],
    }
}
struct Request {
    command: String,
    sms: Option<String>,
    storage: Option<crate::sms::Storage>,
    timeout: Option<Duration>,
    reply: oneshot::Sender<Result<String>>,
}
struct Entry {
    requested: Instant,
    updated: Option<Instant>,
    response: Arc<str>,
    running: Option<tokio::sync::watch::Receiver<bool>>,
}
const CACHE_BYTES: usize = 1024 * 1024;

fn trim_cache(cache: &mut HashMap<String, Entry>, keep: &str) {
    while cache.values().map(|e| e.response.len()).sum::<usize>() > CACHE_BYTES {
        let key = cache
            .iter()
            .filter(|(key, entry)| key.as_str() != keep && entry.running.is_none())
            .min_by_key(|(_, entry)| entry.updated)
            .map(|(key, _)| key.clone());
        let Some(key) = key else { break };
        cache.remove(&key);
    }
}

fn cacheable(command: &str) -> bool {
    if command == SMS_LIST {
        return true;
    }
    static ALLOWED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let allowed = ALLOWED.get_or_init(|| {
        [
            "dashboard",
            "device",
            "network",
            "bands",
            "settings",
            "model",
        ]
        .iter()
        .flat_map(|page| commands(page))
        .flat_map(split)
        .map(|s| s.replace(' ', "").to_ascii_uppercase())
        .collect()
    });
    split(command)
        .iter()
        .all(|part| allowed.contains(&part.replace(' ', "").to_ascii_uppercase()))
}
#[derive(Clone)]
pub struct At {
    #[cfg(test)]
    pub trace: Arc<Mutex<Vec<String>>>,
    tx: mpsc::Sender<Request>,
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    pub overrides: Arc<Mutex<HashMap<String, String>>>,
    pub mock: bool,
    pub sms_changed: Arc<tokio::sync::Notify>,
    ready: Instant,
}
impl At {
    pub fn start(mock: bool, devices: Vec<String>) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel::<Request>(16);
        let overrides = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let mocks = overrides.clone();
        let sms_changed = Arc::new(tokio::sync::Notify::new());
        let worker_sms_changed = sms_changed.clone();
        std::thread::Builder::new()
            .name("at-worker".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                let fixtures: HashMap<String, String> =
                    serde_json::from_str(include_str!("../tests/fixtures/mock-at.json")).unwrap();
                let mut transport: Option<Box<dyn AtTransport>> = None;
                let mut connected = false;
                while let Some(request) = rx.blocking_recv() {
                    if request.reply.is_closed() {
                        continue;
                    }
                    let result = if mock {
                        if transport.is_none() {
                            transport = Some(Box::new(MockAtTransport::fixture(
                                fixtures.clone(),
                                mocks.clone(),
                            )));
                        }
                        let transport = transport.as_mut().unwrap();
                        if request.command == SMS_LIST {
                            transport.sms_list()
                        } else if let Some(storage) = request.storage {
                            transport.sms_in_storage(storage, &request.command)
                        } else {
                            transport.execute(
                                &request.command,
                                request.sms.as_deref(),
                                request.timeout,
                            )
                        }
                    } else {
                        (|| -> Result<String> {
                            if !connected {
                                let mut last = anyhow::anyhow!("no AT device available");
                                for path in &devices {
                                    match UsbAtTransport::open(
                                        std::path::Path::new(path),
                                        Duration::from_secs(10),
                                    ) {
                                        Ok(value) => {
                                            let mut value = Box::new(value);
                                            value.set_sms_notify(worker_sms_changed.clone());
                                            transport = Some(value);
                                            connected = true;
                                            break;
                                        }
                                        Err(error) => last = error,
                                    }
                                }
                                if !connected {
                                    return Err(last);
                                }
                            }
                            let _lock = global_lock()?;
                            let result = {
                                if request.command == SMS_LIST {
                                    transport.as_mut().unwrap().sms_list()
                                } else if let Some(storage) = request.storage {
                                    transport
                                        .as_mut()
                                        .unwrap()
                                        .sms_in_storage(storage, &request.command)
                                } else {
                                    transport.as_mut().unwrap().execute(
                                        &request.command,
                                        request.sms.as_deref(),
                                        request.timeout,
                                    )
                                }
                            };
                            if result.is_err() {
                                if let Some(value) = transport.as_mut() {
                                    value.drain(Duration::from_millis(100));
                                }
                                connected = false;
                            }
                            result
                        })()
                    };
                    let _ = request.reply.send(result);
                }
            })?;
        let uptime = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(35.0);
        let delay = if mock { 0.0 } else { (35.0 - uptime).max(0.0) };
        Ok(Self {
            #[cfg(test)]
            trace: Arc::new(Mutex::new(Vec::new())),
            tx,
            cache: Arc::new(Mutex::new(HashMap::new())),
            overrides,
            mock,
            sms_changed,
            ready: Instant::now() + Duration::from_secs_f64(delay),
        })
    }
    pub async fn run(&self, command: &str) -> Result<String> {
        self.transaction(command, None).await
    }
    pub async fn transaction(&self, command: &str, sms: Option<String>) -> Result<String> {
        self.transaction_timeout(command, sms, None).await
    }
    pub async fn wait_ready(&self) {
        tokio::time::sleep_until(self.ready.into()).await;
    }
    pub async fn transaction_timeout(
        &self,
        command: &str,
        sms: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<String> {
        policy::validate(command)?;
        self.request(command, sms, timeout, None).await
    }

    pub async fn sms_list(&self, force: bool) -> Result<String> {
        self.page("sms", force).await
    }

    pub async fn sms_send(&self, pdu: String, length: usize) -> Result<String> {
        self.transaction(&format!("AT+CMGF=0;+CMGS={length}"), Some(pdu))
            .await
    }
    pub async fn delete_sms(
        &self,
        storage: crate::sms::Storage,
        indices: &[u16],
        all: bool,
    ) -> Result<String> {
        let command = if all {
            "AT+CMGD=1,4".to_owned()
        } else {
            if indices.is_empty() || indices.len() > 1024 {
                bail!("invalid SMS indices")
            }
            format!(
                "AT{}",
                indices
                    .iter()
                    .map(|i| format!("+CMGD={i}"))
                    .collect::<Vec<_>>()
                    .join(";")
            )
        };
        let response = self.request(&command, None, None, Some(storage)).await?;
        if !crate::parser::ok(&response) {
            bail!("SMS deletion rejected: {response}")
        }
        self.invalidate().await;
        Ok(response)
    }
    async fn request(
        &self,
        command: &str,
        sms: Option<String>,
        timeout: Option<Duration>,
        storage: Option<crate::sms::Storage>,
    ) -> Result<String> {
        if command.len() > 4096 || command.chars().any(|c| c.is_control()) {
            bail!("invalid AT command")
        }
        #[cfg(test)]
        {
            let mut trace = self.trace.lock().unwrap();
            if let Some(storage) = storage {
                trace.push(format!("AT+CPMS=\"{}\"", storage.name()));
            }
            trace.push(command.into());
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(Request {
                command: command.into(),
                sms,
                storage,
                timeout,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("AT queue busy"))?;
        rx.await.context("AT worker stopped")?
    }
    pub async fn fetch(&self, command: &str, force: bool) -> Result<String> {
        self.fetch_wait(command, force, true).await
    }
    pub async fn fetch_wait(&self, command: &str, force: bool, wait: bool) -> Result<String> {
        policy::validate(command)?;
        if !cacheable(command) {
            let result = self.run(command).await;
            self.invalidate().await;
            return result;
        }
        let delayed = self.ready > Instant::now() && !policy::immediate(command);
        let (mut completion, old) = {
            let mut cache = self.cache.lock().unwrap();
            if wait && let Some(entry) = cache.get_mut(command) {
                entry.requested = Instant::now();
            }
            if let Some(entry) = cache.get(command)
                && !force
                && entry
                    .updated
                    .is_some_and(|time| time.elapsed() < policy::max_age(command))
            {
                return Ok(entry.response.to_string());
            }
            if !cache.contains_key(command) {
                if cache.len() >= 64 {
                    let key = cache
                        .iter()
                        .filter(|(_, e)| e.running.is_none())
                        .min_by_key(|(_, e)| e.updated)
                        .map(|(k, _)| k.clone())
                        .context("AT cache busy")?;
                    cache.remove(&key);
                }
                cache.insert(
                    command.into(),
                    Entry {
                        requested: Instant::now(),
                        updated: None,
                        response: Arc::from(policy::PENDING),
                        running: None,
                    },
                );
            }
            let entry = cache.get_mut(command).unwrap();
            let old = entry.response.clone();
            let receiver = if let Some(receiver) = &entry.running {
                receiver.clone()
            } else {
                let (sender, receiver) = tokio::sync::watch::channel(false);
                entry.running = Some(receiver.clone());
                let this = self.clone();
                let command = command.to_owned();
                tokio::spawn(async move {
                    if !policy::immediate(&command)
                        && let Some(delay) = this.ready.checked_duration_since(Instant::now())
                    {
                        tokio::time::sleep(delay).await
                    }
                    let response = this
                        .run(&command)
                        .await
                        .unwrap_or_else(|e| format!("ERROR: {e}"));
                    let mut cache = this.cache.lock().unwrap();
                    if let Some(entry) = cache.get_mut(&command) {
                        entry.response = Arc::from(response);
                        entry.updated = Some(Instant::now());
                        entry.running = None;
                    }
                    trim_cache(&mut cache, &command);
                    let _ = sender.send(true);
                });
                receiver
            };
            (receiver, old)
        };
        if !wait || delayed {
            return Ok(old.to_string());
        }
        if !*completion.borrow()
            && tokio::time::timeout(
                policy::timeout(command) + Duration::from_secs(2),
                completion.changed(),
            )
            .await
            .is_err()
        {
            return Ok(policy::PENDING.into());
        }
        Ok(self
            .cache
            .lock()
            .unwrap()
            .get(command)
            .map(|e| e.response.to_string())
            .unwrap_or_else(|| policy::PENDING.into()))
    }
    pub async fn page(&self, page: &str, force: bool) -> Result<String> {
        let mut responses = Vec::new();
        for command in commands(page) {
            responses.push(self.fetch(command, force).await?)
        }
        Ok(responses.join("\n"))
    }
    pub async fn dashboard_sample(&self) -> Result<(String, Option<Instant>)> {
        self.fetch(DASHBOARD, true).await?;
        let cache = self.cache.lock().unwrap();
        let entry = cache
            .get(DASHBOARD)
            .context("dashboard sample unavailable")?;
        Ok((entry.response.to_string(), entry.updated))
    }
    pub async fn invalidate(&self) {
        for entry in self.cache.lock().unwrap().values_mut() {
            entry.updated = None;
        }
    }
    pub fn start_refresh(&self) {
        let this = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = this.fetch_wait(DASHBOARD, false, false).await;
            let mut tick = tokio::time::interval(Duration::from_secs(15));
            loop {
                tick.tick().await;
                let commands: Vec<_> = this
                    .cache
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(c, e)| {
                        c.as_str() != DASHBOARD
                            && e.requested.elapsed() < Duration::from_secs(120)
                            && !policy::immediate(c)
                            && !policy::sms(c)
                            && e.running.is_none()
                            && e.updated.is_none_or(|t| t.elapsed() >= policy::max_age(c))
                    })
                    .map(|(c, _)| c.clone())
                    .collect();
                for command in commands {
                    let _ = this.fetch_wait(&command, false, false).await;
                }
            }
        });
    }
}
pub fn split(command: &str) -> Vec<String> {
    let mut quoted = false;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, ch) in command.char_indices() {
        if ch == '"' {
            quoted = !quoted
        }
        if ch == ';' && !quoted {
            parts.push(command[start..i].trim());
            start = i + 1
        }
    }
    parts.push(command[start..].trim());
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|p| {
            if p.to_ascii_uppercase().starts_with("AT") {
                p.into()
            } else {
                format!("AT{p}")
            }
        })
        .collect()
}
fn terminal(raw: &str) -> bool {
    raw.lines().any(|l| {
        let l = l.trim();
        l == "OK" || l == "ERROR" || l.starts_with("+CME ERROR:") || l.starts_with("+CMS ERROR:")
    })
}

#[cfg(unix)]
fn global_lock() -> Result<std::fs::File> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    // Shared legacy lock name prevents concurrent AT writes during upgrades.
    let path = std::env::var("SIMPLEADMIN_AT_LOCK_FILE")
        .unwrap_or_else(|_| "/tmp/simpleadmin-go-at.lock".into());
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(file);
        }
        if Instant::now() >= deadline {
            bail!("AT port busy")
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(not(unix))]
fn global_lock() -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::io::{Read, Write};
    #[tokio::test]
    async fn background_refresh_does_not_extend_page_interest() {
        let at = At::start(true, vec![]).unwrap();
        let command = "AT+CGMM";
        at.fetch(command, false).await.unwrap();
        let old = Instant::now() - Duration::from_secs(121);
        at.cache.lock().unwrap().get_mut(command).unwrap().requested = old;
        at.fetch_wait(command, false, false).await.unwrap();
        assert_eq!(at.cache.lock().unwrap()[command].requested, old);
        at.fetch(command, false).await.unwrap();
        assert!(at.cache.lock().unwrap()[command].requested.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn only_known_queries_are_refreshed_and_cache_bytes_are_bounded() {
        assert!(cacheable(DASHBOARD));
        assert!(cacheable(SIGNAL));
        assert!(cacheable(SMS_LIST));
        assert!(cacheable("AT+QNWPREFCFG=\"nr5g_band\""));
        assert!(!cacheable("AT+QUIMSLOT=2"));
        assert!(!cacheable("AT+CSQ;+QCFG=\"unknown\",1"));
        let mut cache = HashMap::new();
        for i in 0..4 {
            cache.insert(
                i.to_string(),
                Entry {
                    requested: Instant::now(),
                    updated: Some(Instant::now()),
                    response: Arc::from("x".repeat(512 * 1024)),
                    running: None,
                },
            );
            trim_cache(&mut cache, &i.to_string());
        }
        assert!(cache.values().map(|e| e.response.len()).sum::<usize>() <= CACHE_BYTES);
        assert!(cache.contains_key("3"));
    }
    #[test]
    #[cfg(unix)]
    fn pty_sms_storage_restore_on_read_and_delete_failure() {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        let mut name = [0 as libc::c_char; 128];
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    name.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap();
        let mut master = unsafe { std::fs::File::from_raw_fd(master) };
        let _slave = unsafe { std::fs::File::from_raw_fd(slave) };
        let notify = Arc::new(tokio::sync::Notify::new());
        let mut port =
            UsbAtTransport::open(std::path::Path::new(path), Duration::from_secs(10)).unwrap();
        port.set_sms_notify(notify.clone());
        let modem = std::thread::spawn(move || {
            let ok = "\r\nOK\r\n";
            let current = "\r\n+CPMS: \"MT\",0,100,\"ME\",0,100,\"SM\",0,100\r\nOK\r\n";
            for fail in [false, true] {
                for (command, response) in [
                    ("AT+CMGF=0", ok),
                    ("AT+CNMI=2,1,0,0,0", ok),
                    (
                        "AT+CPMS=?",
                        "\r\n+CPMS: (\"ME\",\"SM\",\"MT\"),(\"ME\",\"SM\"),(\"SM\")\r\nOK\r\n",
                    ),
                    ("AT+CPMS?", current),
                    ("AT+CPMS=\"ME\"", ok),
                    (
                        "AT+CMGL=4",
                        "\r\n+CMGL: 1,\"REC READ\",\"10086\",,\"26/09/08,12:00:00+32\"\r\nME message\r\nOK\r\n",
                    ),
                    ("AT+CPMS=\"MT\"", ok),
                    ("AT+CPMS?", current),
                    ("AT+CPMS=\"SM\"", ok),
                    (
                        "AT+CMGL=4",
                        if fail {
                            "\r\n+CMS ERROR: 500\r\n"
                        } else {
                            "\r\n+CMGL: 1,\"REC READ\",\"10086\",,\"26/09/08,12:00:00+32\"\r\nSM message\r\nOK\r\n"
                        },
                    ),
                    ("AT+CPMS=\"MT\"", ok),
                ] {
                    let expected = format!("{command}\r\n");
                    let mut actual = vec![0; expected.len()];
                    master.read_exact(&mut actual).unwrap();
                    assert_eq!(actual, expected.as_bytes());
                    master.write_all(response.as_bytes()).unwrap();
                }
            }
            for (command, response) in [
                ("AT+CPMS?", current),
                ("AT+CPMS=\"SM\"", ok),
                ("AT+CMGD=7", "\r\n+CMS ERROR: 500\r\n"),
                ("AT+CPMS=\"MT\"", ok),
            ] {
                let expected = format!("{command}\r\n");
                let mut actual = vec![0; expected.len()];
                master.read_exact(&mut actual).unwrap();
                assert_eq!(actual, expected.as_bytes());
                master.write_all(response.as_bytes()).unwrap();
            }
            master.write_all(b"\r\n+CM").unwrap();
            std::thread::sleep(Duration::from_millis(10));
            master.write_all(b"TI: \"SM\",8\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(50));
        });
        let snapshot = port.sms_list().unwrap();
        let entries = crate::sms::received(&snapshot);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["text"], "ME message");
        assert_eq!(entries[1]["storage"], "SM");
        assert!(port.sms_list().is_err());
        assert!(
            port.sms_in_storage(crate::sms::Storage::SM, "AT+CMGD=7")
                .is_err()
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(1), notify.notified())
                .await
                .unwrap();
        });
        modem.join().unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn pty_preserves_grouped_commands_and_sms_prompt() {
        use std::os::fd::FromRawFd;
        let (mut master, mut slave) = (0, 0);
        let mut name = [0 as libc::c_char; 128];
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    name.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let path = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
            .to_str()
            .unwrap()
            .to_owned();
        let mut master = unsafe { std::fs::File::from_raw_fd(master) };
        let _slave = unsafe { std::fs::File::from_raw_fd(slave) };
        let mut port =
            UsbAtTransport::open(std::path::Path::new(&path), Duration::from_secs(10)).unwrap();
        let modem = std::thread::spawn(move || {
            fn read_until(file: &mut std::fs::File, end: u8) -> Vec<u8> {
                let mut data = Vec::new();
                loop {
                    let mut byte = [0];
                    file.read_exact(&mut byte).unwrap();
                    data.push(byte[0]);
                    if byte[0] == end {
                        return data;
                    }
                }
            }
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CGMM;+CSQ\r\n");
            master.write_all(b"\r\n+QSPN: \"TOKYO\"\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(20));
            master
                .write_all(b"RM520N-EU\r\n+CSQ: 20,99\r\nOK\r\n")
                .unwrap();
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CMGF=0\r\n");
            master.write_all(b"\r\n+CMS ERROR: 302\r\n").unwrap();
            // Rejected setup must not submit CMGS or a PDU.
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CMGF=0\r\n");
            master.write_all(b"\r\nOK\r\n").unwrap();
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CMGS=3\r\n");
            master.write_all(b"\r\n+CMS ERROR: 302\r\n").unwrap();
            // Rejected CMGS must not submit a PDU or retry the request.
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CMGF=0\r\n");
            master.write_all(b"\r\nOK\r\n").unwrap();
            assert_eq!(read_until(&mut master, b'\n'), b"AT+CMGS=3\r\n");
            std::thread::sleep(Duration::from_millis(20));
            master.write_all(b"> ").unwrap();
            assert_eq!(read_until(&mut master, 26), b"001122\x1a");
            master.write_all(b"\r\n+CMGS: 1\r\nOK\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(50));
        });
        let response = port.execute("AT+CGMM;+CSQ", None, None).unwrap();
        assert!(response.contains("+CSQ: 20,99"));
        let error = port
            .execute("AT+CMGF=0;+CMGS=3", Some("001122"), None)
            .unwrap_err();
        assert!(error.to_string().contains("SMS setup rejected (AT+CMGF=0)"));
        let error = port
            .execute("AT+CMGF=0;+CMGS=3", Some("001122"), None)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("before PDU submission (AT+CMGS=3)")
        );
        let response = port
            .execute("AT+CMGF=0;+CMGS=3", Some("001122"), None)
            .unwrap();
        assert!(response.contains("+CMGS: 1"));
        modem.join().unwrap();
    }
    #[tokio::test]
    async fn concurrent_readers_share_cache() {
        let at = At::start(true, vec![]).unwrap();
        let (a, b) = tokio::join!(at.fetch(DASHBOARD, true), at.fetch(DASHBOARD, true));
        assert_eq!(a.unwrap(), b.unwrap());
        assert_eq!(at.cache.lock().unwrap().len(), 1);
    }
    #[test]
    fn split_quoted_commands() {
        assert_eq!(
            split("AT+CGDCONT=1,\"IP\",\"a;b\";+CFUN=1"),
            ["AT+CGDCONT=1,\"IP\",\"a;b\"", "AT+CFUN=1"]
        );
    }
    #[test]
    fn final_line_only() {
        assert!(!terminal("+QSPN: \"TOKYO\""));
        assert!(terminal("\r\nOK\r\n"));
    }
}
