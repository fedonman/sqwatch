//! Fixture-driven tests for `squeue` output parsing.
//!
//! The fixtures use `|` as a visible separator; the tests swap it for the
//! real `FIELD_SEP` control character before parsing.

use sqwatch::backend::Job;
use sqwatch::backend::JobState;
use sqwatch::backend::query::{FIELD_SEP, decode_squeue_output};
use sqwatch::views::fields::JobField;

fn default_fmt() -> String {
    ["%i", "%j", "%u", "%T", "%M", "%N", "%C", "%m", "%P", "%q"].join(FIELD_SEP)
}

fn line(cells: &[&str]) -> String {
    cells.join(FIELD_SEP)
}

#[test]
fn decodes_fixture_rows() {
    let raw = include_str!("fixtures/squeue_default.txt").replace('|', FIELD_SEP);
    let jobs = decode_squeue_output(&raw, &default_fmt());

    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].job_id, "1001");
    assert_eq!(jobs[0].name, "train_model");
    assert_eq!(jobs[0].state, JobState::Running);
    assert_eq!(jobs[0].num_cpus, 8);
    assert_eq!(jobs[1].state, JobState::Pending);
    assert_eq!(jobs[1].nodelist, None); // "N/A" is treated as unset
    assert_eq!(jobs[2].state, JobState::Completed);
    assert_eq!(jobs[2].nodelist.as_deref(), Some("node[02-03]"));
    assert_eq!(jobs[2].num_cpus, 16);
}

#[test]
fn empty_and_na_cells_are_unset() {
    let raw = line(&["7", "N/A", ""]);
    let jobs = decode_squeue_output(&raw, &["%i", "%N", "%a"].join(FIELD_SEP));
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_id, "7");
    assert!(jobs[0].nodelist.is_none());
    assert!(jobs[0].account.is_none());
}

#[test]
fn job_name_containing_pipe_is_not_split() {
    let raw = line(&["9", "step|one|two", "alice"]);
    let jobs = decode_squeue_output(&raw, &["%i", "%j", "%u"].join(FIELD_SEP));
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].name, "step|one|two");
    assert_eq!(jobs[0].user, "alice");
}

#[test]
fn blank_lines_are_skipped() {
    let jobs = decode_squeue_output("\n  \n1\n\n2\n", "%i");
    assert_eq!(jobs.len(), 2);
}

/// Every column the UI can request must decode into a field; otherwise the
/// column silently renders empty.
#[test]
fn every_field_code_is_decoded() {
    for field in JobField::enumerate() {
        let code = field.format_code();
        // A value that is non-default for every field's type.
        let value = if code == "%T" { "RUNNING" } else { "1" };
        let jobs = decode_squeue_output(value, code);
        assert_eq!(jobs.len(), 1, "no row decoded for {}", code);
        assert_ne!(
            jobs[0],
            Job::default(),
            "format code {} ({:?}) is requested but never decoded",
            code,
            field
        );
    }
}

/// `%R` is dual purpose: for a running job it returns the allocated nodes,
/// so a Reason column built on it just repeats the Node column.
#[test]
fn the_reason_column_asks_for_the_single_purpose_code() {
    assert_eq!(JobField::PendReason.format_code(), "%r");
}

#[test]
fn reason_decodes_for_a_pending_job() {
    let raw = line(&["1002", "N/A", "Priority"]);
    let jobs = decode_squeue_output(&raw, &["%i", "%N", "%r"].join(FIELD_SEP));
    assert_eq!(jobs[0].reason.as_deref(), Some("Priority"));
}

/// `squeue` writes "None" rather than an empty cell for a job with no reason,
/// and the table shows an unset reason as the same "-" as any other blank.
#[test]
fn a_reason_of_none_reads_as_unset() {
    let raw = line(&["1001", "None"]);
    let jobs = decode_squeue_output(&raw, &["%i", "%r"].join(FIELD_SEP));
    assert_eq!(jobs[0].reason, None);
}
