use async_process::{Command, Output};
use color_eyre::Result;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// A key in `scontrol` output: a word ending in `=`, at the start of the text
/// or after whitespace. Everything up to the next key is the value.
static SCONTROL_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)([A-Za-z][A-Za-z0-9_:]*)=").unwrap());

pub fn check_slurm_available() -> Result<()> {
    use std::process::Command as StdCommand;
    match StdCommand::new("squeue").arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            color_eyre::eyre::bail!(
                "SLURM tools not found.\n\
                 Please ensure squeue, sinfo, and other SLURM utilities \
                 are installed and available in your PATH."
            )
        }
        Err(e) => {
            color_eyre::eyre::bail!("Failed to run squeue: {}", e)
        }
    }
}

pub async fn run_cmd(program: &str, args: Vec<String>) -> Result<Output> {
    let out = Command::new(program).args(args).output().await?;
    Ok(out)
}

/// Parsed result of `scontrol show job <id> -o`.
#[derive(Clone)]
pub struct JobDetail {
    pub stdout_file: Option<String>,
    pub stderr_file: Option<String>,
    pub command: Option<String>,
    pub work_dir: Option<String>,
}

/// Run `scontrol show job <id> -o` and parse the key=value output.
pub fn scontrol_show_job(job_id: &str) -> Option<JobDetail> {
    use std::process::Command as StdCommand;

    let output = StdCommand::new("scontrol")
        .args(["show", "job", job_id, "-o"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let kv = parse_scontrol_kv(&raw);

    Some(JobDetail {
        stdout_file: kv.get("StdOut").cloned(),
        stderr_file: kv.get("StdErr").cloned(),
        command: kv.get("Command").cloned(),
        work_dir: kv.get("WorkDir").cloned(),
    })
}

/// Parse scontrol's space-separated `Key=Value` output into a map.
///
/// Values run from the `=` to the start of the next key rather than to the
/// next space, so paths containing spaces survive. All four values sqwatch
/// keeps are user-controlled paths.
pub fn parse_scontrol_kv(text: &str) -> HashMap<String, String> {
    let keys: Vec<_> = SCONTROL_KEY.captures_iter(text).collect();
    let mut out = HashMap::with_capacity(keys.len());

    for (i, cap) in keys.iter().enumerate() {
        let name = cap.get(1).expect("key group always matches");
        let value_start = cap.get(0).expect("whole match").end();
        let value_end = match keys.get(i + 1) {
            Some(next) => next.get(1).expect("key group always matches").start(),
            None => text.len(),
        };
        out.insert(
            name.as_str().to_string(),
            text[value_start..value_end].trim().to_string(),
        );
    }

    out
}

pub async fn cancel_jobs(job_ids: Vec<String>) -> Result<()> {
    if job_ids.is_empty() {
        return Ok(());
    }

    let batch_limit = 200;
    for batch in job_ids.chunks(batch_limit) {
        let output = run_cmd("scancel", batch.to_vec()).await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = if stderr.trim().is_empty() {
                format!("scancel exited with code {}", output.status)
            } else {
                format!("scancel: {}", stderr.trim())
            };
            color_eyre::eyre::bail!(msg);
        }
    }

    Ok(())
}

pub async fn list_partitions() -> Vec<String> {
    let out = match run_cmd("sinfo", vec!["-h".into(), "-o".into(), "%R".into()]).await {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

pub async fn list_nodes() -> Vec<String> {
    let out = match run_cmd(
        "sinfo",
        vec!["-h".into(), "-N".into(), "-o".into(), "%N".into()],
    )
    .await
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let mut nodes: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    nodes.sort();
    nodes.dedup();
    nodes
}

pub async fn list_qos() -> Vec<String> {
    let out = match run_cmd(
        "sacctmgr",
        vec![
            "-n".into(),
            "show".into(),
            "qos".into(),
            "format=name".into(),
        ],
    )
    .await
    {
        Ok(o) if o.status.success() => o,
        // No accounting DB / QoS on this cluster: show an empty list rather
        // than inventing site-specific names that don't exist here.
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One line of `scontrol show job -o`, with a space in three of the paths.
    const SPACED: &str = "JobId=42 JobName=my job UserId=me(1000) JobState=RUNNING \
StdOut=/home/me/my logs/out.txt StdErr=/home/me/my logs/err.txt \
WorkDir=/scratch/Smith Lab Command=/home/me/run.sh";

    #[test]
    fn keeps_paths_that_contain_spaces() {
        let kv = parse_scontrol_kv(SPACED);
        assert_eq!(kv.get("StdOut").unwrap(), "/home/me/my logs/out.txt");
        assert_eq!(kv.get("StdErr").unwrap(), "/home/me/my logs/err.txt");
        assert_eq!(kv.get("WorkDir").unwrap(), "/scratch/Smith Lab");
        assert_eq!(kv.get("JobName").unwrap(), "my job");
    }

    #[test]
    fn keeps_reading_after_a_value_with_a_space() {
        let kv = parse_scontrol_kv(SPACED);
        assert_eq!(kv.get("JobId").unwrap(), "42");
        assert_eq!(kv.get("JobState").unwrap(), "RUNNING");
        assert_eq!(kv.get("Command").unwrap(), "/home/me/run.sh");
    }

    #[test]
    fn parses_ordinary_output_unchanged() {
        let kv = parse_scontrol_kv("JobId=7 Partition=gpu NumCPUs=4 Priority=41823");
        assert_eq!(kv.get("JobId").unwrap(), "7");
        assert_eq!(kv.get("Partition").unwrap(), "gpu");
        assert_eq!(kv.get("NumCPUs").unwrap(), "4");
        assert_eq!(kv.get("Priority").unwrap(), "41823");
        assert_eq!(kv.len(), 4);
    }

    #[test]
    fn keeps_keys_that_carry_a_colon() {
        let kv = parse_scontrol_kv("TresPerNode=gres:gpu:2 MinMemoryNode=8G");
        assert_eq!(kv.get("TresPerNode").unwrap(), "gres:gpu:2");
        assert_eq!(kv.get("MinMemoryNode").unwrap(), "8G");
    }

    #[test]
    fn an_empty_value_stays_empty() {
        let kv = parse_scontrol_kv("Comment= JobId=1");
        assert_eq!(kv.get("Comment").unwrap(), "");
        assert_eq!(kv.get("JobId").unwrap(), "1");
    }

    #[test]
    fn a_multi_line_response_is_read_to_the_end() {
        let kv = parse_scontrol_kv("JobId=1 WorkDir=/a b\nJobId=2 WorkDir=/c d\n");
        assert_eq!(kv.get("JobId").unwrap(), "2");
        assert_eq!(kv.get("WorkDir").unwrap(), "/c d");
    }

    #[test]
    fn no_keys_gives_an_empty_map() {
        assert!(parse_scontrol_kv("").is_empty());
        assert!(parse_scontrol_kv("slurm_load_jobs error: Invalid job id").is_empty());
    }
}
