// cmd_apply_tui.rs
// TUI for interactive apply using ratatui

use crate::action::Action;
use crate::config::CliOptions;
use crate::config::{GlobalConfig, Tag};
use crate::execution_plan::create_execution_plan;
use crate::hermitgrab_error::ApplyError;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::BorderType;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

// Solarized Dark palette
const BASE03: Color = Color::Rgb(0, 43, 54);
const BASE02: Color = Color::Rgb(7, 54, 66);
const BASE01: Color = Color::Rgb(88, 110, 117);
const YELLOW: Color = Color::Rgb(181, 137, 0);
const MAGENTA: Color = Color::Rgb(211, 54, 130);
const CYAN: Color = Color::Rgb(42, 161, 152);
const GREEN: Color = Color::Rgb(133, 153, 0);

struct App {
    profiles: Vec<String>,
    tags: Vec<(Tag, bool)>,
    execution_plan: Vec<(String, bool)>,
    show_execution: bool,
    progress: u16,
    details: Option<String>,
    visual_cursor: usize, // visual line offset in execution plan
}

impl App {
    fn update_tags_for_profile(
        &mut self,
        idx: usize,
        global_config: &Arc<GlobalConfig>,
    ) -> Result<(), ApplyError> {
        if let Ok(profile_tags) = global_config.get_tags_for_profile(&self.profiles[idx]) {
            for (tag, checked) in &mut self.tags {
                if tag.is_detected() {
                    continue;
                }
                *checked = profile_tags.contains(tag);
            }
        }
        self.update_execution_plan(global_config)?;
        Ok(())
    }

    fn toggle_tag(
        &mut self,
        idx: usize,
        global_config: &Arc<GlobalConfig>,
    ) -> Result<(), ApplyError> {
        if let Some(tag) = self.tags.get_mut(idx) {
            tag.1 = !tag.1;
        }
        self.update_execution_plan(global_config)?;
        Ok(())
    }

    fn update_execution_plan(
        &mut self,
        global_config: &Arc<GlobalConfig>,
    ) -> Result<(), ApplyError> {
        let active_tags = self
            .tags
            .iter()
            .filter_map(|(t, checked)| if *checked { Some(t.clone()) } else { None })
            .collect::<BTreeSet<Tag>>();
        let actions = create_execution_plan(global_config, &CliOptions::default())?;
        let filtered_actions = actions.filter_actions_by_tags(&active_tags);
        let sorted = filtered_actions.sort_by_requires();
        self.execution_plan = sorted
            .iter()
            .map(|(_, a)| (a.short_description(), false))
            .collect::<Vec<_>>();
        Ok(())
    }

    fn start_execution(&mut self) {
        self.show_execution = true;
        self.progress = 0;
        for item in &mut self.execution_plan {
            item.1 = false;
        }
    }

    fn step_execution(&mut self) {
        for (i, item) in self.execution_plan.iter_mut().enumerate() {
            if !item.1 {
                item.1 = true;
                self.progress = ((i + 1) * 100 / self.execution_plan.len()) as u16;
                break;
            }
        }
    }

    fn execution_finished(&self) -> bool {
        self.execution_plan.iter().all(|(_, done)| *done)
    }
}

pub fn run_tui(
    global_config: &Arc<GlobalConfig>,
    tags: &[String],
    profile: &Option<String>,
) -> Result<(), ApplyError> {
    // Collect all profiles and tags from GlobalConfig
    let actions = create_execution_plan(global_config, &CliOptions::default())?;
    let mut all_tags = global_config
        .all_provided_tags()
        .iter()
        .map(|t| (t.clone(), true))
        .collect::<Vec<_>>();
    all_tags.extend(
        global_config
            .all_detected_tags()
            .iter()
            .map(|t| (t.clone(), true)),
    );
    let active_tags = global_config.get_active_tags(tags, profile)?;
    let profile_to_use = global_config.get_profile(profile)?;
    let filtered_actions = actions.filter_actions_by_tags(&active_tags);
    let sorted = filtered_actions.sort_by_requires();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut tag_state = ListState::default();
    tag_state.select(Some(0));
    let mut profile_state = ListState::default();
    profile_state.select(profile_to_use.map(|(i, _)| i));
    let mut exec_state = ListState::default();
    exec_state.select(None);
    let mut focus_on_profiles = true;

    let mut app = App {
        profiles: global_config
            .all_profiles()
            .into_iter()
            .map(|(t, _)| t.clone())
            .collect::<Vec<_>>(),
        tags: all_tags
            .into_iter()
            .map(|(t, _)| {
                let active = active_tags.contains(&t);
                (t, active)
            })
            .collect(),
        execution_plan: sorted
            .iter()
            .map(|(_, a)| (a.short_description(), false))
            .collect(),
        show_execution: false,
        progress: 0,
        details: None,
        visual_cursor: 0,
    };

    // In run_tui, before the event loop, get the width for wrapping:
    let mut last_exec_width = 0usize;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            if app.show_execution {
                last_exec_width = (area.width as usize).saturating_sub(2); // for border
                draw_execution(f, area, &app, &mut exec_state, last_exec_width);
            } else {
                draw_apply(
                    f,
                    area,
                    &app,
                    &mut profile_state,
                    &mut tag_state,
                    focus_on_profiles,
                );
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Exit on q, Ctrl+C, or Esc
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(event::KeyModifiers::CONTROL))
                {
                    break;
                }
                if app.show_execution {
                    match key.code {
                        KeyCode::Char('n') => {
                            if !app.execution_finished() {
                                app.step_execution();
                            }
                        }
                        KeyCode::Down => {
                            let total_lines = get_total_exec_lines(&app, last_exec_width);
                            app.visual_cursor =
                                (app.visual_cursor + 1).min(total_lines.saturating_sub(1));
                        }
                        KeyCode::Up => {
                            app.visual_cursor = app.visual_cursor.saturating_sub(1);
                        }
                        KeyCode::Enter => {
                            let idx = get_exec_item_for_visual_cursor(&app, last_exec_width);
                            app.details =
                                Some(format!("Details for {}", app.execution_plan[idx].0));
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Tab => {
                            focus_on_profiles = !focus_on_profiles;
                        }
                        KeyCode::Down => {
                            if focus_on_profiles {
                                let idx = profile_state.selected().unwrap_or(0);
                                let next = (idx + 1).min(app.profiles.len() - 1);
                                profile_state.select(Some(next));
                                app.update_tags_for_profile(next, global_config)?;
                            } else {
                                let idx = tag_state.selected().unwrap_or(0);
                                let next = (idx + 1).min(app.tags.len() - 1);
                                tag_state.select(Some(next));
                            }
                        }
                        KeyCode::Up => {
                            if focus_on_profiles {
                                let idx = profile_state.selected().unwrap_or(0);
                                let prev = idx.saturating_sub(1);
                                profile_state.select(Some(prev));
                                app.update_tags_for_profile(prev, global_config)?;
                            } else {
                                let idx = tag_state.selected().unwrap_or(0);
                                let prev = idx.saturating_sub(1);
                                tag_state.select(Some(prev));
                            }
                        }
                        KeyCode::Char(' ') => {
                            if !focus_on_profiles {
                                if let Some(idx) = tag_state.selected() {
                                    app.toggle_tag(idx, global_config)?;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            app.start_execution();
                        }
                        KeyCode::Left | KeyCode::Right => {
                            // No-op, handled by Tab
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw_apply(
    f: &mut ratatui::Frame,
    area: Rect,
    app: &App,
    profile_state: &mut ListState,
    tag_state: &mut ListState,
    focus_on_profiles: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(7),
            Constraint::Length(3),
        ])
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    let profile_items: Vec<ListItem> = app
        .profiles
        .iter()
        .map(|p| ListItem::new(p.clone()).style(Style::default().fg(YELLOW)))
        .collect();
    let tag_items: Vec<ListItem> = app
        .tags
        .iter()
        .map(|(t, checked)| {
            let mark = if *checked { "[x]" } else { "[ ]" };
            let color = if *checked { GREEN } else { BASE01 };
            ListItem::new(format!("{} {}", mark, t)).style(Style::default().fg(color))
        })
        .collect();
    let profiles = List::new(profile_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Profiles")
                .style(Style::default().fg(YELLOW).bg(BASE03)),
        )
        .highlight_style(
            Style::default()
                .fg(BASE03)
                .bg(YELLOW)
                .add_modifier(Modifier::BOLD),
        );
    let tags = List::new(tag_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Tags")
                .style(Style::default().fg(GREEN).bg(BASE03)),
        )
        .highlight_style(
            Style::default()
                .fg(BASE03)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        );
    if focus_on_profiles {
        f.render_stateful_widget(profiles, top_chunks[0], profile_state);
        f.render_widget(tags, top_chunks[1]);
    } else {
        f.render_widget(profiles, top_chunks[0]);
        f.render_stateful_widget(tags, top_chunks[1], tag_state);
    }

    let plan_items: Vec<ListItem> = app
        .execution_plan
        .iter()
        .map(|(desc, _)| ListItem::new(desc.clone()).style(Style::default().fg(CYAN)))
        .collect();
    let plan = List::new(plan_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Execution Plan")
                .style(Style::default().fg(CYAN).bg(BASE03)),
        )
        .style(Style::default());
    let plan_area = chunks[1];
    f.render_widget(plan, plan_area);
    // Render execution plan as a single Paragraph with border and wrapping
    let plan_text = app
        .execution_plan
        .iter()
        .map(|(desc, _)| desc.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let plan_paragraph = Paragraph::new(plan_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Execution Plan")
                .style(Style::default().fg(CYAN).bg(BASE03)),
        )
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(CYAN).bg(BASE03));
    f.render_widget(plan_paragraph, chunks[1]);

    let help = Paragraph::new("Tab: Switch | Space: Toggle Tag | Enter: Execute | q: Quit")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Help")
                .style(Style::default().fg(MAGENTA).bg(BASE02)),
        )
        .style(Style::default().fg(MAGENTA));
    f.render_widget(help, chunks[2]);
}

fn draw_execution(
    f: &mut ratatui::Frame,
    area: Rect,
    app: &App,
    _exec_state: &mut ListState,
    wrap_width: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    // Build wrapped lines and highlight the visual_cursor line
    let mut lines = Vec::new();
    for (desc, done) in &app.execution_plan {
        let mark = if *done { "[x]" } else { "[ ]" };
        let color = if *done { GREEN } else { BASE01 };
        let prefix = format!("{} ", mark);
        let mut first = true;
        for l in desc.lines() {
            let mut remaining = l;
            while !remaining.is_empty() {
                let mut width = 0;
                let mut take = 0;
                for (i, c) in remaining.char_indices() {
                    width += c.width().unwrap_or(1);
                    if width > wrap_width.saturating_sub(prefix.len()) {
                        break;
                    }
                    take = i + c.len_utf8();
                }
                let take = if take == 0 { remaining.len() } else { take };
                let (line, rest) = remaining.split_at(take);
                let mut content = String::new();
                if first {
                    content.push_str(&prefix);
                    first = false;
                } else {
                    content.push_str(&" ".repeat(prefix.len()));
                }
                content.push_str(line);
                lines.push((content, color));
                remaining = rest;
            }
        }
    }
    // Build Lines, highlight the visual_cursor line
    let mut text = Vec::new();
    for (i, (content, color)) in lines.iter().enumerate() {
        if i == app.visual_cursor {
            text.push(Line::from(vec![Span::styled(
                content.clone(),
                Style::default()
                    .fg(BASE03)
                    .bg(YELLOW)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else {
            text.push(Line::from(vec![Span::styled(
                content.clone(),
                Style::default().fg(*color).bg(BASE03),
            )]));
        }
    }
    let exec_paragraph = Paragraph::new(Text::from(text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Execution Progress")
                .style(Style::default().fg(GREEN).bg(BASE03)),
        )
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(GREEN).bg(BASE03));
    f.render_widget(exec_paragraph, chunks[0]);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("Progress")
                .style(Style::default().fg(MAGENTA).bg(BASE02)),
        )
        .gauge_style(Style::default().fg(MAGENTA).bg(BASE02))
        .percent(app.progress);
    f.render_widget(gauge, chunks[1]);

    let details = app
        .details
        .as_deref()
        .unwrap_or("Press Enter to view details. q: Quit, n: Next step");
    let details = Paragraph::new(details).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Details")
            .style(Style::default().fg(CYAN).bg(BASE03)),
    );
    f.render_widget(details, chunks[2]);

    // Highlight the item under the visual cursor
    // let width = (chunks[0].width as usize).saturating_sub(2); // account for border
    // let highlight_idx = get_exec_item_for_visual_cursor(app, wrap_width);
    // if let Some(item) = exec_state.selected() {
    //     exec_state.select(Some(highlight_idx));
    // }
}

// Helper functions for visual cursor mapping:
fn get_total_exec_lines(app: &App, width: usize) -> usize {
    use unicode_width::UnicodeWidthStr;
    let mut total = 0;
    for (desc, _) in &app.execution_plan {
        let lines = desc
            .lines()
            .flat_map(|l| {
                let w = UnicodeWidthStr::width(l);
                let mut n = w / width;
                if w % width != 0 || n == 0 {
                    n += 1;
                }
                std::iter::repeat_n((), n)
            })
            .count();
        total += lines.max(1);
    }
    total
}

fn get_exec_item_for_visual_cursor(app: &App, width: usize) -> usize {
    use unicode_width::UnicodeWidthStr;
    let mut line = 0;
    for (i, (desc, _)) in app.execution_plan.iter().enumerate() {
        let lines = desc
            .lines()
            .flat_map(|l| {
                let w = UnicodeWidthStr::width(l);
                let mut n = w / width;
                if w % width != 0 || n == 0 {
                    n += 1;
                }
                std::iter::repeat_n((), n)
            })
            .count();
        let lines = lines.max(1);
        if app.visual_cursor < line + lines {
            return i;
        }
        line += lines;
    }
    app.execution_plan.len().saturating_sub(1)
}
