use async_process::{Command, Output};
use color_eyre::Result;
use std::str::FromStr;

use super::Job;
use super::JobState;

/// Field separator embedded in the `squeue --format` string. A control
/// character (ASCII Unit Separator) is used instead of `|` so job names
/// that contain `|` cannot corrupt column parsing.
pub const FIELD_SEP: &str = "\u{1f}";

#[derive(Debug, Clone)]
pub struct QueryParams {
    pub user: Option<String>,
    pub statuses: Vec<JobState>,
    pub partitions: Vec<String>,
    pub qos: Vec<String>,
    pub name_pattern: Option<String>,
    pub nodes: Vec<String>,
    pub fmt: String,
    /// Sort fields in priority order: (field_code, ascending).
    pub ordering: Vec<(String, bool)>,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            user: None,
            statuses: Vec::new(),
            partitions: Vec::new(),
            qos: Vec::new(),
            name_pattern: None,
            nodes: Vec::new(),
            fmt: ["%i", "%j", "%u", "%T", "%M", "%N", "%C", "%m", "%P", "%q"].join(FIELD_SEP),
            ordering: vec![("i".to_string(), true)],
        }
    }
}

impl QueryParams {
    pub fn columns(&self) -> Vec<&str> {
        self.fmt.split(FIELD_SEP).collect()
    }

    pub fn is_valid_format(&self) -> bool {
        let cols = self.columns();
        !cols.is_empty() && cols.iter().all(|c| c.starts_with('%'))
    }

    pub fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // user filtering is done client-side via regex
        args.push("--all".into());

        if !self.statuses.is_empty() {
            let joined = self
                .statuses
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",");
            args.push("--states".into());
            args.push(joined);
        } else {
            // Include all job states by default (squeue only shows PENDING/RUNNING otherwise)
            args.push("--states".into());
            args.push("all".into());
        }

        if !self.partitions.is_empty() {
            args.push("--partition".into());
            args.push(self.partitions.join(","));
        }

        if !self.qos.is_empty() {
            args.push("--qos".into());
            args.push(self.qos.join(","));
        }

        if !self.nodes.is_empty() {
            args.push("--nodelist".into());
            args.push(self.nodes.join(","));
        }

        args.push("--format".into());
        args.push(self.fmt.clone());

        if !self.ordering.is_empty() {
            let sort_str = self
                .ordering
                .iter()
                .map(|(field, asc)| {
                    if *asc {
                        field.clone()
                    } else {
                        format!("-{}", field)
                    }
                })
                .collect::<Vec<String>>()
                .join(",");
            args.push("--sort".into());
            args.push(sort_str);
        }

        args.push("--noheader".into());
        args
    }
}

pub async fn fetch_jobs(params: &QueryParams) -> Result<Vec<Job>> {
    if !params.is_valid_format() {
        color_eyre::eyre::bail!("internal error: invalid squeue format string");
    }

    let args = params.build_args();
    let output = Command::new("squeue")
        .args(&args)
        .output()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("failed to run squeue: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        if detail.is_empty() {
            color_eyre::eyre::bail!("squeue exited with {}", output.status);
        }
        color_eyre::eyre::bail!("squeue: {}", detail);
    }

    decode_output(&output, &params.fmt)
}

fn decode_output(output: &Output, fmt: &str) -> Result<Vec<Job>> {
    let raw = String::from_utf8_lossy(&output.stdout);
    Ok(decode_squeue_output(&raw, fmt))
}

/// Parse `FIELD_SEP`-delimited `squeue` output into jobs. Split out from
/// [`fetch_jobs`] so parsing can be tested without spawning `squeue`.
pub fn decode_squeue_output(raw: &str, fmt: &str) -> Vec<Job> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let col_codes: Vec<&str> = fmt.split(FIELD_SEP).collect();
    if col_codes.is_empty() {
        return Vec::new();
    }

    let mut jobs = Vec::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(FIELD_SEP).collect();
        if fields.is_empty() || fields.len() < col_codes.len() / 2 {
            continue;
        }

        let mut job = Job::default();

        for (idx, cell) in fields.iter().enumerate() {
            if idx >= col_codes.len() {
                break;
            }

            let val = cell.trim().to_string();
            if val.is_empty() || val == "N/A" {
                continue;
            }

            match col_codes[idx] {
                "%i" | "%A" => job.job_id = val,
                "%j" => job.name = val,
                "%u" => job.user = val,
                "%T" => job.state = JobState::from_str(&val).unwrap_or(JobState::Unknown),
                "%M" => job.time = val,
                "%D" => job.num_nodes = val.parse().unwrap_or(0),
                "%N" => job.nodelist = Some(val),
                "%C" => job.num_cpus = val.parse().unwrap_or(0),
                "%m" => job.min_memory = val,
                "%P" => job.partition = val,
                "%q" => job.qos = val,
                "%a" => job.account = Some(val),
                "%Q" => job.priority = val.parse().ok(),
                "%Z" => job.work_dir = Some(val),
                "%V" => job.submit_time = Some(val),
                "%S" => job.start_time = Some(val),
                "%e" => job.end_time = Some(val),
                // squeue prints "None" when a job has no reason, which the
                // table shows as the same "-" placeholder as any other unset
                // cell. %R is deliberately not accepted here: it returns the
                // nodelist for anything that is not pending or failed.
                "%r" => job.reason = (val != "None").then_some(val),
                _ => {}
            }
        }

        jobs.push(job);
    }

    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::JobState;

    #[test]
    fn build_args_defaults_to_all_states() {
        let args = QueryParams::default().build_args();
        assert!(args.contains(&"--all".to_string()));
        assert!(args.contains(&"--noheader".to_string()));
        let i = args.iter().position(|a| a == "--states").unwrap();
        assert_eq!(args[i + 1], "all");
    }

    #[test]
    fn build_args_joins_states_and_partitions() {
        let p = QueryParams {
            statuses: vec![JobState::Pending, JobState::Running],
            partitions: vec!["gpu".into(), "cpu".into()],
            ..QueryParams::default()
        };
        let args = p.build_args();
        let si = args.iter().position(|a| a == "--states").unwrap();
        assert_eq!(args[si + 1], "PENDING,RUNNING");
        let pi = args.iter().position(|a| a == "--partition").unwrap();
        assert_eq!(args[pi + 1], "gpu,cpu");
    }

    #[test]
    fn build_args_prefixes_descending_sort_with_dash() {
        let p = QueryParams {
            ordering: vec![("P".into(), false), ("i".into(), true)],
            ..QueryParams::default()
        };
        let args = p.build_args();
        let si = args.iter().position(|a| a == "--sort").unwrap();
        assert_eq!(args[si + 1], "-P,i");
    }

    #[test]
    fn is_valid_format_requires_percent_prefixed_columns() {
        let mut p = QueryParams::default();
        assert!(p.is_valid_format());
        p.fmt = ["%i", "bad"].join(FIELD_SEP);
        assert!(!p.is_valid_format());
        p.fmt = String::new();
        assert!(!p.is_valid_format());
    }
}
