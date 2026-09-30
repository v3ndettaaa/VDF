//! Atomic file replacement — the discipline behind "an interrupted save
//! never corrupts the original" (MASTER_PLAN.md §12).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use vdf_core::{VdfError, VdfResult};

use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` atomically:
///
/// 1. write to a sibling temp file (same filesystem, so rename is atomic)
/// 2. flush, optionally `fsync`
/// 3. rename over the destination
///
/// Any interruption leaves either the old file or the new file at `path` —
/// never a partial write. Returns an error without touching `path` if the
/// initial write fails.
pub fn atomic_write(path: &Path, bytes: &[u8], fsync: bool) -> VdfResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            VdfError::Persist(format!("destination {path:?} has no parent directory"))
        })?;
    fs::create_dir_all(parent)?;

    let tmp: PathBuf = parent.join(format!(
        ".{}.vdf-tmp-{}-{}",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into()),
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));

    let write_result = (|| -> VdfResult<()> {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        if fsync {
            f.sync_all()?;
        }
        Ok(())
    })();

    match write_result {
        Ok(()) => {
            fs::rename(&tmp, path)?;
            Ok(())
        }
        Err(e) => {
            // best-effort cleanup; the destination was never touched
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vdf-persist-{tag}-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_and_replaces_content() {
        let dir = tmp_dir("write");
        let p = dir.join("a.bin");
        atomic_write(&p, b"first", false).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"first");
        atomic_write(&p, b"second", true).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"second");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = tmp_dir("temp");
        let p = dir.join("b.bin");
        atomic_write(&p, &[b'x'; 4096], true).unwrap();
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("vdf-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files must be renamed away: {leftovers:?}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failure_does_not_touch_destination() {
        let dir = tmp_dir("fail");
        let p = dir.join("c.bin");
        atomic_write(&p, b"original", false).unwrap();
        // writing into a *file* path used as a directory must fail...
        let impossible = dir.join("c.bin").join("sub").join("d.bin");
        // parent "c.bin/sub" cannot be created because c.bin is a file
        assert!(atomic_write(&impossible, b"nope", false).is_err());
        // ...and the original file is intact
        assert_eq!(fs::read(&p).unwrap(), b"original");
        fs::remove_dir_all(&dir).ok();
    }
}
