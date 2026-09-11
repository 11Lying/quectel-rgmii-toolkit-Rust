use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct Store {
    pub managed: bool,
    lock: Mutex<()>,
}
pub struct SavedError {
    pub committed: bool,
    pub error: anyhow::Error,
}
impl std::fmt::Debug for SavedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::fmt::Display for SavedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.committed {
            write!(f, "settings written but durability failed: ")?
        }
        self.error.fmt(f)
    }
}
impl std::error::Error for SavedError {}

impl Store {
    pub fn new(mock: bool) -> Self {
        let _ = mock;
        Self {
            managed: false,
            lock: Mutex::new(()),
        }
    }
    pub fn write(
        &self,
        path: &Path,
        data: &[u8],
        mode: u32,
    ) -> std::result::Result<(), SavedError> {
        self.update(path, mode, |_| Ok(data.to_vec()))
    }
    pub fn update(
        &self,
        path: &Path,
        mode: u32,
        prepare: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    ) -> std::result::Result<(), SavedError> {
        #[cfg(not(unix))]
        let _ = mode;
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let _process = if self.managed {
            Some(process_lock().map_err(|error| SavedError {
                committed: false,
                error,
            })?)
        } else {
            None
        };
        let old = match fs::read(path) {
            Ok(old) => old,
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(SavedError {
                    committed: false,
                    error: e.into(),
                });
            }
            _ => Vec::new(),
        };
        let data = prepare(&old).map_err(|error| SavedError {
            committed: false,
            error,
        })?;
        if old == data {
            return Ok(());
        }
        let mut committed = false;
        let result = (|| -> Result<()> {
            let parent = path.parent().context("missing settings directory")?;
            fs::create_dir_all(parent)?;
            let temp = parent.join(format!(
                ".simpleadmin-config-{}-{:x}",
                std::process::id(),
                rand::random::<u64>()
            ));
            let result = (|| -> Result<()> {
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(mode);
                }
                let mut file = options.open(&temp)?;
                file.write_all(&data)?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temp, path)?;
                committed = true;
                #[cfg(unix)]
                File::open(parent)?.sync_all()?;
                Ok(())
            })();
            let _ = fs::remove_file(&temp);
            result
        })();
        match result {
            Ok(()) => Ok(()),
            Err(error) => Err(SavedError { committed, error }),
        }
    }
}

#[cfg(unix)]
fn process_lock() -> Result<File> {
    use std::{os::fd::AsRawFd, os::unix::fs::OpenOptionsExt};
    for dir in ["/run", "/tmp"] {
        let name = std::ffi::CString::new(dir)?;
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // statfs initializes the output on success; only tmpfs is allowed for locks.
        if unsafe { libc::statfs(name.as_ptr(), stat.as_mut_ptr()) } != 0 {
            continue;
        }
        if unsafe { stat.assume_init() }.f_type as u64 != 0x01021994 {
            continue;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(Path::new(dir).join("simpleadmin-config.lock"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(file);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::WouldBlock || Instant::now() >= deadline {
                return Err(error.into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    bail!("configuration lock requires tmpfs at /run or /tmp")
}
#[cfg(not(unix))]
fn process_lock() -> Result<File> {
    bail!("device persistence requires Linux")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_identical_settings_and_preserves_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings");
        let store = Store::new(true);
        store.write(&path, b"test", 0o600).unwrap();
        let first = fs::metadata(&path).unwrap().modified().unwrap();
        store.write(&path, b"test", 0o600).unwrap();
        assert_eq!(first, fs::metadata(&path).unwrap().modified().unwrap());
        store.write(&path, b"updated", 0o600).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"updated");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
