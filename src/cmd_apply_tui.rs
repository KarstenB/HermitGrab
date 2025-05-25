// cmd_apply_tui.rs
// TUI for interactive apply using ratatui

use crate::cmd_apply;
use crate::config::{GlobalConfig, Tag};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph};
use std::collections::BTreeSet;
use std::io;

struct App {
    profiles: Vec<String>,
    tags: Vec<(Tag, bool)>,
    execution_plan: Vec<(String, bool)>,
    show_execution: bool,
    progress: u16,
    details: Option<String>,
}

impl App {
    fn update_tags_for_profile(&mut self, idx: usize, global_config: &GlobalConfig) {
        if let Some(profile_tags) = global_config.all_profiles.get(&self.profiles[idx]) {
            for (tag, checked) in &mut self.tags {
                *checked = profile_tags.contains(tag);
            }
        }
        self.update_execution_plan(global_config);
    }

    fn toggle_tag(&mut self, idx: usize, global_config: &GlobalConfig) {
        if let Some(tag) = self.tags.get_mut(idx) {
            tag.1 = !tag.1;
        }
        self.update_execution_plan(global_config);
    }

    fn update_execution_plan(&mut self, global_config: &GlobalConfig) {
        let active_tags = self.tags.iter().filter_map(|(t, checked)| if *checked { Some(t.clone()) } else { None }).collect::<BTreeSet<Tag>>();
        let actions = cmd_apply::create_actions(global_config).unwrap_or_default();
        let filtered = cmd_apply::filter_actions_by_tags(&actions, &active_tags);
        self.execution_plan = filtered.iter().map(|a| (a.short_description(), false)).collect::<Vec<_>>();
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

pub(crate) fn run_tui(global_config: &GlobalConfig, cli: &crate::Cli) -> Result<(), std::io::Error> {
    use std::collections::BTreeSet;
    use crate::cmd_apply;
    use crate::config::Tag;
    // Collect all profiles and tags from GlobalConfig
    let profiles: Vec<String> = global_config.all_profiles.keys().cloned().collect();
    let _tags: Vec<Tag> = global_config.all_tags.iter().cloned().collect();
    // Compute initial active tags (profile + cli tags)
    let mut detected_tags = BTreeSet::new();
    for t in &cli.tags {
        detected_tags.insert(Tag::from(t.as_str()));
    }
    let profile_to_use = if let Some(profile) = &cli.profile {
        Some(profile.to_lowercase())
    } else if global_config.all_profiles.contains_key("default") {
        Some("default".to_string())
    } else {
        None
    };
    let mut active_tags = detected_tags.clone();
    if let Some(profile) = profile_to_use {
        if let Some(profile_tags) = global_config.all_profiles.get(&profile) {
            active_tags.extend(profile_tags.iter().cloned());
        }
    }
    let actions = cmd_apply::create_actions(global_config).unwrap_or_default();
    let filtered_actions = cmd_apply::filter_actions_by_tags(&actions, &active_tags);
    let sorted = cmd_apply::topological_sort(filtered_actions);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App {
        profiles,
        tags: active_tags.into_iter().map(|t| (t, false)).collect(),
        execution_plan: sorted.iter().map(|a| (a.short_description(), false)).collect(),
        show_execution: false,
        progress: 0,
        details: None,
    };
    let mut tag_state = ListState::default();
    tag_state.select(Some(0));
    let mut profile_state = ListState::default();
    profile_state.select(Some(0));
    let mut exec_state = ListState::default();
    exec_state.select(Some(0));
    let mut focus_on_profiles = true;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            if app.show_execution {
                draw_execution(f, area, &app, &mut exec_state);
            } else {
                draw_apply(f, area, &app, &mut profile_state, &mut tag_state, focus_on_profiles);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Exit on q, Ctrl+C, or Esc
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) ||
                   (key.code == KeyCode::Char('c') && key.modifiers.contains(event::KeyModifiers::CONTROL)) {
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
                            let idx = exec_state.selected().unwrap_or(0);
                            let next = (idx + 1).min(app.execution_plan.len() - 1);
                            exec_state.select(Some(next));
                        }
                        KeyCode::Up => {
                            let idx = exec_state.selected().unwrap_or(0);
                            let prev = idx.saturating_sub(1);
                            exec_state.select(Some(prev));
                        }
                        KeyCode::Enter => {
                            let idx = exec_state.selected().unwrap_or(0);
                            app.details = Some(format!("Details for {}", app.execution_plan[idx].0));
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
                                app.update_tags_for_profile(next, global_config);
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
                                app.update_tags_for_profile(prev, global_config);
                            } else {
                                let idx = tag_state.selected().unwrap_or(0);
                                let prev = idx.saturating_sub(1);
                                tag_state.select(Some(prev));
                            }
                        }
                        KeyCode::Char(' ') => {
                            if !focus_on_profiles {
                                if let Some(idx) = tag_state.selected() {
                                    app.toggle_tag(idx, global_config);
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
        .map(|p| ListItem::new(p.clone()))
        .collect();
    let tag_items: Vec<ListItem> = app
        .tags
        .iter()
        .map(|(t, checked)| {
            let mark = if *checked { "[x]" } else { "[ ]" };
            ListItem::new(format!("{} {}", mark, t))
        })
        .collect();
    let profiles = List::new(profile_items)
        .block(Block::default().borders(Borders::ALL).title("Profiles"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    let tags = List::new(tag_items)
        .block(Block::default().borders(Borders::ALL).title("Tags"))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
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
        .map(|(desc, _)| ListItem::new(desc.clone()))
        .collect();
    let plan = List::new(plan_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Execution Plan"),
    );
    f.render_widget(plan, chunks[1]);

    let help = Paragraph::new("Tab: Switch | Space: Toggle Tag | Enter: Execute | q: Quit")
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[2]);
}

fn draw_execution(
    f: &mut ratatui::Frame,
    area: Rect,
    app: &App,
    exec_state: &mut ListState,
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

    let exec_items: Vec<ListItem> = app
        .execution_plan
        .iter()
        .map(|(desc, done)| {
            let mark = if *done { "[x]" } else { "[ ]" };
            ListItem::new(format!("{} {}", mark, desc))
        })
        .collect();
    let exec = List::new(exec_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Execution Progress"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(exec, chunks[0], exec_state);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(Style::default().fg(Color::Magenta).bg(Color::Black))
        .percent(app.progress);
    f.render_widget(gauge, chunks[1]);

    let details = app
        .details
        .as_deref()
        .unwrap_or("Press Enter to view details. q: Quit, n: Next step");
    let details =
        Paragraph::new(details).block(Block::default().borders(Borders::ALL).title("Details"));
    f.render_widget(details, chunks[2]);
}
