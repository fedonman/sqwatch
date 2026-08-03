pub mod commands;
pub mod query;

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
    NodeFail,
    Preempted,
    BootFail,
    Suspended,
    OutOfMemory,
    Unknown,
}

impl JobState {
    pub fn all_known() -> Vec<JobState> {
        vec![
            JobState::Pending,
            JobState::Running,
            JobState::Completed,
            JobState::Failed,
            JobState::Cancelled,
            JobState::Timeout,
            JobState::NodeFail,
            JobState::Preempted,
            JobState::BootFail,
            JobState::Suspended,
            JobState::OutOfMemory,
        ]
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            JobState::Pending => "PENDING",
            JobState::Running => "RUNNING",
            JobState::Completed => "COMPLETED",
            JobState::Failed => "FAILED",
            JobState::Cancelled => "CANCELLED",
            JobState::Timeout => "TIMEOUT",
            JobState::NodeFail => "NODE_FAIL",
            JobState::Preempted => "PREEMPTED",
            JobState::BootFail => "BOOT_FAIL",
            JobState::Suspended => "SUSPENDED",
            JobState::OutOfMemory => "OUT_OF_MEMORY",
            JobState::Unknown => "OTHER",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for JobState {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.to_uppercase().as_str() {
            "PENDING" | "PD" => Ok(JobState::Pending),
            "RUNNING" | "R" => Ok(JobState::Running),
            "COMPLETED" | "CD" | "COMPLETING" | "CG" => Ok(JobState::Completed),
            "FAILED" | "F" => Ok(JobState::Failed),
            "CANCELLED" | "CA" => Ok(JobState::Cancelled),
            "TIMEOUT" | "TO" => Ok(JobState::Timeout),
            "NODE_FAIL" | "NF" => Ok(JobState::NodeFail),
            "PREEMPTED" | "PR" => Ok(JobState::Preempted),
            "BOOT_FAIL" | "BF" => Ok(JobState::BootFail),
            "SUSPENDED" | "S" => Ok(JobState::Suspended),
            "OUT_OF_MEMORY" | "OOM" => Ok(JobState::OutOfMemory),
            _ => Ok(JobState::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub job_id: String,
    pub name: String,
    pub user: String,
    pub state: JobState,
    pub time: String,
    pub num_nodes: u32,
    pub nodelist: Option<String>,
    pub num_cpus: u32,
    pub min_memory: String,
    pub partition: String,
    pub qos: String,
    pub account: Option<String>,
    pub priority: Option<u32>,
    pub work_dir: Option<String>,
    pub submit_time: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub reason: Option<String>,
}

impl Default for Job {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            name: String::new(),
            user: String::new(),
            state: JobState::Unknown,
            time: String::new(),
            num_nodes: 0,
            nodelist: None,
            num_cpus: 0,
            min_memory: String::new(),
            partition: String::new(),
            qos: String::new(),
            account: None,
            priority: None,
            work_dir: None,
            submit_time: None,
            start_time: None,
            end_time: None,
            reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_long_and_short_state_codes() {
        assert_eq!(JobState::from_str("RUNNING").unwrap(), JobState::Running);
        assert_eq!(JobState::from_str("R").unwrap(), JobState::Running);
        assert_eq!(JobState::from_str("pd").unwrap(), JobState::Pending);
        assert_eq!(
            JobState::from_str("OUT_OF_MEMORY").unwrap(),
            JobState::OutOfMemory
        );
        assert_eq!(JobState::from_str("OOM").unwrap(), JobState::OutOfMemory);
    }

    #[test]
    fn unknown_state_falls_back_to_unknown() {
        assert_eq!(JobState::from_str("NONSENSE").unwrap(), JobState::Unknown);
    }

    #[test]
    fn display_round_trips_through_from_str() {
        for st in JobState::all_known() {
            let shown = st.to_string();
            assert_eq!(JobState::from_str(&shown).unwrap(), st, "state {:?}", st);
        }
    }
}
