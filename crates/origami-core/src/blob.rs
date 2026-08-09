//! Content-addressed blob store for raw RFC 822 message bodies.
//!
//! Bodies live as files under `<root>/<aa>/<bb>/<sha256>`; identical
//! bodies (cross-folder copies, Gmail labels) are stored once. This is
//! deliberately plain files — never mbox, never a custom format
//! (Thunderbird Mork lesson).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Error, Result};

#[derive(Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// In-memory-adjacent variant for tests: a fresh temp dir.
    #[cfg(test)]
    pub fn open_temp() -> Result<(Self, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        Ok((Self::open(dir.path())?, dir))
    }

    /// Store bytes; returns the hex SHA-256 content hash (idempotent).
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = hex_sha256(bytes);
        let path = self.path_for(&hash);
        if path.exists() {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write-then-rename: readers never see a partial blob. A unique
        // temporary path also makes cross-folder dedupe safe concurrently.
        let tmp = path.with_file_name(format!(".{hash}.{}.tmp", uuid::Uuid::now_v7()));
        std::fs::write(&tmp, bytes)?;
        if let Err(error) = std::fs::rename(&tmp, &path) {
            if path.exists() {
                let _ = std::fs::remove_file(&tmp);
                return Ok(hash);
            }
            return Err(error.into());
        }
        Ok(hash)
    }

    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash);
        std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Backend(format!("blob not found: {hash}"))
            } else {
                Error::Io(e)
            }
        })
    }

    pub fn contains(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    pub fn path_for(&self, hash: &str) -> PathBuf {
        let (aa, rest) = hash.split_at(2.min(hash.len()));
        let (bb, _) = rest.split_at(2.min(rest.len()));
        self.root.join(aa).join(bb).join(hash)
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let (store, _dir) = BlobStore::open_temp().unwrap();
        let hash = store.put(b"hello origami").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(store.contains(&hash));
        assert_eq!(store.get(&hash).unwrap(), b"hello origami");
    }

    #[test]
    fn put_is_idempotent_and_dedupes() {
        let (store, _dir) = BlobStore::open_temp().unwrap();
        let h1 = store.put(b"same bytes").unwrap();
        let h2 = store.put(b"same bytes").unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn missing_blob_is_a_clean_error() {
        let (store, _dir) = BlobStore::open_temp().unwrap();
        let err = store.get(&"0".repeat(64)).unwrap_err();
        assert!(err.to_string().contains("blob not found"));
    }
}
