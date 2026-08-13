use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{interval::DeleteInterval, media::MediaInfo, time::Micros};

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIdentity {
    pub canonical_path: String,
    pub size_bytes: u64,
    pub modified_unix_ms: i64,
    pub edge_hash_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectV1 {
    pub schema_version: u32,
    pub project_id: String,
    pub source: SourceIdentity,
    pub media: MediaInfo,
    pub delete_intervals: Vec<DeleteInterval>,
    pub next_interval_id: u64,
    pub last_playhead_us: Micros,
    pub reviewed_interval_ids: Vec<u64>,
    pub created_unix_ms: i64,
    pub updated_unix_ms: i64,
}

impl ProjectV1 {
    pub fn new(source: SourceIdentity, media: MediaInfo) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            project_id: uuid::Uuid::new_v4().to_string(),
            source,
            media,
            delete_intervals: Vec::new(),
            next_interval_id: 1,
            last_playhead_us: 0,
            reviewed_interval_ids: Vec::new(),
            created_unix_ms: now,
            updated_unix_ms: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_unix_ms = Utc::now().timestamp_millis();
    }
}
