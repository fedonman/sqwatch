# sqwatch - SLURM Queue Watcher

A lightweight terminal UI for watching and managing SLURM job queues in real time.

`sqwatch` gives you a live, interactive dashboard for your SLURM cluster right in the terminal. Browse jobs, inspect scripts and logs, filter by any field, and cancel jobs — all without leaving the command line.

## Features

- **Live queue view** — Auto-refreshing job table with color-coded states (pending, running, failed, completed, suspended, out of memory, etc.). Job selection and cursor position are preserved across refresh cycles. Job fetching runs in a background thread so the UI never stalls.
- **Flexible filtering** — Persistent sidebar for filtering by user (regex), job name (regex), state, partition, QoS, or node. Partitions, QoS, and nodes are populated from the cluster automatically. Filter settings are persisted to disk and restored on launch.
- **Column configuration** — Choose which `squeue` fields to display, reorder them, and define multi-level sort priorities. Column settings are also persisted.
- **Script inspector** — View the submission script of any job, with syntax highlighting via [`bat`](https://github.com/sharkdp/bat) if available. Falls back to plain text with line numbers. Script content loads in a background thread.
- **Log viewer** — Tail stdout/stderr logs in real time with automatic file watching via `notify`.
- **Custom output widgets** — Define additional file-watching panels for arbitrary job output files, with automatic JSON pretty-printing.
- **Widget layout** — Toggle visibility of individual panels (filters, script, stdout, stderr, custom widgets) and persist your preferred layout.
- **Clipboard support** — Copy widget contents to the system clipboard via OSC 52, which works over SSH and inside tmux without X11/Wayland.
- **Bulk actions** — Select one or many jobs and cancel them in batch with confirmation. Errors from `scancel` are reported through the flash notification bar.

## Requirements

- **SLURM client utilities** — `squeue`, `scontrol`, `sinfo`, `scancel`, and `sacctmgr` must be in your `PATH`.
- **Rust 1.90+** — Required to build from source.
- **Optional:** [`bat`](https://github.com/sharkdp/bat) for syntax-highlighted script viewing.

## Installation

From [crates.io](https://crates.io/crates/sqwatch):

```sh
cargo install sqwatch
```

From source:

```sh
git clone https://github.com/fedonman/sqwatch.git
cd sqwatch
cargo install --path .
```

## Usage

```sh
sqwatch
```

The UI starts immediately. It detects your username from `$USER` and displays all jobs by default (with `--all --states=all` passed to `squeue`).

### Configuration

Settings are stored in `~/.config/sqwatch/` (or `$XDG_CONFIG_HOME/sqwatch/`):

| File | Contents |
|------|----------|
| `filters.json` | Saved filter presets (user, states, partitions, QoS, nodes, name pattern) |
| `columns.json` | Visible columns and sort order |
| `layout.json` | Widget visibility and custom widget definitions |

Press `Ctrl+S` inside the filter sidebar, column dialog, or widget selector to persist the current configuration.

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus to next visible widget |
| `Shift+Tab` | Cycle focus to previous visible widget |
| `w` | Open widget selector (toggle panel visibility) |
| `c` | Open column / sort configuration (when table is focused) |
| `Esc` | Return focus to table, or quit if already on table |
| `Ctrl+C` | Copy focused widget contents to clipboard, or quit if on table |

### Job Table

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate job list |
| `Space` | Toggle selection on focused job |
| `a` | Select / deselect all |
| `x` | Cancel selected jobs (with confirmation) |

### Script / Log / Custom Widgets

| Key | Action |
|-----|--------|
| `Up` / `Down` | Scroll content |
| `PageUp` / `PageDown` | Scroll one page |
| `Ctrl+U` / `Ctrl+D` | Scroll one page (vim-style) |
| `Shift+Up` / `Shift+Down` | Switch to previous/next job in the table |

### Filter Sidebar

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate between fields and filter sections |
| `Enter` | Edit text field or toggle checkbox |
| `Space` | Toggle checkbox item |
| `Ctrl+S` | Save filter settings to disk |

### Column / Sort Configuration

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate within a list |
| `Left` / `Right` | Switch between pool, active, and sort lists |
| `Enter` | Add field to selected / sort, or toggle sort order |
| `Del` | Remove field from list |
| `Shift+Up` / `Shift+Down` | Reorder items |
| `Tab` | Cycle between lists |
| `r` | Reset to defaults |
| `Ctrl+S` | Save column settings to disk |
| `Esc` | Close |

### Widget Selector

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate widget list |
| `Enter` / `Space` | Toggle widget visibility |
| `Ctrl+S` | Save layout to disk |
| `Esc` | Close |

## Architecture

The project is organized into four modules:

```
src/
├── main.rs                # Entry point — terminal setup and teardown
├── dashboard.rs           # Central orchestrator — event loop, state, rendering
├── backend/
│   ├── mod.rs             # Job and JobState data types
│   ├── commands.rs        # Async wrappers around SLURM CLI tools
│   └── query.rs           # squeue invocation and output parsing
├── core/
│   ├── mod.rs
│   ├── input.rs           # Keyboard/mouse/timer event loop (crossbeam channels)
│   ├── config.rs          # Filter, column, and layout persistence (JSON, XDG paths)
│   ├── job_fetcher.rs     # Background thread for periodic squeue refreshes
│   ├── job_detail.rs      # Background scontrol cache (LRU, max 64 entries)
│   └── live_file.rs       # File watcher for live log tailing (notify crate)
└── views/
    ├── mod.rs
    ├── chrome.rs           # Titlebar, statusbar, and layout framing
    ├── job_table.rs        # Job list table with selection and sorting
    ├── filter_tree.rs      # Persistent filter sidebar with regex text fields and checkbox lists
    ├── fields.rs           # Column and sort configuration dialog
    ├── script_widget.rs    # Job script viewer with optional bat highlighting
    ├── output_widget.rs    # Live log viewer (stdout/stderr)
    ├── custom_widget.rs    # User-defined file-watching panels
    ├── widget_selector.rs  # Panel visibility toggle popup
    └── theme.rs            # Centralized color constants
```

**Dashboard** is the central hub. It owns all view components, the query parameters, the tokio runtime for async SLURM commands, and the input event channel. The main loop is: receive input signal → dispatch to the appropriate handler → redraw.

**Backend** wraps all SLURM interactions. Commands are executed asynchronously via `async-process` and dispatched through a shared tokio runtime. The query module builds `squeue` invocations with dynamic format strings and parses the pipe-delimited output.

**Core** handles cross-cutting concerns: the input loop runs on a dedicated thread, multiplexing keyboard, mouse, resize, and timer events into a single `crossbeam` channel. Background workers (`job_fetcher` and `job_detail`) run SLURM queries off the main thread, communicating results back via crossbeam channels polled on timer ticks. The config module manages JSON persistence for filters, columns, and layout. The live file watcher uses `notify` to detect log file changes for real-time tailing.

**Views** are pure rendering components. Each one receives a `Frame` and `Rect` from ratatui and draws itself. The filter sidebar is a persistent side panel, while overlays (column config, widget selector) are rendered on top of the main layout via popup regions.

## Developers

### Prerequisites

- Rust 1.90+ (the minimum supported Rust version)
- A working SLURM environment for manual testing (or mock the CLI tools)

### Building

```sh
cargo build
```

### Running checks

```sh
cargo fmt --all --check                     # Formatting (requires nightly rustfmt)
cargo clippy --all-targets -- -D warnings   # Linting
cargo test                                  # Tests
cargo deny check                            # License and advisory audit
```

### Project conventions

- **Edition 2024** — uses let-chains and other modern Rust features.
- **No `unsafe`** — the codebase is entirely safe Rust.
- **Async for SLURM commands only** — the TUI event loop is synchronous; async is used solely for non-blocking SLURM CLI calls via `async-process` + tokio. Background workers use crossbeam channels, not async, to communicate with the dashboard.
- **`color-eyre`** for error handling — `Result<()>` flows from `main()` through the dashboard.

## Contributing

1. Fork the repository and create a feature branch from `main`.
2. Make your changes. Every PR must include an entry in `CHANGELOG.md` following the [Keep a Changelog](https://keepachangelog.com/) format.
3. Ensure all CI checks pass: formatting, clippy, tests, MSRV build, and `cargo-deny`.
4. Open a pull request against `main`. All merges use squash merge.

### Changelog format

Each PR gets a single bullet point under the appropriate section (`Added`, `Changed`, `Fixed`, `Removed`). End the entry with a link to the PR:

```markdown
- Description of the change. [PR #N](https://github.com/fedonman/sqwatch/pull/N)
```

## License

Apache-2.0
