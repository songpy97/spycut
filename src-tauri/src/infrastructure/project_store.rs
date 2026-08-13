use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::domain::project::{PROJECT_SCHEMA_VERSION, ProjectV1, SourceIdentity};

const PROJECT_SIDECAR_SUFFIX: &str = ".spycut.json";

#[derive(Clone, Debug)]
pub struct ProjectStore {
    projects_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectStoreError {
    #[error("project directory cannot be created: {0}")]
    CreateDirectory(#[source] std::io::Error),
    #[error("project file cannot be read: {0}")]
    Read(#[source] std::io::Error),
    #[error("project JSON is invalid: {0}")]
    Json(#[source] serde_json::Error),
    #[error("project schema version {0} is not supported")]
    UnsupportedSchema(u32),
    #[error("project sidecar belongs to a different source video")]
    SidecarSourceMismatch,
    #[error("source path cannot be used for a project sidecar")]
    InvalidSourcePath,
    #[error("project file cannot be written: {0}")]
    Write(#[source] std::io::Error),
    #[error("temporary project file cannot be persisted: {0}")]
    Persist(#[source] tempfile::PersistError),
}

impl ProjectStore {
    pub fn new(app_data_dir: impl Into<PathBuf>) -> Result<Self, ProjectStoreError> {
        let projects_dir = app_data_dir.into().join("projects");
        fs::create_dir_all(&projects_dir).map_err(ProjectStoreError::CreateDirectory)?;
        Ok(Self { projects_dir })
    }

    pub fn project_path(&self, project_id: &str) -> PathBuf {
        self.projects_dir.join(format!("{project_id}.json"))
    }

    pub fn sidecar_path(source_path: &Path) -> Result<PathBuf, ProjectStoreError> {
        let source_name = source_path
            .file_name()
            .ok_or(ProjectStoreError::InvalidSourcePath)?;
        let mut sidecar_name = source_name.to_os_string();
        sidecar_name.push(PROJECT_SIDECAR_SUFFIX);
        Ok(source_path.with_file_name(sidecar_name))
    }

    pub fn save(&self, project: &ProjectV1) -> Result<(), ProjectStoreError> {
        let source_path = Path::new(&project.source.canonical_path);
        let sidecar_path = Self::sidecar_path(source_path)?;
        Self::save_path(&sidecar_path, project)?;

        if let Err(error) = Self::save_path(&self.project_path(&project.project_id), project) {
            tracing::warn!("recent project cache refresh failed: {error}");
        }
        Ok(())
    }

    fn save_path(target: &Path, project: &ProjectV1) -> Result<(), ProjectStoreError> {
        let directory = target
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .ok_or(ProjectStoreError::InvalidSourcePath)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(directory).map_err(ProjectStoreError::Write)?;
        serde_json::to_writer_pretty(&mut temporary, project).map_err(ProjectStoreError::Json)?;
        temporary
            .write_all(b"\n")
            .map_err(ProjectStoreError::Write)?;
        temporary.flush().map_err(ProjectStoreError::Write)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(ProjectStoreError::Write)?;
        temporary
            .persist(target)
            .map_err(ProjectStoreError::Persist)?;
        Ok(())
    }

    pub fn load(&self, project_id: &str) -> Result<ProjectV1, ProjectStoreError> {
        self.load_path(&self.project_path(project_id))
    }

    pub fn find_matching_source(
        &self,
        source: &SourceIdentity,
    ) -> Result<Option<ProjectV1>, ProjectStoreError> {
        let sidecar_path = Self::sidecar_path(Path::new(&source.canonical_path))?;
        match File::open(&sidecar_path) {
            Ok(file) => {
                let project = Self::read_project(file)?;
                if !same_source_content(&project.source, source) {
                    return Err(ProjectStoreError::SidecarSourceMismatch);
                }
                return Ok(Some(project));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ProjectStoreError::Read(error)),
        }

        let entries = fs::read_dir(&self.projects_dir).map_err(ProjectStoreError::Read)?;
        let mut newest: Option<ProjectV1> = None;

        for entry in entries {
            let entry = entry.map_err(ProjectStoreError::Read)?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(project) = self.load_path(&entry.path()) else {
                continue;
            };
            if same_source_content(&project.source, source)
                && newest
                    .as_ref()
                    .is_none_or(|current| project.updated_unix_ms > current.updated_unix_ms)
            {
                newest = Some(project);
            }
        }

        Ok(newest)
    }

    pub fn load_most_recent(&self) -> Result<Option<ProjectV1>, ProjectStoreError> {
        let entries = fs::read_dir(&self.projects_dir).map_err(ProjectStoreError::Read)?;
        let mut newest: Option<ProjectV1> = None;
        for entry in entries {
            let entry = entry.map_err(ProjectStoreError::Read)?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(project) = self.load_path(&entry.path()) else {
                continue;
            };
            if newest
                .as_ref()
                .is_none_or(|current| project.updated_unix_ms > current.updated_unix_ms)
            {
                newest = Some(project);
            }
        }
        Ok(newest)
    }

    fn load_path(&self, path: &Path) -> Result<ProjectV1, ProjectStoreError> {
        let file = File::open(path).map_err(ProjectStoreError::Read)?;
        Self::read_project(file)
    }

    fn read_project(file: File) -> Result<ProjectV1, ProjectStoreError> {
        let project: ProjectV1 = serde_json::from_reader(file).map_err(ProjectStoreError::Json)?;
        if project.schema_version != PROJECT_SCHEMA_VERSION {
            return Err(ProjectStoreError::UnsupportedSchema(project.schema_version));
        }
        Ok(project)
    }
}

fn same_source_content(expected: &SourceIdentity, actual: &SourceIdentity) -> bool {
    expected.size_bytes == actual.size_bytes && expected.edge_hash_blake3 == actual.edge_hash_blake3
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        media::{FrameRate, MediaInfo, VideoCodec},
        project::SourceIdentity,
    };

    use super::*;

    fn project(source_path: &Path, updated_unix_ms: i64) -> ProjectV1 {
        let mut project = ProjectV1::new(
            SourceIdentity {
                canonical_path: source_path.to_string_lossy().into_owned(),
                size_bytes: 10,
                modified_unix_ms: 1,
                edge_hash_blake3: "edge".into(),
            },
            MediaInfo {
                duration_us: 1_000_000,
                container: "mp4".into(),
                video_codec: VideoCodec::H264,
                width: 320,
                height: 180,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                variable_frame_rate: false,
                video_stream_count: 1,
                audio_stream_count: 0,
                pixel_format: Some("yuv420p".into()),
                bit_depth: Some(8),
                video_bit_rate: Some(2_000_000),
                has_audio: false,
                audio_codec: None,
                audio_sample_rate: None,
                audio_channels: None,
                audio_bit_rate: None,
            },
        );
        project.updated_unix_ms = updated_unix_ms;
        project
    }

    #[test]
    fn loads_newest_valid_project_and_ignores_corrupt_json() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let older = project(&source_path, 10);
        let newer = project(&source_path, 20);
        store.save(&older).unwrap();
        store.save(&newer).unwrap();
        fs::write(store.projects_dir.join("corrupt.json"), b"not json").unwrap();
        assert_eq!(store.load_most_recent().unwrap(), Some(newer));
    }

    #[test]
    fn sidecar_path_preserves_the_complete_source_filename() {
        let source = Path::new("/tmp/课程 '01'.mp4");
        assert_eq!(
            ProjectStore::sidecar_path(source).unwrap(),
            Path::new("/tmp/课程 '01'.mp4.spycut.json")
        );
    }

    #[test]
    fn save_writes_the_source_sidecar_and_recent_project_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("课程 '01'.mp4");
        let project = project(&source_path, 10);

        store.save(&project).unwrap();

        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        assert_eq!(store.load_path(&sidecar).unwrap(), project);
        assert_eq!(store.load(&project.project_id).unwrap(), project);
    }

    #[test]
    fn recent_cache_failure_does_not_undo_a_committed_sidecar() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let project = project(&source_path, 10);
        fs::create_dir(store.project_path(&project.project_id)).unwrap();

        store.save(&project).unwrap();

        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        assert_eq!(store.load_path(&sidecar).unwrap(), project);
    }

    #[test]
    fn sidecar_is_preferred_over_an_older_recent_project_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let cached = project(&source_path, 10);
        store.save(&cached).unwrap();

        let mut sidecar_project = cached.clone();
        sidecar_project.last_playhead_us = 42;
        sidecar_project.updated_unix_ms = 20;
        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        fs::write(
            sidecar,
            serde_json::to_vec_pretty(&sidecar_project).unwrap(),
        )
        .unwrap();

        assert_eq!(
            store.find_matching_source(&cached.source).unwrap(),
            Some(sidecar_project)
        );
    }

    #[test]
    fn missing_sidecar_falls_back_to_the_legacy_recent_project_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let cached = project(&source_path, 10);
        store.save(&cached).unwrap();
        fs::remove_file(ProjectStore::sidecar_path(&source_path).unwrap()).unwrap();

        assert_eq!(
            store.find_matching_source(&cached.source).unwrap(),
            Some(cached)
        );
    }

    #[test]
    fn corrupt_sidecar_is_not_hidden_by_the_recent_project_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let cached = project(&source_path, 10);
        store.save(&cached).unwrap();
        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        fs::write(&sidecar, b"not json").unwrap();

        assert!(matches!(
            store.find_matching_source(&cached.source),
            Err(ProjectStoreError::Json(_))
        ));
        assert_eq!(fs::read(sidecar).unwrap(), b"not json");
    }

    #[test]
    fn unsupported_sidecar_schema_is_rejected_without_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let cached = project(&source_path, 10);
        store.save(&cached).unwrap();
        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        let mut unsupported = cached.clone();
        unsupported.schema_version = PROJECT_SCHEMA_VERSION + 1;
        let bytes = serde_json::to_vec_pretty(&unsupported).unwrap();
        fs::write(&sidecar, &bytes).unwrap();

        assert!(matches!(
            store.find_matching_source(&cached.source),
            Err(ProjectStoreError::UnsupportedSchema(_))
        ));
        assert_eq!(fs::read(sidecar).unwrap(), bytes);
    }

    #[test]
    fn sidecar_for_different_source_is_rejected_without_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let cached = project(&source_path, 10);
        store.save(&cached).unwrap();
        let sidecar = ProjectStore::sidecar_path(&source_path).unwrap();
        let original = fs::read(&sidecar).unwrap();
        let mut current_source = cached.source.clone();
        current_source.edge_hash_blake3 = "different".into();

        assert!(matches!(
            store.find_matching_source(&current_source),
            Err(ProjectStoreError::SidecarSourceMismatch)
        ));
        assert_eq!(fs::read(sidecar).unwrap(), original);
    }
}
