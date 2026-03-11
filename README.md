# sqwatch

A lightweight terminal UI for watching and managing SLURM job queues in real time.

`sqwatch` gives you a live, interactive dashboard for your SLURM cluster right in the terminal. Browse jobs, inspect scripts and logs, filter by any field, and cancel jobs — all without leaving the command line.

## Features

- **Live queue view** — Auto-refreshing job table with color-coded states (pending, running, failed, etc.)
- **Flexible filtering** — Narrow down by user, state, partition, QoS, job name (regex), or node (regex)
- **Column configuration** — Choose which squeue fields to display, reorder them, and set sort priorities
- **Script inspector** — Read the submission script of any job, with syntax highlighting via `bat` if available
- **Log viewer** — Tail stdout/stderr logs in real time with automatic file watching
- **Bulk actions** — Select one or many jobs and cancel them in batch

## Requirements

- SLURM client utilities (`squeue`, `scontrol`, `sinfo`, `scancel`) must be in your `PATH`
- Optional: [`bat`](https://github.com/sharkdp/bat) for syntax-highlighted script viewing

## Installation

From crates.io:

```
cargo install sqwatch
```

From source:

```
git clone https://github.com/fedonman/sqwatch.git
cd sqwatch
cargo install --path .
```

## Usage

```
sqwatch
```

The UI starts immediately. It detects your username and shows your jobs by default.

## Keybindings

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate job list |
| `Space` | Toggle selection on focused job |
| `a` | Select / deselect all |
| `Enter` | View job script |
| `v` | View job log (stdout/stderr) |
| `f` | Open filter dialog |
| `c` | Open column / sort configuration |
| `r` | Manual refresh |
| `x` | Cancel selected jobs (with confirmation) |
| `Esc` | Close overlay or quit |

Inside the script and log viewers, use `Up`/`Down` to scroll and `Shift+Up`/`Shift+Down` to switch between jobs.

## License

MIT
