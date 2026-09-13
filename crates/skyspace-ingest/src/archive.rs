//! Layer 1: every fetched body, gzipped, keyed by the SHA-256 of the
//! uncompressed bytes at `archive/<first two hex>/<sha256>.gz`. Bytes are
//! stored before anything interprets them and are never deleted.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

/// The content-addressed blob store. Synchronous and concrete: gzipping
/// 9 KB costs less than the `spawn_blocking` that would avoid it, and
/// there is one implementation.
#[derive(Debug, Clone)]
pub struct Archive {
    root: PathBuf,
}

/// Lower-case hex of a hash.
#[must_use]
pub fn hex(sha256: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in sha256 {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Read a 64-character hex hash back.
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in text.as_bytes().chunks(2).enumerate() {
        let digit = |b: u8| char::from(b).to_digit(16);
        out[i] = u8::try_from((digit(pair[0])? << 4) | digit(pair[1])?).ok()?;
    }
    Some(out)
}

/// SHA-256 of bytes, the archive key.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

impl Archive {
    /// An archive rooted at `root`. Nothing is created until the first `put`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where the blobs live.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, sha256: &[u8; 32]) -> PathBuf {
        let name = hex(sha256);
        self.root.join(&name[..2]).join(format!("{name}.gz"))
    }

    /// Store under the content hash and return the hash. Idempotent: an
    /// existing blob is left alone. Writes to a temp file and renames, so
    /// a crash cannot leave a truncated blob under a valid hash.
    ///
    /// # Errors
    /// `std::io::Error` when the directory or file cannot be written.
    pub fn put(&self, bytes: &[u8]) -> std::io::Result<[u8; 32]> {
        let key = sha256(bytes);
        let path = self.path_for(&key);
        if path.exists() {
            return Ok(key);
        }
        let dir = path
            .parent()
            .ok_or_else(|| std::io::Error::other("archive path has no parent"))?;
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(
            "{}.{}.{}.tmp",
            hex(&key),
            std::process::id(),
            unique_suffix()
        ));
        let result = write_gz(&tmp, bytes).and_then(|()| fs::rename(&tmp, &path));
        if result.is_err() {
            // Best effort: a leftover temp file is harmless but untidy.
            let _ = fs::remove_file(&tmp);
        }
        result?;
        Ok(key)
    }

    /// The uncompressed bytes under `sha256`.
    ///
    /// # Errors
    /// `std::io::Error` with `NotFound` when no blob has that hash, or
    /// whatever reading and gunzipping raised.
    pub fn get(&self, sha256: &[u8; 32]) -> std::io::Result<Vec<u8>> {
        let file = fs::File::open(self.path_for(sha256))?;
        let mut out = Vec::new();
        GzDecoder::new(file).read_to_end(&mut out)?;
        Ok(out)
    }

    /// Whether a blob with this hash is held.
    #[must_use]
    pub fn contains(&self, sha256: &[u8; 32]) -> bool {
        self.path_for(sha256).exists()
    }

    /// Create the root and write a probe file, for `skyspace doctor`.
    ///
    /// # Errors
    /// `std::io::Error` when the root is not writable.
    pub fn check_writable(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let probe = self.root.join(format!(".probe.{}", std::process::id()));
        fs::write(&probe, b"ok")?;
        fs::remove_file(&probe)
    }
}

fn write_gz(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file = fs::File::create(path)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(bytes)?;
    let file = encoder.finish()?;
    file.sync_all()
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::{Archive, from_hex, hex};

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("skyspace-archive-test-{}", super::unique_suffix()))
    }

    #[test]
    fn put_get_round_trip_and_dedupe() {
        let archive = Archive::new(temp_root());
        let key = archive.put(b"<html>hello</html>").unwrap();
        assert!(archive.contains(&key));
        assert_eq!(archive.get(&key).unwrap(), b"<html>hello</html>");
        // Same bytes, same key, no second file.
        assert_eq!(archive.put(b"<html>hello</html>").unwrap(), key);
        let dir = archive.root().join(&hex(&key)[..2]);
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
        assert!(archive.get(&[9u8; 32]).is_err());
        let _ = std::fs::remove_dir_all(archive.root());
    }

    #[test]
    fn hex_round_trips() {
        let key = [0xabu8; 32];
        assert_eq!(from_hex(&hex(&key)), Some(key));
        assert_eq!(from_hex("zz"), None);
    }
}
