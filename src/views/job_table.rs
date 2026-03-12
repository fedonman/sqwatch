use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::backend::{Job, JobState};
use crate::views::fields::{JobField, OrderedField, Ordering};

pub struct JobTable {
    pub tbl_state: TableState,
    pub jobs: Vec<Job>,
    pub marked: Vec<usize>,
    pub primary_sort_col: usize,
    pub sort_asc: bool,
}

impl JobTable {
    pub fn new() -> Self {
        Self {
            tbl_state: TableState::default(),
            jobs: Vec::new(),
            marked: Vec::new(),
            primary_sort_col: 0,
            sort_asc: true,
        }
    }

    pub fn set_jobs(&mut self, data: Vec<Job>) {
        // Preserve marked selections across refreshes by remapping via job ID
        let marked_ids: Vec<String> = self
            .marked
            .iter()
            .filter_map(|&i| self.jobs.get(i))
            .map(|j| j.job_id.clone())
            .collect();

        // Preserve cursor position by job ID
        let focused_id = self
            .tbl_state
            .selected()
            .and_then(|i| self.jobs.get(i))
            .map(|j| j.job_id.clone());

        self.jobs = data;

        // Remap marked indices
        self.marked = self
            .jobs
            .iter()
            .enumerate()
            .filter(|(_, j)| marked_ids.contains(&j.job_id))
            .map(|(i, _)| i)
            .collect();

        // Restore cursor position
        if let Some(ref id) = focused_id {
            if let Some(pos) = self.jobs.iter().position(|j| &j.job_id == id) {
                self.tbl_state.select(Some(pos));
                return;
            }
        }

        if let Some(sel) = self.tbl_state.selected() {
            if sel >= self.jobs.len() {
                self.tbl_state.select(Some(0));
            }
        } else if !self.jobs.is_empty() {
            self.tbl_state.select(Some(0));
        }
    }

    pub fn flip_selection(&mut self) {
        if let Some(idx) = self.tbl_state.selected() {
            if self.marked.contains(&idx) {
                self.marked.retain(|&i| i != idx);
            } else {
                self.marked.push(idx);
            }
        }
    }

    pub fn everything_marked(&self) -> bool {
        self.marked.len() == self.jobs.len()
    }

    pub fn mark_all(&mut self) {
        self.marked = (0..self.jobs.len()).collect();
    }

    pub fn unmark_all(&mut self) {
        self.marked.clear();
    }

    pub fn sync_sort(&mut self, visible: &[JobField], sort_fields: &[OrderedField]) {
        if let Some(first) = sort_fields.first() {
            let pos = visible
                .iter()
                .position(|f| std::mem::discriminant(f) == std::mem::discriminant(&first.field))
                .unwrap_or(0);
            self.primary_sort_col = pos;
            self.sort_asc = matches!(first.direction, Ordering::Asc);
        }
    }

    pub fn advance(&mut self) -> bool {
        if self.jobs.is_empty() {
            return false;
        }
        let prev = self.tbl_state.selected();
        let next = match prev {
            Some(i) if i >= self.jobs.len() - 1 => 0,
            Some(i) => i + 1,
            None => 0,
        };
        self.tbl_state.select(Some(next));
        prev != Some(next)
    }

    pub fn retreat(&mut self) -> bool {
        if self.jobs.is_empty() {
            return false;
        }
        let prev = self.tbl_state.selected();
        let next = match prev {
            Some(0) => self.jobs.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.tbl_state.select(Some(next));
        prev != Some(next)
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        visible: &[JobField],
        sort_fields: &[OrderedField],
    ) {
        if !sort_fields.is_empty() {
            self.sync_sort(visible, sort_fields);
        }

        if visible.is_empty() {
            let msg = Paragraph::new("No columns selected. Press 'c' to configure columns.")
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().title("Warning").borders(Borders::ALL));
            frame.render_widget(msg, area);
            return;
        }

        let header_cells = visible.iter().map(|f| {
            let is_sorted = sort_fields.iter().any(|of| of.field.heading() == f.heading());
            let indicator = if is_sorted {
                sort_fields
                    .iter()
                    .find(|of| of.field.heading() == f.heading())
                    .map(|of| match of.direction {
                        Ordering::Asc => " \u{2191}",
                        Ordering::Desc => " \u{2193}",
                    })
                    .unwrap_or("")
            } else {
                ""
            };

            let sty = Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD);

            Cell::from(format!("{}{}", f.heading(), indicator)).style(sty)
        });

        let header = Row::new(header_cells)
            .style(Style::default().bg(Color::DarkGray))
            .height(1);

        let data_rows = self.jobs.iter().enumerate().map(|(i, job)| {
            let is_marked = self.marked.contains(&i);
            let tint = state_color(job.state);

            let row_style = if is_marked {
                Style::default()
                    .fg(tint)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(tint)
            };

            let cells: Vec<Cell> = visible
                .iter()
                .map(|f| {
                    let txt = match f {
                        JobField::Id => job.job_id.clone(),
                        JobField::Name => {
                            if job.name.len() > 30 {
                                format!("{}...", &job.name[0..27])
                            } else {
                                job.name.clone()
                            }
                        }
                        JobField::User => job.user.clone(),
                        JobField::State => job.state.to_string(),
                        JobField::Partition => job.partition.clone(),
                        JobField::QoS => job.qos.clone(),
                        JobField::Nodes => job.num_nodes.to_string(),
                        JobField::Node => {
                            job.nodelist.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::CPUs => job.num_cpus.to_string(),
                        JobField::Time => job.time.clone(),
                        JobField::Memory => job.min_memory.clone(),
                        JobField::Account => {
                            job.account.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::Priority => job
                            .priority
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "-".into()),
                        JobField::WorkDir => {
                            job.work_dir.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::SubmitTime => {
                            job.submit_time.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::StartTime => {
                            job.start_time.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::EndTime => {
                            job.end_time.clone().unwrap_or_else(|| "-".into())
                        }
                        JobField::PendReason => job
                            .reason
                            .clone()
                            .unwrap_or_else(|| "-".into()),
                    };
                    Cell::from(txt)
                })
                .collect();

            Row::new(cells).style(row_style).height(1)
        });

        let widths: Vec<Constraint> = visible
            .iter()
            .map(|f| match f {
                JobField::Name => Constraint::Min(15),
                JobField::WorkDir => Constraint::Min(20),
                JobField::SubmitTime | JobField::StartTime | JobField::EndTime => {
                    Constraint::Length(19)
                }
                other => other.width_hint(),
            })
            .collect();

        let caption = format!("{} Jobs", self.jobs.len());
        let table = Table::new(data_rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title(caption))
            .row_highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol(" \u{25b6} ");

        frame.render_stateful_widget(table, area, &mut self.tbl_state);
    }

    pub fn focused_job(&self) -> Option<&Job> {
        self.tbl_state.selected().and_then(|i| self.jobs.get(i))
    }

    pub fn marked_job_ids(&self) -> Vec<String> {
        self.marked
            .iter()
            .filter_map(|&i| self.jobs.get(i))
            .map(|j| j.job_id.clone())
            .collect()
    }
}

fn state_color(s: JobState) -> Color {
    match s {
        JobState::Pending => Color::Yellow,
        JobState::Running => Color::Green,
        JobState::Completed => Color::Blue,
        JobState::Failed | JobState::Timeout | JobState::NodeFail | JobState::BootFail => {
            Color::Red
        }
        JobState::Cancelled => Color::Magenta,
        _ => Color::White,
    }
}
