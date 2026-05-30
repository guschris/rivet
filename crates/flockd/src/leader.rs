use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::Path;

pub fn try_acquire(lock_path: &Path) -> Result<Option<File>, String> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| format!("cannot open lock file {}: {}", lock_path.display(), e))?;

    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };

    if ret == 0 {
        Ok(Some(file))
    } else {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::WouldBlock || err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(None)
        } else {
            Err(format!("flock error: {}", err))
        }
    }
}

pub fn release(_file: File) {
    // flock is released when the file descriptor is closed (file dropped)
    drop(_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquires_and_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("test.lock");

        let lock1 = try_acquire(&lock_path).unwrap();
        assert!(lock1.is_some());

        let lock2 = try_acquire(&lock_path).unwrap();
        assert!(lock2.is_none());

        release(lock1.unwrap());

        let lock3 = try_acquire(&lock_path).unwrap();
        assert!(lock3.is_some());
    }
}
