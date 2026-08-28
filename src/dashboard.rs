use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use crate::{
    backend::{
        Job, JobState,
        commands::{cancel_jobs, check_slurm_available, list_nodes, list_partitions, list_qos},
        query::{FIELD_SEP, QueryParams, fetch_jobs},
    },
    core::{
        config::{
            SavedSettings, load_columns, load_filters, load_layout, load_settings, save_columns,
            save_filters, save_layout, save_settings,
        },
        input::{InputConfig, InputLoop, Signal},
        job_detail::JobDetailResolver,
        job_fetcher::JobFetcher,
    },
    views::{
        chrome::{build_frame, popup_rect, render_statusbar, render_titlebar},
        custom_widget::CustomOutputWidget,
        fields::{FieldAction, FieldSelector, JobField, OrderedField, SortDirection},
        filter_tree::{FilterTree, FilterTreeAction},
        job_table::JobTable,
        output_widget::{OutputWidget, StreamKind},
        script_widget::ScriptWidget,
        widget_selector::{VisibleWidgets, WidgetKind, WidgetSelector, WidgetSelectorAction},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusWidget {
    Table,
    Script,
    Stdout,
    Stderr,
    Sidebar,
    Custom(usize),
}

pub struct Dashboard {
    pub alive: bool,
    pub input: InputLoop,
    pub table: JobTable,
    pub params: QueryParams,
    pub rt: Runtime,
    pub refreshed_at: Instant,
    pub field_sel: FieldSelector,
    pub stdout_widget: OutputWidget,
    pub stderr_widget: OutputWidget,
    pub script: ScriptWidget,
    pub filter_tree: FilterTree,
    pub widget_sel: WidgetSelector,
    pub visible_widgets: VisibleWidgets,
    pub custom_widgets: Vec<CustomOutputWidget>,
    pub focus: FocusWidget,
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
    show_help: bool,
    job_detail_resolver: JobDetailResolver,
    job_fetcher: JobFetcher,
    pending_filter_apply: bool,
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

        // Drop any saved regex that no longer compiles so the runtime filter
        // path only ever sees valid patterns (validated once, here).
        if let Some(p) = &params.user
            && regex::Regex::new(p).is_err()
        {
            params.user = None;
        }
        if let Some(p) = &params.name_pattern
            && regex::Regex::new(p).is_err()
        {
            params.name_pattern = None;
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
                    direction: SortDirection::Asc,
                }],
            )
        });

        let refresh_secs = load_settings()
            .map(|s| s.refresh_secs.clamp(1, 60))
            .unwrap_or(3);

        let visible_widgets = load_layout().unwrap_or_default();
        let custom_widgets = visible_widgets
            .custom
            .iter()
            .enumerate()
            .map(|(i, def)| CustomOutputWidget::new(i, def.title.clone(), def.filename.clone()))
            .collect();

        Ok(Self {
            alive: true,
            input: InputLoop::start(InputConfig::default()),
            table: JobTable::new(),
            params,
            rt,
            refreshed_at: Instant::now(),
            field_sel: FieldSelector::new(visible_fields.clone(), sort_fields.clone()),
            stdout_widget: OutputWidget::new_for(StreamKind::Stdout),
            stderr_widget: OutputWidget::new_for(StreamKind::Stderr),
            script: ScriptWidget::new(),
            filter_tree: FilterTree::new(),
            widget_sel: WidgetSelector::new(),
            visible_widgets,
            custom_widgets,
            focus: FocusWidget::Table,
            notice: String::new(),
            notice_expires: None,
            refresh_secs,
            known_partitions,
            known_qos,
            known_nodes,
            known_states,
            visible_fields,
            sort_fields,
            login_user: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
            confirming_cancel: false,
            show_help: false,
            job_detail_resolver: JobDetailResolver::new(),
            job_fetcher: JobFetcher::new(),
            pending_filter_apply: false,
        })
    }

    pub fn run<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut ratatui::Terminal<B>,
    ) -> Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        self.reload_jobs_sync()?;

        while self.alive {
            terminal.draw(|f| self.draw(f))?;
            self.process_input()?;
        }

        Ok(())
    }

    /// Synchronous job reload — used only during initialization.
    fn reload_jobs_sync(&mut self) -> Result<()> {
        self.rebuild_format();

        let p = self.params.clone();
        let mut jobs = match self.rt.block_on(fetch_jobs(&p)) {
            Ok(jobs) => jobs,
            Err(e) => {
                self.flash(format!("squeue failed: {}", e), 10);
                Vec::new()
            }
        };

        let mut stats = Vec::new();
        let total = jobs.len();

        stats.extend(self.run_regex_filters(&mut jobs));

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

    /// Submit an asynchronous job list reload to the background fetcher.
    fn submit_reload(&mut self) {
        self.rebuild_format();
        let p = self.params.clone();
        self.job_fetcher.submit(p);
    }

    /// Handle a completed async fetch result: apply regex filters and update the table.
    fn apply_fetched_jobs(&mut self, mut jobs: Vec<Job>) {
        let mut stats = Vec::new();
        let total = jobs.len();

        stats.extend(self.run_regex_filters(&mut jobs));

        if self.pending_filter_apply {
            self.pending_filter_apply = false;
            let remaining = jobs.len();
            let desc = self.filter_summary();
            if !desc.is_empty() && desc != "No filters applied" {
                self.flash(
                    format!("Filters applied: {} ({} jobs shown)", desc, remaining),
                    3,
                );
            } else {
                self.flash(format!("Filters cleared ({} jobs shown)", remaining), 3);
            }
        } else if !stats.is_empty() {
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
    }

    fn apply_regex_filter(
        jobs: &mut Vec<crate::backend::Job>,
        pattern: &str,
        field: fn(&crate::backend::Job) -> &str,
    ) -> std::result::Result<Option<String>, regex::Error> {
        if pattern.is_empty() {
            return Ok(None);
        }
        let re = regex::Regex::new(pattern)?;
        let before = jobs.len();
        jobs.retain(|j| re.is_match(field(j)));
        let after = jobs.len();
        if before != after && before > 0 {
            Ok(Some(format!(
                "{}/{} ({:.1}%)",
                after,
                before,
                (after as f64 / before as f64) * 100.0
            )))
        } else {
            Ok(None)
        }
    }

    /// Apply the user and job-name regex filters to `jobs`, returning a
    /// stat string per filter that removed rows. Shared by the synchronous
    /// startup reload and the async fetch path.
    fn run_regex_filters(&mut self, jobs: &mut Vec<Job>) -> Vec<String> {
        let mut stats = Vec::new();
        if let Some(pat) = self.params.user.clone() {
            match Self::apply_regex_filter(jobs, &pat, |j| &j.user) {
                Ok(Some(stat)) => stats.push(format!("user: {}", stat)),
                Ok(None) => {}
                Err(e) => self.flash(format!("Invalid user regex pattern: {}", e), 3),
            }
        }
        if let Some(pat) = self.params.name_pattern.clone() {
            match Self::apply_regex_filter(jobs, &pat, |j| &j.name) {
                Ok(Some(stat)) => stats.push(format!("name: {}", stat)),
                Ok(None) => {}
                Err(e) => self.flash(format!("Invalid name regex pattern: {}", e), 3),
            }
        }
        stats
    }

    // ── Drawing ──────────────────────────────────────────────

    pub fn draw(&mut self, frame: &mut Frame) {
        self.filter_tree.open = self.visible_widgets.filters;

        let layout = build_frame(frame, &self.visible_widgets);

        self.draw_titlebar(frame, layout.titlebar);
        self.draw_statusbar(frame, layout.statusbar);

        self.sync_widgets_to_focused_job();

        // Filter sidebar
        if let Some(sidebar_area) = layout.sidebar {
            self.filter_tree.render(
                frame,
                sidebar_area,
                self.focus == FocusWidget::Sidebar,
                &self.params,
                &self.known_states,
                &self.known_partitions,
                &self.known_qos,
                &self.known_nodes,
            );
        }

        // Job table
        self.table.render(
            frame,
            layout.table,
            &self.visible_fields,
            &self.sort_fields,
            self.focus == FocusWidget::Table,
        );

        // Right-panel and bottom-panel widgets
        for (kind, area) in layout
            .right_widgets
            .iter()
            .chain(layout.bottom_widgets.iter())
        {
            self.render_widget_by_kind(frame, kind, *area);
        }

        // Popups
        if self.widget_sel.visible {
            let custom_count = self.visible_widgets.custom.len();
            let pct_h = (45 + custom_count as u16 * 3).min(80);
            let r = popup_rect(frame.area(), 50, pct_h);
            self.widget_sel.render(frame, r, &self.visible_widgets);
        }

        if self.field_sel.visible {
            let r = popup_rect(frame.area(), 75, 75);
            self.field_sel.render(frame, r);
        }

        if self.confirming_cancel {
            let r = popup_rect(frame.area(), 45, 25);
            self.draw_cancel_confirm(frame, r);
        }

        if self.show_help {
            let r = popup_rect(frame.area(), 60, 85);
            self.draw_help(frame, r);
        }
    }

    fn render_widget_by_kind(&mut self, frame: &mut Frame, kind: &WidgetKind, area: Rect) {
        match kind {
            WidgetKind::Script => {
                self.script
                    .render_inline(frame, area, self.focus == FocusWidget::Script);
            }
            WidgetKind::Stdout => {
                let focused = self.focus == FocusWidget::Stdout;
                self.stdout_widget.render_inline(frame, area, focused);
            }
            WidgetKind::Stderr => {
                let focused = self.focus == FocusWidget::Stderr;
                self.stderr_widget.render_inline(frame, area, focused);
            }
            WidgetKind::Custom(i) => {
                let focused = self.focus == FocusWidget::Custom(*i);
                if let Some(cw) = self.custom_widgets.get_mut(*i) {
                    cw.render_inline(frame, area, focused);
                }
            }
            WidgetKind::Filters => {}
        }
    }

    fn sync_widgets_to_focused_job(&mut self) {
        if let Some(job) = self.table.focused_job() {
            let id = job.job_id.clone();
            let name = job.name.clone();
            let work_dir = job.work_dir.clone();
            self.script.ensure_job(&id, &name);
            self.stdout_widget.ensure_job(&id);
            self.stderr_widget.ensure_job(&id);
            for cw in &mut self.custom_widgets {
                cw.ensure_job(&id, work_dir.as_deref());
            }

            // Request detail from background resolver (no-op if cached or in-flight)
            self.job_detail_resolver.request(&id);

            // Push cached detail to widgets that need it
            let detail = self.job_detail_resolver.get_cached(&id).cloned();
            if let Some(ref d) = detail {
                self.stdout_widget.set_detail(d);
                self.stderr_widget.set_detail(d);
                self.script.set_detail(d);
            }
        } else {
            self.script.clear_job();
            self.stdout_widget.clear_job();
            self.stderr_widget.clear_job();
            for cw in &mut self.custom_widgets {
                cw.clear_job();
            }
        }
    }

    /// Push newly resolved job details to widgets after polling the resolver.
    fn push_resolved_details(&mut self) {
        let Some(job) = self.table.focused_job() else {
            return;
        };
        let id = job.job_id.clone();

        let detail = self.job_detail_resolver.get_cached(&id).cloned();
        if let Some(ref d) = detail {
            self.stdout_widget.set_detail(d);
            self.stderr_widget.set_detail(d);
            self.script.set_detail(d);
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
        render_statusbar(frame, area, (pending, running, other), &self.focus);
    }

    fn draw_titlebar(&self, frame: &mut Frame, area: Rect) {
        let flash = if let Some(deadline) = self.notice_expires {
            if Instant::now() < deadline {
                Some(self.notice.as_str())
            } else {
                None
            }
        } else {
            None
        };

        render_titlebar(frame, area, &self.login_user, flash);
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

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(Line::from(" \u{25c6} Keybindings \u{25c6} ").centered())
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(200, 120, 255)))
            .style(Style::default().bg(Color::Rgb(15, 15, 30)));

        let widget = Paragraph::new(help_lines()).block(block);
        frame.render_widget(widget, area);
    }

    // ── Input handling ───────────────────────────────────────

    fn process_input(&mut self) -> Result<()> {
        use std::sync::mpsc::RecvTimeoutError;

        // Wait for the next event, but time out so a dead input thread (a
        // dropped sender) is detected instead of blocking the UI forever.
        let first = match self.input.rx.recv_timeout(Duration::from_millis(500)) {
            Ok(sig) => sig,
            Err(RecvTimeoutError::Timeout) => return Ok(()),
            Err(RecvTimeoutError::Disconnected) => {
                color_eyre::eyre::bail!("input worker thread stopped unexpectedly");
            }
        };

        // Drain every additional pending event so stale timers that
        // accumulated while the terminal was unfocused are collapsed
        // into a single tick instead of each triggering a full draw.
        let mut events = vec![first];
        while let Ok(sig) = self.input.rx.try_recv() {
            events.push(sig);
        }

        let mut had_timer = false;
        for sig in events {
            match sig {
                Signal::Keyboard(k) if k.kind == KeyEventKind::Press => self.on_keypress(k),
                Signal::Mouse(m) => self.on_mouse(m),
                Signal::TermResize(_, _) => {}
                Signal::Timer => had_timer = true,
                _ => {}
            }
        }

        if had_timer {
            self.on_timer();
        }

        Ok(())
    }

    fn on_keypress(&mut self, key: KeyEvent) {
        // ── Help overlay (modal, any key dismisses) ──
        if self.show_help {
            self.show_help = false;
            return;
        }

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

        if self.widget_sel.visible {
            let action = self.widget_sel.handle_key(key, &mut self.visible_widgets);
            match action {
                WidgetSelectorAction::Dismiss => self.widget_sel.visible = false,
                WidgetSelectorAction::Changed => {
                    self.sync_custom_widget_instances();
                    self.filter_tree.open = self.visible_widgets.filters;
                    if self.visible_widgets.filters {
                        self.filter_tree.sync_from_params(&self.params);
                    } else {
                        self.filter_tree.editing = false;
                    }
                    if !self.is_widget_visible(&self.focus) {
                        self.focus = FocusWidget::Table;
                    }
                }
                WidgetSelectorAction::Save => match save_layout(&self.visible_widgets) {
                    Ok(_) => self.flash("Layout settings saved".into(), 3),
                    Err(e) => self.flash(format!("Failed to save layout: {}", e), 3),
                },
                WidgetSelectorAction::Noop => {}
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
                    self.submit_reload();
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
        if self.focus == FocusWidget::Sidebar && self.filter_tree.is_editing() {
            self.on_sidebar_key(key);
            return;
        }

        // ── Global keys ──
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc) => {
                if self.focus != FocusWidget::Table {
                    self.focus = FocusWidget::Table;
                } else {
                    self.alive = false;
                }
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                match &self.focus {
                    FocusWidget::Script => {
                        self.copy_and_flash(&self.script.body.clone(), "Script contents");
                    }
                    FocusWidget::Stdout => {
                        self.copy_and_flash(&self.stdout_widget.content.clone(), "Stdout contents");
                    }
                    FocusWidget::Stderr => {
                        self.copy_and_flash(&self.stderr_widget.content.clone(), "Stderr contents");
                    }
                    FocusWidget::Custom(i) => {
                        if let Some(cw) = self.custom_widgets.get(*i) {
                            let title = cw.title.clone();
                            let content = cw.content.clone();
                            self.copy_and_flash(&content, &format!("{} contents", title));
                        }
                    }
                    FocusWidget::Sidebar | FocusWidget::Table => {
                        self.field_sel = FieldSelector::new(
                            self.visible_fields.clone(),
                            self.sort_fields.clone(),
                        );
                        self.field_sel.visible = true;
                    }
                }
                return;
            }
            (_, KeyCode::Tab) => {
                self.cycle_focus(true);
                return;
            }
            (_, KeyCode::BackTab) => {
                self.cycle_focus(false);
                return;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                self.widget_sel.visible = true;
                return;
            }
            (_, KeyCode::Char('+')) | (_, KeyCode::Char('=')) => {
                self.adjust_refresh(1);
                return;
            }
            (_, KeyCode::Char('-')) | (_, KeyCode::Char('_')) => {
                self.adjust_refresh(-1);
                return;
            }
            (_, KeyCode::Char('?')) => {
                self.show_help = true;
                return;
            }
            _ => {}
        }

        // ── Focus-specific dispatch ──
        match &self.focus {
            FocusWidget::Table => self.on_table_key(key),
            FocusWidget::Script
            | FocusWidget::Stdout
            | FocusWidget::Stderr
            | FocusWidget::Custom(_) => {
                let focus = self.focus.clone();
                self.on_widget_key(key, &focus);
            }
            FocusWidget::Sidebar => self.on_sidebar_key(key),
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
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                if self.table.everything_marked() {
                    self.table.unmark_all();
                } else {
                    self.table.mark_all();
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                self.confirming_cancel = true;
            }
            _ => {}
        }
    }

    fn on_widget_key(&mut self, key: KeyEvent, widget: &FocusWidget) {
        match (key.modifiers, key.code) {
            (KeyModifiers::SHIFT, KeyCode::Up) => {
                self.table.retreat();
            }
            (KeyModifiers::SHIFT, KeyCode::Down) => {
                self.table.advance();
            }
            _ => match widget {
                FocusWidget::Script => self.script.handle_key(key),
                FocusWidget::Stdout => self.stdout_widget.handle_key(key),
                FocusWidget::Stderr => self.stderr_widget.handle_key(key),
                FocusWidget::Custom(i) => {
                    if let Some(cw) = self.custom_widgets.get_mut(*i) {
                        cw.handle_key(key);
                    }
                }
                _ => {}
            },
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
                if action == FilterTreeAction::Applied {
                    self.apply_search();
                }
            }
        }
    }

    fn focusable_widgets(&self) -> Vec<FocusWidget> {
        let mut items = vec![FocusWidget::Table];
        if self.visible_widgets.script {
            items.push(FocusWidget::Script);
        }
        if self.visible_widgets.stdout {
            items.push(FocusWidget::Stdout);
        }
        if self.visible_widgets.stderr {
            items.push(FocusWidget::Stderr);
        }
        for (i, c) in self.visible_widgets.custom.iter().enumerate() {
            if c.visible {
                items.push(FocusWidget::Custom(i));
            }
        }
        if self.visible_widgets.filters {
            items.push(FocusWidget::Sidebar);
        }
        items
    }

    fn cycle_focus(&mut self, forward: bool) {
        let items = self.focusable_widgets();
        if items.len() <= 1 {
            return;
        }
        let current = items.iter().position(|p| *p == self.focus).unwrap_or(0);
        let next = if forward {
            (current + 1) % items.len()
        } else {
            (current + items.len() - 1) % items.len()
        };
        self.focus = items[next].clone();
    }

    fn is_widget_visible(&self, widget: &FocusWidget) -> bool {
        match widget {
            FocusWidget::Table => true,
            FocusWidget::Script => self.visible_widgets.script,
            FocusWidget::Stdout => self.visible_widgets.stdout,
            FocusWidget::Stderr => self.visible_widgets.stderr,
            FocusWidget::Sidebar => self.visible_widgets.filters,
            FocusWidget::Custom(i) => self
                .visible_widgets
                .custom
                .get(*i)
                .is_some_and(|c| c.visible),
        }
    }

    /// Keep custom_widgets vec in sync with visible_widgets.custom definitions.
    fn sync_custom_widget_instances(&mut self) {
        let defs = &self.visible_widgets.custom;
        // Resize: add new or trim removed
        while self.custom_widgets.len() < defs.len() {
            let i = self.custom_widgets.len();
            let def = &defs[i];
            self.custom_widgets.push(CustomOutputWidget::new(
                i,
                def.title.clone(),
                def.filename.clone(),
            ));
        }
        self.custom_widgets.truncate(defs.len());
        // Sync index and metadata for each
        for (i, cw) in self.custom_widgets.iter_mut().enumerate() {
            cw.def_index = i;
            cw.title = defs[i].title.clone();
            cw.filename = defs[i].filename.clone();
        }
    }

    fn on_mouse(&mut self, _ev: MouseEvent) {}

    fn on_timer(&mut self) {
        // Poll for completed job list fetch
        if let Some(result) = self.job_fetcher.poll() {
            match result {
                Ok(jobs) => self.apply_fetched_jobs(jobs),
                Err(e) => {
                    self.pending_filter_apply = false;
                    self.flash(format!("Auto-refresh failed: {}", e), 3);
                }
            }
        }

        // Submit new fetch if interval elapsed and none in-flight
        if !self.job_fetcher.in_flight
            && !self.field_sel.visible
            && !self.widget_sel.visible
            && self.refreshed_at.elapsed().as_secs() >= self.refresh_secs
        {
            self.submit_reload();
        }

        // Poll job detail resolver and push results to widgets
        self.job_detail_resolver.poll();
        self.push_resolved_details();

        // Poll all visible widgets for file/content updates
        if self.visible_widgets.script {
            self.script.poll_updates();
        }
        if self.visible_widgets.stdout {
            self.stdout_widget.poll_updates();
        }
        if self.visible_widgets.stderr {
            self.stderr_widget.poll_updates();
        }
        for (i, c) in self.visible_widgets.custom.iter().enumerate() {
            if c.visible
                && let Some(cw) = self.custom_widgets.get_mut(i)
            {
                cw.poll_updates();
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────

    /// Copy text to the system clipboard via the OSC 52 escape sequence.
    /// Works over SSH and inside tmux without requiring X11/Wayland.
    /// Returns whether the escape sequence was actually written.
    fn copy_to_clipboard(&self, text: &str) -> bool {
        use std::io::Write;
        let encoded = BASE64.encode(text);
        let seq = format!("\x1b]52;c;{}\x07", encoded);
        let mut out = std::io::stdout();
        out.write_all(seq.as_bytes())
            .and_then(|_| out.flush())
            .is_ok()
    }

    /// Copy `text` and flash success or failure honestly.
    fn copy_and_flash(&mut self, text: &str, label: &str) {
        if self.copy_to_clipboard(text) {
            self.flash(format!("{} copied", label), 3);
        } else {
            self.flash("Clipboard copy failed".into(), 3);
        }
    }

    /// Adjust the auto-refresh interval (clamped to 1–60s) and persist it.
    fn adjust_refresh(&mut self, delta: i64) {
        let new = (self.refresh_secs as i64 + delta).clamp(1, 60) as u64;
        if new == self.refresh_secs {
            return;
        }
        self.refresh_secs = new;
        match save_settings(&SavedSettings { refresh_secs: new }) {
            Ok(_) => self.flash(format!("Refresh interval: {}s", new), 3),
            Err(e) => self.flash(
                format!("Refresh interval: {}s (save failed: {})", new, e),
                3,
            ),
        }
    }

    fn flash(&mut self, msg: String, secs: u64) {
        self.notice = msg;
        self.notice_expires = Some(Instant::now() + Duration::from_secs(secs));
    }

    fn apply_search(&mut self) {
        self.flash("Applying filters...".into(), 3);
        self.pending_filter_apply = true;
        self.submit_reload();
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
        let mut codes: Vec<&str> = self
            .visible_fields
            .iter()
            .map(|f| f.format_code())
            .collect();

        // Ensure %Z (WorkDir) is present when custom widgets need it
        if self.visible_widgets.custom.iter().any(|c| c.visible) && !codes.contains(&"%Z") {
            codes.push("%Z");
        }

        self.params.fmt = codes.join(FIELD_SEP);

        self.params.ordering.clear();
        if !self.sort_fields.is_empty() {
            for sf in &self.sort_fields {
                let code = sf.field.format_code().trim_start_matches('%');
                let asc = matches!(sf.direction, SortDirection::Asc);
                self.params.ordering.push((code.to_string(), asc));
            }

            if let Some(first) = self.sort_fields.first() {
                let idx = self
                    .visible_fields
                    .iter()
                    .position(|f| std::mem::discriminant(f) == std::mem::discriminant(&first.field))
                    .unwrap_or(0);
                self.table.primary_sort_col = idx;
                self.table.sort_asc = matches!(first.direction, SortDirection::Asc);
            }
        } else {
            self.params.ordering.push(("i".to_string(), true));
            self.table.primary_sort_col = 0;
            self.table.sort_asc = true;
        }
    }

    fn do_cancel(&mut self) {
        let ids = self.table.marked_job_ids();
        let count = ids.len();
        match self.rt.block_on(cancel_jobs(ids)) {
            Ok(_) => {
                self.submit_reload();
                self.flash(format!("Cancelled {} job(s), refreshing...", count), 3);
            }
            Err(e) => {
                self.flash(format!("Cancel failed: {}", e), 5);
            }
        }
    }
}

/// Build the keybinding reference shown in the help overlay.
fn help_lines() -> Vec<Line<'static>> {
    let header = |t: &'static str| {
        Line::from(Span::styled(
            t,
            Style::default()
                .fg(Color::Rgb(200, 170, 240))
                .add_modifier(Modifier::BOLD),
        ))
    };
    let row = |k: &'static str, d: &'static str| {
        Line::from(vec![
            Span::styled(
                format!("  {:<18}", k),
                Style::default().fg(Color::Rgb(120, 200, 255)),
            ),
            Span::styled(d, Style::default().fg(Color::Rgb(200, 200, 210))),
        ])
    };

    vec![
        Line::raw(""),
        header("  Global"),
        row("Tab / Shift+Tab", "Cycle focus between panels"),
        row("Ctrl+W", "Widget layout"),
        row("+ / -", "Refresh interval"),
        row("?", "Toggle this help"),
        row("Esc", "Back to table, or quit"),
        Line::raw(""),
        header("  Job table"),
        row("Up / Down", "Navigate jobs"),
        row("Space", "Mark / unmark job"),
        row("Ctrl+A", "Select / deselect all"),
        row("Ctrl+X", "Cancel selected jobs"),
        row("Ctrl+C", "Column configuration"),
        Line::raw(""),
        header("  Log / script panels"),
        row("Up/Dn PgUp/PgDn", "Scroll"),
        row("Ctrl+U / Ctrl+D", "Page up / down"),
        row("f / End / Home", "Follow / jump to bottom / top"),
        row("Shift+Up/Down", "Switch to prev / next job"),
        row("Ctrl+C", "Copy panel contents"),
        Line::raw(""),
        header("  Filter sidebar"),
        row("Up / Down", "Navigate"),
        row("Enter", "Edit field / toggle item"),
        row("Ctrl+S", "Save filters"),
        Line::raw(""),
        Line::from(Span::styled(
            "  Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Job;

    fn job_with_user(user: &str) -> Job {
        Job {
            user: user.to_string(),
            ..Job::default()
        }
    }

    #[test]
    fn regex_filter_retains_matching_rows() {
        let mut jobs = vec![
            job_with_user("alice"),
            job_with_user("bob"),
            job_with_user("alba"),
        ];
        let stat = Dashboard::apply_regex_filter(&mut jobs, "^al", |j| &j.user).unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(stat.is_some());
    }

    #[test]
    fn regex_filter_reports_none_when_nothing_removed() {
        let mut jobs = vec![job_with_user("alice"), job_with_user("alba")];
        let stat = Dashboard::apply_regex_filter(&mut jobs, "^al", |j| &j.user).unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(stat.is_none());
    }

    #[test]
    fn regex_filter_empty_pattern_is_a_noop() {
        let mut jobs = vec![job_with_user("alice")];
        let stat = Dashboard::apply_regex_filter(&mut jobs, "", |j| &j.user).unwrap();
        assert!(stat.is_none());
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn regex_filter_errors_on_invalid_pattern() {
        let mut jobs = vec![job_with_user("alice")];
        assert!(Dashboard::apply_regex_filter(&mut jobs, "[", |j| &j.user).is_err());
    }
}
