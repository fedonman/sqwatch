use async_process::{Command, Output};
use color_eyre::Result;
use std::collections::HashMap;

pub fn check_slurm_available() -> Result<()> {
    use std::process::Command as StdCommand;
    match StdCommand::new("squeue").arg("--version").output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => color_eyre::eyre::bail!(failure_message("squeue --version", &out)),
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

/// What a command printed when it exited non-zero, or its exit status when it
/// printed nothing.
fn failure_message(program: &str, out: &Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = stderr.trim();
    if detail.is_empty() {
        format!("{} exited with {}", program, out.status)
    } else {
        format!("{}: {}", program, detail)
    }
}

/// Run a command and collect its non-empty output lines. A non-zero exit is an
/// error rather than an empty list, so an unreachable controller cannot be
/// mistaken for a cluster that has none of whatever was asked for.
async fn collect_lines(program: &str, args: Vec<String>) -> Result<Vec<String>> {
    let out = run_cmd(program, args)
        .await
        .map_err(|e| color_eyre::eyre::eyre!("failed to run {}: {}", program, e))?;

    if !out.status.success() {
        color_eyre::eyre::bail!(failure_message(program, &out));
    }

    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
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

pub async fn list_partitions() -> Result<Vec<String>> {
    collect_lines("sinfo", vec!["-h".into(), "-o".into(), "%R".into()]).await
}

pub async fn list_nodes() -> Result<Vec<String>> {
    let mut nodes = collect_lines(
        "sinfo",
        vec!["-h".into(), "-N".into(), "-o".into(), "%N".into()],
    )
    .await?;
    nodes.sort();
    nodes.dedup();
    Ok(nodes)
}

pub async fn list_qos() -> Result<Vec<String>> {
    collect_lines(
        "sacctmgr",
        vec![
            "-n".into(),
            "show".into(),
            "qos".into(),
            "format=name".into(),
        ],
    )
    .await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(fut)
    }

    fn sh(script: &str) -> Result<Vec<String>> {
        block_on(collect_lines("sh", vec!["-c".into(), script.into()]))
    }

    #[test]
    fn collects_the_non_empty_output_lines() {
        let lines = sh("printf 'gpu\\n\\n  cpu  \\n'").unwrap();
        assert_eq!(lines, vec!["gpu", "cpu"]);
    }

    #[test]
    fn a_non_zero_exit_is_an_error_not_an_empty_list() {
        let err =
            sh("echo 'slurm_load_partitions: Unable to contact slurm controller' >&2; exit 1")
                .unwrap_err();
        assert!(
            err.to_string()
                .contains("Unable to contact slurm controller"),
            "error was {}",
            err
        );
    }

    #[test]
    fn a_silent_failure_reports_the_exit_status() {
        let err = sh("exit 2").unwrap_err();
        assert!(err.to_string().contains("exited with"), "error was {}", err);
    }

    #[test]
    fn a_cluster_with_nothing_to_list_is_still_ok() {
        assert_eq!(sh("true").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_missing_command_is_an_error() {
        let err = block_on(collect_lines("sqwatch-no-such-command", Vec::new())).unwrap_err();
        assert!(
            err.to_string().contains("failed to run"),
            "error was {}",
            err
        );
    }
}
