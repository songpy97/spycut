use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::domain::project::SourceIdentity;

const EDGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FingerprintError {
    #[error("source path cannot be canonicalized: {0}")]
    Canonicalize(#[source] std::io::Error),
    #[error("source metadata cannot be read: {0}")]
    Metadata(#[source] std::io::Error),
    #[error("source file cannot be opened: {0}")]
    Open(#[source] std::io::Error),
    #[error("source file cannot be read: {0}")]
    Read(#[source] std::io::Error),
    #[error("source modified time predates the Unix epoch")]
    InvalidModifiedTime,
}

pub fn fingerprint_source(path: &Path) -> Result<SourceIdentity, FingerprintError> {
    let canonical_path = path
        .canonicalize()
        .map_err(FingerprintError::Canonicalize)?;
    let metadata = canonical_path
        .metadata()
        .map_err(FingerprintError::Metadata)?;
    let size_bytes = metadata.len();
    let modified_unix_ms = metadata
        .modified()
        .map_err(FingerprintError::Metadata)?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| FingerprintError::InvalidModifiedTime)?
        .as_millis() as i64;
    let mut file = File::open(&canonical_path).map_err(FingerprintError::Open)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size_bytes.to_le_bytes());

    let first_len = usize::try_from(size_bytes.min(EDGE_BYTES as u64)).unwrap_or(EDGE_BYTES);
    let mut first = vec![0_u8; first_len];
    file.read_exact(&mut first)
        .map_err(FingerprintError::Read)?;
    hasher.update(&first);

    if size_bytes > EDGE_BYTES as u64 {
        file.seek(SeekFrom::End(-(EDGE_BYTES as i64)))
            .map_err(FingerprintError::Read)?;
        let mut last = vec![0_u8; EDGE_BYTES];
        file.read_exact(&mut last).map_err(FingerprintError::Read)?;
        hasher.update(&last);
    }

    Ok(SourceIdentity {
        canonical_path: path_to_string(canonical_path),
        size_bytes,
        modified_unix_ms,
        edge_hash_blake3: hasher.finalize().to_hex().to_string(),
    })
}

pub fn source_identity_matches(expected: &SourceIdentity, actual: &SourceIdentity) -> bool {
    expected.size_bytes == actual.size_bytes
        && expected.modified_unix_ms == actual.modified_unix_ms
        && expected.edge_hash_blake3 == actual.edge_hash_blake3
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{Seek, Write};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn fingerprints_sparse_file_larger_than_ten_gibibytes_by_edges() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("超大课程 source.mp4");
        let size = 11 * 1024 * 1024 * 1024_u64;
        let mut file = File::create(&path).unwrap();
        file.set_len(size).unwrap();
        file.write_all(b"SPYCUT-HEAD").unwrap();
        file.seek(SeekFrom::End(-11)).unwrap();
        file.write_all(b"SPYCUT-TAIL").unwrap();
        file.sync_all().unwrap();

        let identity = fingerprint_source(&path).unwrap();
        assert_eq!(identity.size_bytes, size);
        assert_eq!(identity.edge_hash_blake3.len(), 64);
        assert_eq!(
            identity.canonical_path,
            path_to_string(path.canonicalize().unwrap())
        );
    }
}
