use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, str::FromStr};

use crate::backend::{JobState, query::QueryParams};
use crate::views::fields::{JobField, OrderedField, SortDirection};
use crate::views::widget_selector::{CustomWidgetDef, VisibleWidgets};

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

fn sqwatch_config_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config")
        });
    base.join("sqwatch")
}

fn config_path() -> PathBuf {
    sqwatch_config_dir().join("filters.json")
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

// --- Column settings persistence ---

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SavedSort {
    field: String,
    direction: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedColumns {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SavedSort>,
}

impl SavedColumns {
    pub fn from_fields(active: &[JobField], sort_list: &[OrderedField]) -> Self {
        Self {
            columns: active.iter().map(|f| f.heading().to_string()).collect(),
            sort: sort_list
                .iter()
                .map(|of| SavedSort {
                    field: of.field.heading().to_string(),
                    direction: match of.direction {
                        SortDirection::Asc => "asc".to_string(),
                        SortDirection::Desc => "desc".to_string(),
                    },
                })
                .collect(),
        }
    }

    pub fn to_fields(&self) -> Option<(Vec<JobField>, Vec<OrderedField>)> {
        let all = JobField::enumerate();
        let lookup =
            |name: &str| -> Option<JobField> { all.iter().find(|f| f.heading() == name).copied() };

        let columns: Vec<JobField> = self.columns.iter().filter_map(|n| lookup(n)).collect();
        if columns.is_empty() {
            return None;
        }

        let sort_list: Vec<OrderedField> = self
            .sort
            .iter()
            .filter_map(|s| {
                let field = lookup(&s.field)?;
                let direction = match s.direction.as_str() {
                    "desc" => SortDirection::Desc,
                    _ => SortDirection::Asc,
                };
                Some(OrderedField { field, direction })
            })
            .collect();

        Some((columns, sort_list))
    }
}

fn columns_path() -> PathBuf {
    sqwatch_config_dir().join("columns.json")
}

pub fn load_columns() -> Option<(Vec<JobField>, Vec<OrderedField>)> {
    let path = columns_path();
    let data = fs::read_to_string(path).ok()?;
    let saved: SavedColumns = serde_json::from_str(&data).ok()?;
    saved.to_fields()
}

pub fn save_columns(active: &[JobField], sort_list: &[OrderedField]) -> Result<(), String> {
    let path = columns_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let saved = SavedColumns::from_fields(active, sort_list);
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

// --- Layout settings persistence ---

#[derive(Debug, Default, Serialize, Deserialize)]
struct SavedLayout {
    #[serde(default)]
    pub filters: bool,
    #[serde(default)]
    pub script: bool,
    #[serde(default)]
    pub stdout: bool,
    #[serde(default)]
    pub stderr: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_widgets: Vec<CustomWidgetDef>,
}

fn layout_path() -> PathBuf {
    sqwatch_config_dir().join("layout.json")
}

pub fn load_layout() -> Option<VisibleWidgets> {
    let path = layout_path();
    let data = fs::read_to_string(path).ok()?;
    let saved: SavedLayout = serde_json::from_str(&data).ok()?;
    Some(VisibleWidgets {
        filters: saved.filters,
        script: saved.script,
        stdout: saved.stdout,
        stderr: saved.stderr,
        custom: saved.custom_widgets,
    })
}

pub fn save_layout(widgets: &VisibleWidgets) -> Result<(), String> {
    let path = layout_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let saved = SavedLayout {
        filters: widgets.filters,
        script: widgets.script,
        stdout: widgets.stdout,
        stderr: widgets.stderr,
        custom_widgets: widgets.custom.clone(),
    };
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

// --- General settings persistence ---

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSettings {
    pub refresh_secs: u64,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self { refresh_secs: 3 }
    }
}

fn settings_path() -> PathBuf {
    sqwatch_config_dir().join("settings.json")
}

pub fn load_settings() -> Option<SavedSettings> {
    let path = settings_path();
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_settings(settings: &SavedSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}
