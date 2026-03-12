use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    str::FromStr,
};

use crate::backend::{query::QueryParams, JobState};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partitions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qos: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_pattern: Option<String>,
}

impl SavedFilters {
    pub fn from_params(params: &QueryParams) -> Self {
        Self {
            user: params.user.clone(),
            statuses: params.statuses.iter().map(|s| s.to_string()).collect(),
            partitions: params.partitions.clone(),
            qos: params.qos.clone(),
            nodes: params.nodes.clone(),
            name_pattern: params.name_pattern.clone(),
        }
    }

    pub fn apply_to(&self, params: &mut QueryParams) {
        params.user = self.user.clone();
        params.statuses = self
            .statuses
            .iter()
            .filter_map(|s| JobState::from_str(s).ok())
            .filter(|s| *s != JobState::Unknown)
            .collect();
        params.partitions = self.partitions.clone();
        params.qos = self.qos.clone();
        params.nodes = self.nodes.clone();
        params.name_pattern = self.name_pattern.clone();
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("sqwatch").join("filters.json")
}

pub fn load_filters() -> Option<SavedFilters> {
    let path = config_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_filters(params: &QueryParams) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let saved = SavedFilters::from_params(params);
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}
