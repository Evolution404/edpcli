//! Private same-directory candidates and explicit publication policies.
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn private_directory(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => return Ok(()),
        Ok(_) => {
            return Err(io::Error::other(
                "private directory is not a real directory",
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        private_directory(parent)?;
    }
    let builder = fs::DirBuilder::new();
    #[cfg(unix)]
    let mut builder = builder;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {
            crate::platform::protect_private_path(path)?;
            #[cfg(unix)]
            crate::platform::own_invoking_user_file(&File::open(path)?)?;
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                crate::platform::sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(path)?;
            if meta.is_dir() && !meta.file_type().is_symlink() {
                Ok(())
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

pub(crate) struct AtomicFile {
    pub(crate) file: File,
    candidate: PathBuf,
    target: PathBuf,
}
impl AtomicFile {
    pub(crate) fn new(target: &Path) -> io::Result<Self> {
        let directory = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        private_directory(directory)?;
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(io::Error::other)?;
        let candidate = directory.join(format!(
            ".edpcli-{}.tmp",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&candidate)?;
        let result = Self {
            file,
            candidate,
            target: target.to_owned(),
        };
        crate::platform::protect_private_path(&result.candidate)?;
        crate::platform::own_invoking_user_file(&result.file)?;
        Ok(result)
    }
    pub(crate) fn publish(self, replace: bool) -> io::Result<()> {
        self.file.sync_all()?;
        #[cfg(unix)]
        {
            if replace {
                fs::rename(&self.candidate, &self.target)?;
            } else {
                fs::hard_link(&self.candidate, &self.target)?;
                fs::remove_file(&self.candidate)?;
            }
        }
        #[cfg(windows)]
        crate::platform::publish_file(&self.candidate, &self.target, replace)?;
        let directory = self
            .target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        crate::platform::sync_directory(directory).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "published {} but directory durability failed: {error}",
                    self.target.display()
                ),
            )
        })?;
        Ok(())
    }
}
impl Drop for AtomicFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.candidate);
    }
}
pub(crate) fn write(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    use std::io::Write;
    let mut candidate = AtomicFile::new(path)?;
    candidate.file.write_all(bytes)?;
    candidate.publish(replace)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let mut nonce = [0; 8];
        getrandom::fill(&mut nonce).unwrap();
        std::env::temp_dir().join(format!("edpcli-atomic-{:x}", u64::from_ne_bytes(nonce)))
    }
    #[test]
    fn failure_preserves_old_and_exclusive_publish_is_immutable() {
        let root = root();
        let path = root.join("record.json");
        write(&path, b"old", false).unwrap();
        assert!(write(&path, b"new", false).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        {
            let candidate = AtomicFile::new(&path).unwrap();
            assert!(candidate.candidate.exists());
        }
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        write(&path, b"new", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_publish_has_one_winner() {
        let root = root();
        private_directory(&root).unwrap();
        let path = root.join("same.json");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .map(|bytes| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut f = AtomicFile::new(&path).unwrap();
                    use std::io::Write;
                    f.file.write_all(bytes).unwrap();
                    barrier.wait();
                    f.publish(false).is_ok()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|w| usize::from(w.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
