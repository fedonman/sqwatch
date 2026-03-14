use async_process::{Command, Output};
use color_eyre::Result;
use std::collections::HashMap;

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
pub struct JobDetail {
    pub stdout_file: Option<String>,
    pub stderr_file: Option<String>,
    pub command: Option<String>,
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
    })
}

/// Parse scontrol's space-separated `Key=Value` output into a map.
pub fn parse_scontrol_kv(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for token in text.split_whitespace() {
        if let Some(eq) = token.find('=') {
            out.insert(token[..eq].to_string(), token[eq + 1..].to_string());
        }
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
        Ok(o) => o,
        Err(_) => return vec!["normal".into(), "huge".into()],
    };

    let items: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    if items.is_empty() {
        vec!["normal".into(), "huge".into()]
    } else {
        items
    }
}
