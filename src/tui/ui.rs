use ratatui::prelude::*;
use ratatui::widgets::*;

use super::{App, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(5),    // content
            Constraint::Length(2), // footer
        ])
        .split(frame.area());

    draw_tabs(frame, app, chunks[0]);

    match &app.screen {
        Screen::List => draw_list(frame, app, chunks[1]),
        Screen::Detail(id) => draw_detail(frame, app, *id, chunks[1]),
        Screen::Search => draw_search(frame, app, chunks[1]),
        Screen::Stats => draw_stats(frame, app, chunks[1]),
    }

    draw_footer(frame, app, chunks[2]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["1:Recents", "2:Search", "3:Stats"];
    let selected = match app.screen {
        Screen::List | Screen::Detail(_) => 0,
        Screen::Search => 1,
        Screen::Stats => 2,
    };
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Igris Memory "),
        )
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .observations
        .iter()
        .map(|obs| {
            Row::new(vec![
                Cell::from(obs.id.to_string()),
                Cell::from(obs.observation_type.as_str()),
                Cell::from(obs.title.as_str()),
                Cell::from(obs.project.as_deref().unwrap_or("-")),
                Cell::from(obs.updated_at.as_str()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(14),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(22),
        ],
    )
    .header(
        Row::new(vec!["ID", "Type", "Title", "Project", "Updated"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(Block::default().borders(Borders::ALL).title(" Memories "))
    .row_highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
    .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if !app.observations.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_detail(frame: &mut Frame, app: &App, id: i64, area: Rect) {
    let obs = match app.db.get_observation(id) {
        Ok(o) => o,
        Err(_) => {
            let p = Paragraph::new("Observation not found")
                .block(Block::default().borders(Borders::ALL).title(" Detail "));
            frame.render_widget(p, area);
            return;
        }
    };

    let tags_str = obs
        .tags
        .as_ref()
        .map(|t| t.join(", "))
        .unwrap_or_else(|| "-".to_string());

    let text = vec![
        Line::from(vec![
            Span::styled("ID: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(obs.id.to_string()),
            Span::raw("  "),
            Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&obs.observation_type),
        ]),
        Line::from(vec![
            Span::styled("Project: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(obs.project.as_deref().unwrap_or("-")),
            Span::raw("  "),
            Span::styled("Scope: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&obs.scope),
        ]),
        Line::from(vec![
            Span::styled("Topic: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(obs.topic_key.as_deref().unwrap_or("-")),
        ]),
        Line::from(vec![
            Span::styled("Tags: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&tags_str),
        ]),
        Line::from(vec![
            Span::styled("Created: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&obs.created_at),
            Span::raw("  "),
            Span::styled("Updated: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&obs.updated_at),
        ]),
        Line::from(vec![
            Span::styled("Revisions: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(obs.revision_count.to_string()),
            Span::raw("  "),
            Span::styled(
                "Duplicates: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(obs.duplicate_count.to_string()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "─── Content ───",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    let mut lines = text;
    for line in obs.content.lines() {
        lines.push(Line::from(line.to_string()));
    }

    let title = format!(" {} ", obs.title);
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((0, 0));
    frame.render_widget(paragraph, area);
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Search input
    let input = Paragraph::new(app.search_input.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search (type to filter) "),
        )
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(input, chunks[0]);
    // Set cursor position
    frame.set_cursor_position((
        chunks[0].x + app.search_input.len() as u16 + 1,
        chunks[0].y + 1,
    ));

    // Results
    let rows: Vec<Row> = app
        .search_results
        .iter()
        .map(|r| {
            let obs = &r.observation;
            Row::new(vec![
                Cell::from(obs.id.to_string()),
                Cell::from(obs.observation_type.as_str()),
                Cell::from(obs.title.as_str()),
                Cell::from(obs.project.as_deref().unwrap_or("-")),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(14),
            Constraint::Min(20),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(vec!["ID", "Type", "Title", "Project"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Results ({}) ", app.search_results.len())),
    )
    .row_highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan))
    .highlight_symbol("▶ ");

    let mut state = TableState::default();
    if !app.search_results.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, chunks[1], &mut state);
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    let stats = match &app.stats {
        Some(s) => s,
        None => {
            let p = Paragraph::new("Loading...")
                .block(Block::default().borders(Borders::ALL).title(" Stats "));
            frame.render_widget(p, area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // By type
    let mut type_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Total: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(stats.total_observations.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Sessions: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "{} total, {} active",
                stats.total_sessions, stats.active_sessions
            )),
        ]),
        Line::from(""),
    ];
    let mut types: Vec<_> = stats.by_type.iter().collect();
    types.sort_by(|a, b| b.1.cmp(a.1));
    for (t, c) in &types {
        type_lines.push(Line::from(format!("  {t}: {c}")));
    }
    let type_block =
        Paragraph::new(type_lines).block(Block::default().borders(Borders::ALL).title(" By Type "));
    frame.render_widget(type_block, chunks[0]);

    // By project
    let mut proj_lines = Vec::new();
    let mut projects: Vec<_> = stats.by_project.iter().collect();
    projects.sort_by(|a, b| b.1.cmp(a.1));
    for (p, c) in &projects {
        proj_lines.push(Line::from(format!("  {p}: {c}")));
    }
    let proj_block = Paragraph::new(proj_lines)
        .block(Block::default().borders(Borders::ALL).title(" By Project "));
    frame.render_widget(proj_block, chunks[1]);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let text = if app.confirm_delete.is_some() {
        "Delete this observation? (y/n)"
    } else {
        match app.screen {
            Screen::List => "↑↓/jk:navigate  Enter:detail  d:delete  /:search  1-3:tabs  q:quit",
            Screen::Detail(_) => "Esc/q:back",
            Screen::Search => "Type to search  ↑↓:navigate  Enter:detail  Esc:back",
            Screen::Stats => "Esc/q:back",
        }
    };

    let style = if app.confirm_delete.is_some() {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let footer = Paragraph::new(text)
        .style(style)
        .alignment(Alignment::Center);
    frame.render_widget(footer, area);
}
