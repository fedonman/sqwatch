//! Arguments are handled before the terminal is set up, so these run the real
//! binary with no terminal and no SLURM.

use std::process::{Command, Output};

fn sqwatch(arg: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sqwatch"))
        .arg(arg)
        .output()
        .unwrap()
}

#[test]
fn version_prints_the_crate_version() {
    for arg in ["-V", "--version"] {
        let out = sqwatch(arg);
        assert!(out.status.success(), "{} exited with {}", arg, out.status);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("sqwatch {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn help_prints_usage() {
    for arg in ["-h", "--help"] {
        let out = sqwatch(arg);
        assert!(out.status.success(), "{} exited with {}", arg, out.status);
        assert!(String::from_utf8_lossy(&out.stdout).contains("Usage: sqwatch"));
    }
}

#[test]
fn an_unknown_argument_is_rejected() {
    let out = sqwatch("--not-a-flag");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("'--not-a-flag'"), "stderr was {}", stderr);
    assert!(stderr.contains("Usage: sqwatch"), "stderr was {}", stderr);
}
