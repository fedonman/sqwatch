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

pub async fn _run_squeue(args: Vec<String>) -> Result<String> {
    let out = run_cmd("squeue", args).await?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn _run_scontrol(job_id: &str) -> Result<String> {
    let args = vec!["show".into(), "job".into(), job_id.into()];
    let out = run_cmd("scontrol", args).await?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn cancel_jobs(job_ids: Vec<String>) -> Result<()> {
    if job_ids.is_empty() {
        return Ok(());
    }

    let batch_limit = 200;
    for batch in job_ids.chunks(batch_limit) {
        let _ = run_cmd("scancel", batch.to_vec()).await?;
    }

    Ok(())
}

pub async fn _update_job(job_id: &str, params: HashMap<String, String>) -> Result<()> {
    let mut args = vec!["update".to_string(), format!("JobId={}", job_id)];
    for (k, v) in params {
        args.push(format!("{}={}", k, v));
    }
    let _ = run_cmd("scontrol", args).await?;
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
