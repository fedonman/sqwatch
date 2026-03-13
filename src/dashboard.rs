use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::{
    backend::{
        JobState,
        commands::{cancel_jobs, check_slurm_available, list_nodes, list_partitions, list_qos},
        query::{QueryParams, fetch_jobs},
    },
    core::{
        config::{load_columns, load_filters, save_columns, save_filters},
        input::{InputConfig, InputLoop, Signal},
    },
    views::{
        chrome::{build_frame, popup_rect, render_statusbar, render_titlebar},
        fields::{FieldAction, FieldSelector, JobField, OrderedField, Ordering},
        filter_tree::{FilterTree, FilterTreeAction},
        job_table::JobTable,
        output_pane::OutputPane,
        script_pane::ScriptPane,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Table,
    Script,
    Output,
    Sidebar,
}

pub struct Dashboard {
    pub alive: bool,
    pub input: InputLoop,
    pub table: JobTable,
    pub params: QueryParams,
    pub rt: Runtime,
    pub refreshed_at: Instant,
    pub field_sel: FieldSelector,
    pub output: OutputPane,
    pub script: ScriptPane,
    pub filter_tree: FilterTree,
    pub focus: FocusPanel,
    pub notice: String,
    pub notice_expires: Option<Instant>,
    pub refresh_secs: u64,
    pub known_partitions: Vec<String>,
    pub known_qos: Vec<String>,
    pub known_nodes: Vec<String>,
    pub known_states: Vec<JobState>,
    pub visible_fields: Vec<JobField>,
    pub sort_fields: Vec<OrderedField>,
    pub login_user: String,
    confirming_cancel: bool,
}

impl Dashboard {
    pub fn new() -> Result<Self> {
        check_slurm_available()?;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime init failed");

        let mut params = QueryParams::default();
        if let Some(saved) = load_filters() {
            saved.apply_to(&mut params);
        }

        let known_partitions = rt.block_on(list_partitions());
        let known_qos = rt.block_on(list_qos());
        let known_nodes = rt.block_on(list_nodes());
        let known_states = JobState::all_known();

        let (visible_fields, sort_fields) = load_columns().unwrap_or_else(|| {
            (
                JobField::defaults(),
                vec![OrderedField {
                    field: JobField::Id,
                    direction: Ordering::Asc,
                }],
            )
        });

        Ok(Self {
            alive: true,
            input: InputLoop::start(InputConfig::default()),
            table: JobTable::new(),
            params,
            rt,
            refreshed_at: Instant::now(),
            field_sel: FieldSelector::new(visible_fields.clone(), sort_fields.clone()),
            output: OutputPane::new(),
            script: ScriptPane::new(),
            filter_tree: FilterTree::new(),
            focus: FocusPanel::Table,
            notice: String::new(),
            notice_expires: None,
            refresh_secs: 1,
            known_partitions,
            known_qos,
            known_nodes,
            known_states,
            visible_fields,
            sort_fields,
            login_user: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            confirming_cancel: false,
        })
    }

    pub fn run<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) -> Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        self.reload_jobs()?;

        while self.alive {
            terminal.draw(|f| self.draw(f))?;
            self.process_input()?;
        }

        Ok(())
    }

    fn reload_jobs(&mut self) -> Result<()> {
        self.rebuild_format();

        let p = self.params.clone();
        let mut jobs = self.rt.block_on(fetch_jobs(&p))?;

        let mut stats = Vec::new();
        let total = jobs.len();

        if let Some(ref pat) = self.params.user
            && !pat.is_empty()
        {
            match regex::Regex::new(pat) {
                Ok(re) => {
                    let before = jobs.len();
                    jobs.retain(|j| re.is_match(&j.user));
                    let after = jobs.len();
                    if before != after && before > 0 {
                        stats.push(format!(
                            "user: {}/{} ({:.1}%)",
                            after,
                            before,
                            (after as f64 / before as f64) * 100.0
                        ));
                    }
                }
                Err(e) => {
                    self.flash(format!("Invalid user regex pattern: {}", e), 3);
                }
            }
        }

        if let Some(ref pat) = self.params.name_pattern
            && !pat.is_empty()
        {
            match regex::Regex::new(pat) {
                Ok(re) => {
                    let before = jobs.len();
                    jobs.retain(|j| re.is_match(&j.name));
                    let after = jobs.len();
                    if before != after && before > 0 {
                        stats.push(format!(
                            "name: {}/{} ({:.1}%)",
                            after,
                            before,
                            (after as f64 / before as f64) * 100.0
                        ));
                    }
                }
                Err(e) => {
                    self.flash(format!("Invalid name regex pattern: {}", e), 3);
                }
            }
        }

        if !stats.is_empty() {
            let remaining = jobs.len();
            let pct = if total > 0 {
                (remaining as f64 / total as f64) * 100.0
            } else {
                100.0
            };
            self.flash(
                format!(
                    "Filtered: {}/{} total ({:.1}%) [{}]",
                    remaining,
                    total,
                    pct,
                    stats.join(", ")
                ),
                5,
            );
        }

        self.table.set_jobs(jobs);
        self.refreshed_at = Instant::now();
        Ok(())
    }

    // ── Drawing ──────────────────────────────────────────────

    pub fn draw(&mut self, frame: &mut Frame) {
        let layout = build_frame(frame, self.filter_tree.open);

        self.draw_titlebar(frame, layout.titlebar);
        self.draw_statusbar(frame, layout.statusbar);

        // Sync right-side panels to focused job
        self.sync_panels_to_focused_job();

        // Filter sidebar
        if let Some(sidebar_area) = layout.sidebar {
            self.filter_tree.render(
                frame,
                sidebar_area,
                self.focus == FocusPanel::Sidebar,
                &self.params,
                &self.known_states,
                &self.known_partitions,
                &self.known_qos,
                &self.known_nodes,
            );
        }

        // Job table
        self.table
            .render(frame, layout.table, &self.visible_fields, &self.sort_fields, self.focus == FocusPanel::Table);

        // Right-side panels (always visible)
        self.script
            .render_inline(frame, layout.script, self.focus == FocusPanel::Script);
        self.output
            .render_inline(frame, layout.output, self.focus == FocusPanel::Output);

        // Popups
        if self.field_sel.visible {
            let r = popup_rect(frame.area(), 75, 75);
            self.field_sel.render(frame, r);
        }

        if self.confirming_cancel {
            let r = popup_rect(frame.area(), 45, 25);
            self.draw_cancel_confirm(frame, r);
        }
    }

    fn sync_panels_to_focused_job(&mut self) {
        if let Some(job) = self.table.focused_job() {
            let id = job.job_id.clone();
            let name = job.name.clone();
            self.script.ensure_job(&id, &name);
            self.output.ensure_job(&id);
        } else {
            self.script.clear_job();
            self.output.clear_job();
        }
    }

    fn draw_statusbar(&self, frame: &mut Frame, area: Rect) {
        let pending = self
            .table
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Pending)
            .count();
        let running = self
            .table
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Running)
            .count();
        let other = self.table.jobs.len() - pending - running;
        render_statusbar(frame, area, (pending, running, other));
    }

    fn draw_titlebar(&self, frame: &mut Frame, area: Rect) {
        let filters = self.filter_summary();

        let flash = if let Some(deadline) = self.notice_expires {
            if Instant::now() < deadline {
                Some(self.notice.as_str())
            } else {
                None
            }
        } else {
            None
        };

        render_titlebar(frame, area, &filters, &self.login_user, flash);
    }

    fn draw_cancel_confirm(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);

        let n = self.table.marked_job_ids().len();
        let msg = if n == 0 {
            "No jobs selected for cancellation.".to_string()
        } else {
            format!(
                "Are you sure you want to cancel {} selected job(s)? (y/n)",
                n
            )
        };

        let blk = Block::default()
            .title(Line::from(" \u{25c6} Confirm Cancel \u{25c6} ").centered())
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(230, 70, 70)))
            .style(Style::default().bg(Color::Rgb(15, 15, 30)));

        let widget = Paragraph::new(msg)
            .style(Style::default().fg(Color::Rgb(255, 170, 50)))
            .block(blk)
            .centered();

        frame.render_widget(widget, area);
    }

    // ── Input handling ───────────────────────────────────────

    fn process_input(&mut self) -> Result<()> {
        match self.input.rx.recv()? {
            Signal::Keyboard(k) if k.kind == KeyEventKind::Press => self.on_keypress(k),
            Signal::Mouse(m) => self.on_mouse(m),
            Signal::TermResize(_, _) => {}
            Signal::Timer => self.on_timer(),
            _ => {}
        }
        Ok(())
    }

    fn on_keypress(&mut self, key: KeyEvent) {
        // ── Popup-level dispatch (highest priority) ──
        if self.confirming_cancel {
            match key.code {
                KeyCode::Char('y') => {
                    self.do_cancel();
                    self.confirming_cancel = false;
                }
                KeyCode::Char('n') | KeyCode::Esc => self.confirming_cancel = false,
                _ => {}
            }
            return;
        }

        if self.field_sel.visible {
            let action = self.field_sel.handle_key(key);
            match action {
                FieldAction::Dismiss => self.field_sel.visible = false,
                FieldAction::Confirm => {
                    self.visible_fields = self.field_sel.active.clone();
                    self.sort_fields = self.field_sel.sort_list.clone();
                    if let Err(e) = self.reload_jobs() {
                        self.flash(format!("Failed to refresh: {}", e), 3);
                    }
                }
                FieldAction::Save => {
                    self.visible_fields = self.field_sel.active.clone();
                    self.sort_fields = self.field_sel.sort_list.clone();
                    match save_columns(&self.visible_fields, &self.sort_fields) {
                        Ok(_) => self.flash("Column settings saved".into(), 3),
                        Err(e) => self.flash(format!("Failed to save columns: {}", e), 3),
                    }
                }
                FieldAction::Noop => {}
            }
            return;
        }

        // If editing a text field in the sidebar, send all keys there
        if self.focus == FocusPanel::Sidebar && self.filter_tree.is_editing() {
            self.on_sidebar_key(key);
            return;
        }

        // ── Global keys ──
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) | (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.focus != FocusPanel::Table {
                    self.focus = FocusPanel::Table;
                } else {
                    self.alive = false;
                }
                return;
            }
            (_, KeyCode::Tab) => {
                self.cycle_focus();
                return;
            }
            (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                self.cycle_focus_reverse();
                return;
            }
            (_, KeyCode::Char('f')) => {
                let opened = self.filter_tree.toggle();
                if opened {
                    self.filter_tree.sync_from_params(&self.params);
                    self.focus = FocusPanel::Sidebar;
                } else if self.focus == FocusPanel::Sidebar {
                    self.focus = FocusPanel::Table;
                }
                return;
            }
            (_, KeyCode::Char('c')) if self.focus == FocusPanel::Table => {
                self.field_sel =
                    FieldSelector::new(self.visible_fields.clone(), self.sort_fields.clone());
                self.field_sel.visible = true;
                return;
            }
            _ => {}
        }

        // ── Focus-specific dispatch ──
        match self.focus {
            FocusPanel::Table => self.on_table_key(key),
            FocusPanel::Script => self.on_script_key(key),
            FocusPanel::Output => self.on_output_key(key),
            FocusPanel::Sidebar => self.on_sidebar_key(key),
        }
    }

    fn on_table_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Up) => {
                self.table.retreat();
            }
            (_, KeyCode::Down) => {
                self.table.advance();
            }
            (_, KeyCode::Char(' ')) => self.table.flip_selection(),
            (_, KeyCode::Char('a')) => {
                if self.table.everything_marked() {
                    self.table.unmark_all();
                } else {
                    self.table.mark_all();
                }
            }
            (_, KeyCode::Char('x')) => {
                self.confirming_cancel = true;
            }
            (_, KeyCode::Char('s')) => {
                self.focus = FocusPanel::Script;
            }
            (_, KeyCode::Char('v')) => {
                self.focus = FocusPanel::Output;
            }
            _ => {}
        }
    }

    fn on_script_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::SHIFT, KeyCode::Up) => {
                self.table.retreat();
            }
            (KeyModifiers::SHIFT, KeyCode::Down) => {
                self.table.advance();
            }
            _ => self.script.handle_key(key),
        }
    }

    fn on_output_key(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::SHIFT, KeyCode::Up) => {
                self.table.retreat();
            }
            (KeyModifiers::SHIFT, KeyCode::Down) => {
                self.table.advance();
            }
            _ => self.output.handle_key(key),
        }
    }

    fn on_sidebar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match save_filters(&self.params) {
                    Ok(_) => self.flash("Filter settings saved".into(), 3),
                    Err(e) => self.flash(format!("Failed to save filters: {}", e), 3),
                }
            }
            _ => {
                let action = self.filter_tree.handle_key(
                    key,
                    &mut self.params,
                    &self.known_states,
                    &self.known_partitions,
                    &self.known_qos,
                    &self.known_nodes,
                );
                if action == FilterTreeAction::Applied
                    && let Err(e) = self.apply_search()
                {
                    self.flash(format!("Failed to apply filters: {}", e), 3);
                }
            }
        }
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPanel::Table => FocusPanel::Script,
            FocusPanel::Script => FocusPanel::Output,
            FocusPanel::Output => {
                if self.filter_tree.open {
                    FocusPanel::Sidebar
                } else {
                    FocusPanel::Table
                }
            }
            FocusPanel::Sidebar => FocusPanel::Table,
        };
    }

    fn cycle_focus_reverse(&mut self) {
        self.focus = match self.focus {
            FocusPanel::Table => {
                if self.filter_tree.open {
                    FocusPanel::Sidebar
                } else {
                    FocusPanel::Output
                }
            }
            FocusPanel::Script => FocusPanel::Table,
            FocusPanel::Output => FocusPanel::Script,
            FocusPanel::Sidebar => FocusPanel::Output,
        };
    }

    fn on_mouse(&mut self, _ev: MouseEvent) {}

    fn on_timer(&mut self) {
        if !self.field_sel.visible
            && self.refreshed_at.elapsed().as_secs() >= self.refresh_secs
            && let Err(e) = self.reload_jobs()
        {
            self.flash(format!("Auto-refresh failed: {}", e), 3);
        }

        // Always poll output updates (panel is always visible)
        self.output.poll_updates();
    }

    // ── Helpers ──────────────────────────────────────────────

    fn flash(&mut self, msg: String, secs: u64) {
        self.notice = msg;
        self.notice_expires = Some(Instant::now() + Duration::from_secs(secs));
    }

    fn _set_refresh_rate(&mut self, secs: u64) {
        self.refresh_secs = secs;
        self.flash(format!("Auto-refresh interval set to {}s", secs), 3);
    }

    fn apply_search(&mut self) -> Result<()> {
        self.flash("Applying filters...".into(), 3);

        let result = self.reload_jobs();

        if result.is_ok() {
            let desc = self.filter_summary();
            let count = self.table.jobs.len();
            if !desc.is_empty() && desc != "No filters applied" {
                self.flash(
                    format!("Filters applied: {} ({} jobs shown)", desc, count),
                    3,
                );
            } else {
                self.flash(format!("Filters cleared ({} jobs shown)", count), 3);
            }
        }

        result
    }

    fn filter_summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref u) = self.params.user {
            parts.push(format!("user_regex={}", u));
        }
        if !self.params.statuses.is_empty() {
            let s = self
                .params
                .statuses
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(",");
            parts.push(format!("state={}", s));
        }
        if !self.params.partitions.is_empty() {
            parts.push(format!("partition={}", self.params.partitions.join(",")));
        }
        if !self.params.qos.is_empty() {
            parts.push(format!("qos={}", self.params.qos.join(",")));
        }
        if let Some(ref n) = self.params.name_pattern {
            parts.push(format!("name_regex={}", n));
        }
        if !self.params.nodes.is_empty() {
            parts.push(format!("nodes={}", self.params.nodes.join(",")));
        }

        if parts.is_empty() {
            "No filters applied".to_string()
        } else {
            parts.join(", ")
        }
    }

    fn rebuild_format(&mut self) {
        let fmt = self
            .visible_fields
            .iter()
            .map(|f| f.format_code())
            .collect::<Vec<&str>>()
            .join("|");
        self.params.fmt = fmt;

        self.params.ordering.clear();
        if !self.sort_fields.is_empty() {
            for sf in &self.sort_fields {
                let code = sf.field.format_code().trim_start_matches('%');
                let asc = matches!(sf.direction, Ordering::Asc);
                self.params.ordering.insert(code.to_string(), asc);
            }

            if let Some(first) = self.sort_fields.first() {
                let idx = self
                    .visible_fields
                    .iter()
                    .position(|f| std::mem::discriminant(f) == std::mem::discriminant(&first.field))
                    .unwrap_or(0);
                self.table.primary_sort_col = idx;
                self.table.sort_asc = matches!(first.direction, Ordering::Asc);
            }
        } else {
            self.params.ordering.insert("i".to_string(), true);
            self.table.primary_sort_col = 0;
            self.table.sort_asc = true;
        }
    }

    fn do_cancel(&mut self) {
        let ids = self.table.marked_job_ids();
        let count = ids.len();
        match self.rt.block_on(cancel_jobs(ids)) {
            Ok(_) => {
                if let Err(e) = self.reload_jobs() {
                    self.flash(format!("Failed to refresh after cancel: {}", e), 3);
                } else {
                    self.flash(format!("Cancelled {} job(s)", count), 3);
                }
            }
            Err(e) => {
                self.flash(format!("Cancel failed: {}", e), 5);
            }
        }
    }
}
