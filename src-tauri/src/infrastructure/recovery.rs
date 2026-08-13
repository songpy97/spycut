use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct RecoveryStore {
    jobs_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryRecord {
    job_id: String,
    partial_path: PathBuf,
    filter_script_path: PathBuf,
    destination_path: PathBuf,
    created_unix_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverableExport {
    pub job_id: String,
    pub destination_path: String,
    pub partial_path: Option<String>,
    pub reveal_path: String,
    pub partial_size_bytes: u64,
    pub created_unix_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("recovery directory cannot be created: {0}")]
    CreateDirectory(#[source] std::io::Error),
    #[error("invalid export job id")]
    InvalidJobId,
    #[error("recovery record contains paths not owned by this export job")]
    UnsafePaths,
    #[error("recovery record cannot be read: {0}")]
    Read(#[source] std::io::Error),
    #[error("recovery record JSON is invalid: {0}")]
    Json(#[source] serde_json::Error),
    #[error("recovery record cannot be written: {0}")]
    Write(#[source] std::io::Error),
    #[error("recovery record cannot be persisted: {0}")]
    Persist(#[source] tempfile::PersistError),
}

impl RecoveryStore {
    pub fn new(app_data_dir: impl Into<PathBuf>) -> Result<Self, RecoveryError> {
        let jobs_dir = app_data_dir.into().join("jobs");
        fs::create_dir_all(&jobs_dir).map_err(RecoveryError::CreateDirectory)?;
        Ok(Self { jobs_dir })
    }

    pub fn record(
        &self,
        job_id: &str,
        partial_path: PathBuf,
        filter_script_path: PathBuf,
        destination_path: PathBuf,
    ) -> Result<(), RecoveryError> {
        validate_job_id(job_id)?;
        let record = RecoveryRecord {
            job_id: job_id.into(),
            partial_path,
            filter_script_path,
            destination_path,
            created_unix_ms: Utc::now().timestamp_millis(),
        };
        validate_owned_paths(&record)?;

        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.jobs_dir).map_err(RecoveryError::Write)?;
        serde_json::to_writer_pretty(&mut temporary, &record).map_err(RecoveryError::Json)?;
        temporary.write_all(b"\n").map_err(RecoveryError::Write)?;
        temporary.flush().map_err(RecoveryError::Write)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(RecoveryError::Write)?;
        temporary
            .persist(self.record_path(job_id)?)
            .map_err(RecoveryError::Persist)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<RecoverableExport>, RecoveryError> {
        let entries = fs::read_dir(&self.jobs_dir).map_err(RecoveryError::Read)?;
        let mut recoverable = Vec::new();
        for entry in entries {
            let entry = entry.map_err(RecoveryError::Read)?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(record) = self.load_path(&entry.path()) else {
                continue;
            };
            if validate_job_id(&record.job_id).is_err() || validate_owned_paths(&record).is_err() {
                continue;
            }
            let partial_exists = record.partial_path.is_file();
            let filter_exists = record.filter_script_path.is_file();
            if !partial_exists && !filter_exists {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            let reveal_path = if partial_exists {
                &record.partial_path
            } else {
                &record.filter_script_path
            };
            recoverable.push(RecoverableExport {
                job_id: record.job_id,
                destination_path: path_string(&record.destination_path),
                partial_path: partial_exists.then(|| path_string(&record.partial_path)),
                reveal_path: path_string(reveal_path),
                partial_size_bytes: record
                    .partial_path
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
                created_unix_ms: record.created_unix_ms,
            });
        }
        recoverable.sort_by_key(|item| std::cmp::Reverse(item.created_unix_ms));
        Ok(recoverable)
    }

    pub fn cleanup(&self, job_id: &str) -> Result<(), RecoveryError> {
        let path = self.record_path(job_id)?;
        let record = self.load_path(&path)?;
        validate_owned_paths(&record)?;
        remove_owned_file(&record.partial_path)?;
        remove_owned_file(&record.filter_script_path)?;
        self.clear(job_id)
    }

    pub fn clear(&self, job_id: &str) -> Result<(), RecoveryError> {
        let path = self.record_path(job_id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RecoveryError::Write(error)),
        }
    }

    fn record_path(&self, job_id: &str) -> Result<PathBuf, RecoveryError> {
        validate_job_id(job_id)?;
        Ok(self.jobs_dir.join(format!("{job_id}.json")))
    }

    fn load_path(&self, path: &Path) -> Result<RecoveryRecord, RecoveryError> {
        let file = File::open(path).map_err(RecoveryError::Read)?;
        serde_json::from_reader(file).map_err(RecoveryError::Json)
    }
}

fn validate_job_id(job_id: &str) -> Result<(), RecoveryError> {
    let parsed = uuid::Uuid::parse_str(job_id).map_err(|_| RecoveryError::InvalidJobId)?;
    if parsed.hyphenated().to_string() != job_id {
        return Err(RecoveryError::InvalidJobId);
    }
    Ok(())
}

fn validate_owned_paths(record: &RecoveryRecord) -> Result<(), RecoveryError> {
    let parent = record
        .destination_path
        .parent()
        .ok_or(RecoveryError::UnsafePaths)?;
    let stem = record
        .destination_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("spycut-export");
    let expected_partial = parent.join(format!(".{stem}.spycut-{}.partial.mp4", record.job_id));
    let expected_filter = parent.join(format!(".{stem}.spycut-{}.filter.txt", record.job_id));
    if record.partial_path != expected_partial || record.filter_script_path != expected_filter {
        return Err(RecoveryError::UnsafePaths);
    }
    Ok(())
}

fn remove_owned_file(path: &Path) -> Result<(), RecoveryError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RecoveryError::Write(error)),
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn records_lists_and_cleans_only_owned_work_files() {
        let directory = tempdir().unwrap();
        let store = RecoveryStore::new(directory.path()).unwrap();
        let job_id = uuid::Uuid::new_v4().to_string();
        let destination = directory.path().join("课程 output.mp4");
        let partial = directory
            .path()
            .join(format!(".课程 output.spycut-{job_id}.partial.mp4"));
        let filter = directory
            .path()
            .join(format!(".课程 output.spycut-{job_id}.filter.txt"));
        fs::write(&partial, b"partial").unwrap();
        fs::write(&filter, b"filter").unwrap();
        store
            .record(
                &job_id,
                partial.clone(),
                filter.clone(),
                destination.clone(),
            )
            .unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].partial_size_bytes, 7);
        assert_eq!(listed[0].destination_path, path_string(&destination));

        store.cleanup(&job_id).unwrap();
        assert!(!partial.exists());
        assert!(!filter.exists());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn refuses_job_ids_and_paths_not_owned_by_spycut() {
        let directory = tempdir().unwrap();
        let store = RecoveryStore::new(directory.path()).unwrap();
        let destination = directory.path().join("output.mp4");
        assert!(matches!(
            store.record(
                "../escape",
                directory.path().join("partial.mp4"),
                directory.path().join("filter.txt"),
                destination.clone(),
            ),
            Err(RecoveryError::InvalidJobId)
        ));

        let job_id = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            store.record(
                &job_id,
                directory.path().join("unrelated.mp4"),
                directory.path().join("unrelated.txt"),
                destination,
            ),
            Err(RecoveryError::UnsafePaths)
        ));
    }
}
