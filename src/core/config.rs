use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

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

/// Read and parse a config file. A file that is not there is `Ok(None)`; one
/// that cannot be read or does not parse is an error, so a damaged config is
/// never mistaken for a first run and quietly replaced with defaults.
fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Failed to read {}: {}", path.display(), e)),
    };

    serde_json::from_str(&data)
        .map(Some)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Write a config file by filling a temp file in the same directory and
/// renaming it over the target. An interrupted save leaves the previous file
/// whole rather than truncated in place.
fn write_atomically(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let tmp = parent.join(format!(".{}.{}.tmp", name, std::process::id()));

    let filled = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    })();

    if let Err(e) = filled {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to write {}: {}", path.display(), e));
    }

    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Failed to replace {}: {}", path.display(), e)
    })
}

fn config_path() -> PathBuf {
    sqwatch_config_dir().join("filters.json")
}

pub fn load_filters() -> Result<Option<SavedFilters>, String> {
    load_json(&config_path())
}

pub fn save_filters(params: &QueryParams) -> Result<(), String> {
    let saved = SavedFilters::from_params(params);
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    write_atomically(&config_path(), &json)
}

// --- Column settings persistence ---

/// The visible columns in order, paired with the sort keys applied to them.
pub type ColumnLayout = (Vec<JobField>, Vec<OrderedField>);

/// A column as `columns.json` stores it: the `JobField` variant name. Any other
/// string is kept as it is, so one entry that matches nothing does not fail the
/// whole file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedField {
    Known(JobField),
    Other(String),
}

impl SavedField {
    fn resolve(&self) -> Result<JobField, &str> {
        match self {
            SavedField::Known(field) => Ok(*field),
            // Older files store the heading instead. These are the headings
            // that differ from their variant name; the rest match it.
            SavedField::Other(name) => match name.as_str() {
                "ID" => Ok(JobField::Id),
                "Submit" => Ok(JobField::SubmitTime),
                "Start" => Ok(JobField::StartTime),
                "End" => Ok(JobField::EndTime),
                "Reason" => Ok(JobField::PendReason),
                _ => Err(name),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSort {
    field: SavedField,
    direction: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SavedColumns {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<SavedField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SavedSort>,
}

impl SavedColumns {
    pub fn from_fields(active: &[JobField], sort_list: &[OrderedField]) -> Self {
        Self {
            columns: active.iter().copied().map(SavedField::Known).collect(),
            sort: sort_list
                .iter()
                .map(|of| SavedSort {
                    field: SavedField::Known(of.field),
                    direction: match of.direction {
                        SortDirection::Asc => "asc".to_string(),
                        SortDirection::Desc => "desc".to_string(),
                    },
                })
                .collect(),
        }
    }

    /// The saved layout, or `None` when no saved column is known, along with
    /// the saved names that match no column and were left out.
    pub fn to_fields(&self) -> (Option<ColumnLayout>, Vec<String>) {
        let mut unknown: Vec<String> = Vec::new();
        let mut resolve = |saved: &SavedField| match saved.resolve() {
            Ok(field) => Some(field),
            Err(name) => {
                if !unknown.iter().any(|n| n == name) {
                    unknown.push(name.to_string());
                }
                None
            }
        };

        let columns: Vec<JobField> = self.columns.iter().filter_map(&mut resolve).collect();

        let sort_list: Vec<OrderedField> = self
            .sort
            .iter()
            .filter_map(|s| {
                let field = resolve(&s.field)?;
                let direction = match s.direction.as_str() {
                    "desc" => SortDirection::Desc,
                    _ => SortDirection::Asc,
                };
                Some(OrderedField { field, direction })
            })
            .collect();

        let layout = (!columns.is_empty()).then_some((columns, sort_list));
        (layout, unknown)
    }
}

fn columns_path() -> PathBuf {
    sqwatch_config_dir().join("columns.json")
}

/// Load the saved column layout, with the saved names that match no column.
pub fn load_columns() -> Result<(Option<ColumnLayout>, Vec<String>), String> {
    Ok(load_json::<SavedColumns>(&columns_path())?
        .map(|saved| saved.to_fields())
        .unwrap_or_default())
}

pub fn save_columns(active: &[JobField], sort_list: &[OrderedField]) -> Result<(), String> {
    let saved = SavedColumns::from_fields(active, sort_list);
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    write_atomically(&columns_path(), &json)
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

pub fn load_layout() -> Result<Option<VisibleWidgets>, String> {
    Ok(
        load_json::<SavedLayout>(&layout_path())?.map(|saved| VisibleWidgets {
            filters: saved.filters,
            script: saved.script,
            stdout: saved.stdout,
            stderr: saved.stderr,
            custom: saved.custom_widgets,
        }),
    )
}

pub fn save_layout(widgets: &VisibleWidgets) -> Result<(), String> {
    let saved = SavedLayout {
        filters: widgets.filters,
        script: widgets.script,
        stdout: widgets.stdout,
        stderr: widgets.stderr,
        custom_widgets: widgets.custom.clone(),
    };
    let json =
        serde_json::to_string_pretty(&saved).map_err(|e| format!("Failed to serialize: {}", e))?;
    write_atomically(&layout_path(), &json)
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

pub fn load_settings() -> Result<Option<SavedSettings>, String> {
    load_json(&settings_path())
}

pub fn save_settings(settings: &SavedSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    write_atomically(&settings_path(), &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A directory under the temp dir that removes itself when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("sqwatch-cfg-{}-{}", std::process::id(), n));
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn entries(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = TempDir::new();
        let loaded: Option<SavedSettings> = load_json(&dir.file("settings.json")).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn a_truncated_file_is_an_error_rather_than_a_first_run() {
        let dir = TempDir::new();
        let path = dir.file("filters.json");
        fs::write(&path, r#"{"user": "alice", TRUNCATED"#).unwrap();

        let err = load_json::<SavedFilters>(&path).unwrap_err();
        assert!(err.contains("filters.json"), "error was {}", err);
    }

    #[test]
    fn a_file_that_is_not_json_is_an_error() {
        let dir = TempDir::new();
        let path = dir.file("settings.json");
        fs::write(&path, "not json at all").unwrap();

        assert!(load_json::<SavedSettings>(&path).is_err());
    }

    #[test]
    fn a_valid_file_loads() {
        let dir = TempDir::new();
        let path = dir.file("settings.json");
        fs::write(&path, r#"{"refresh_secs": 7}"#).unwrap();

        let loaded: SavedSettings = load_json(&path).unwrap().unwrap();
        assert_eq!(loaded.refresh_secs, 7);
    }

    #[test]
    fn writing_replaces_the_previous_contents() {
        let dir = TempDir::new();
        let path = dir.file("settings.json");

        write_atomically(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        write_atomically(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn writing_leaves_no_temp_file_behind() {
        let dir = TempDir::new();
        write_atomically(&dir.file("settings.json"), "{}").unwrap();
        assert_eq!(dir.entries(), vec!["settings.json"]);
    }

    #[test]
    fn writing_creates_the_config_directory() {
        let dir = TempDir::new();
        let path = dir.file("nested").join("settings.json");

        write_atomically(&path, "{}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn a_failed_write_leaves_the_original_alone() {
        let dir = TempDir::new();
        let path = dir.file("settings.json");
        write_atomically(&path, r#"{"refresh_secs": 7}"#).unwrap();

        // A directory where the temp file wants to go: the fill step fails and
        // the rename never runs.
        let blocker = dir
            .0
            .join(format!(".settings.json.{}.tmp", std::process::id()));
        fs::create_dir(&blocker).unwrap();

        assert!(write_atomically(&path, "replacement").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"refresh_secs": 7}"#);
    }

    fn parse_columns(json: &str) -> (Option<ColumnLayout>, Vec<String>) {
        serde_json::from_str::<SavedColumns>(json)
            .unwrap()
            .to_fields()
    }

    #[test]
    fn columns_are_saved_by_variant_name() {
        let columns = JobField::enumerate();
        let sort = vec![OrderedField {
            field: JobField::PendReason,
            direction: SortDirection::Desc,
        }];

        let json = serde_json::to_string(&SavedColumns::from_fields(&columns, &sort)).unwrap();
        assert!(json.contains(r#""PendReason""#), "json was {}", json);

        let (layout, unknown) = parse_columns(&json);
        assert_eq!(layout, Some((columns, sort)));
        assert!(unknown.is_empty());
    }

    #[test]
    fn a_file_saved_with_headings_still_loads() {
        let (layout, unknown) = parse_columns(
            r#"{"columns": ["ID", "Name", "Submit", "Start", "End", "Reason"],
                "sort": [{"field": "Reason", "direction": "desc"}]}"#,
        );

        let (columns, sort) = layout.unwrap();
        assert_eq!(
            columns,
            vec![
                JobField::Id,
                JobField::Name,
                JobField::SubmitTime,
                JobField::StartTime,
                JobField::EndTime,
                JobField::PendReason,
            ]
        );
        assert_eq!(
            sort,
            vec![OrderedField {
                field: JobField::PendReason,
                direction: SortDirection::Desc,
            }]
        );
        assert!(unknown.is_empty());
    }

    #[test]
    fn unknown_columns_are_reported_and_the_rest_kept() {
        let (layout, unknown) = parse_columns(
            r#"{"columns": ["Id", "Name", "Nodelist", "Pending Reason"],
                "sort": [{"field": "Nodelist", "direction": "desc"},
                         {"field": "Name", "direction": "asc"}]}"#,
        );

        let (columns, sort) = layout.unwrap();
        assert_eq!(columns, vec![JobField::Id, JobField::Name]);
        assert_eq!(
            sort,
            vec![OrderedField {
                field: JobField::Name,
                direction: SortDirection::Asc,
            }]
        );
        assert_eq!(unknown, vec!["Nodelist", "Pending Reason"]);
    }

    #[test]
    fn no_known_column_falls_back_but_still_reports() {
        let (layout, unknown) = parse_columns(r#"{"columns": ["Nodelist"]}"#);
        assert!(layout.is_none());
        assert_eq!(unknown, vec!["Nodelist"]);
    }
}
