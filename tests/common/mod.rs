use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct TemporaryRoot(PathBuf);
impl TemporaryRoot {
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "kiryl-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl std::ops::Deref for TemporaryRoot {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}
impl AsRef<Path> for TemporaryRoot {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}
impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
