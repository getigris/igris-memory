mod handler;
mod ui;

use crate::db::Database;
use crate::models::{Observation, SearchResult, Stats};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use std::io;

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    List,
    Detail(i64),
    Search,
    Stats,
}

pub struct App {
    pub db: Database,
    pub screen: Screen,
    pub observations: Vec<Observation>,
    pub selected: usize,
    pub search_input: String,
    pub search_results: Vec<SearchResult>,
    pub stats: Option<Stats>,
    pub should_quit: bool,
    pub confirm_delete: Option<i64>,
}

impl App {
    pub fn new(db: Database) -> Self {
        let mut app = Self {
            db,
            screen: Screen::List,
            observations: Vec::new(),
            selected: 0,
            search_input: String::new(),
            search_results: Vec::new(),
            stats: None,
            should_quit: false,
            confirm_delete: None,
        };
        app.refresh_list();
        app
    }

    pub fn refresh_list(&mut self) {
        self.observations = self.db.recent_context(None, Some(50)).unwrap_or_default();
    }

    pub fn refresh_stats(&mut self) {
        self.stats = self.db.stats().ok();
    }

    pub fn run_search(&mut self) {
        if self.search_input.trim().is_empty() {
            self.search_results.clear();
            return;
        }
        self.search_results = self
            .db
            .search(&self.search_input, None, None, Some(50))
            .unwrap_or_default();
    }

    pub fn delete_selected(&mut self) -> bool {
        let id = self.selected_observation_id();
        if let Some(id) = id
            && self.db.delete_observation(id).unwrap_or(false)
        {
            self.refresh_list();
            if self.selected > 0 && self.selected >= self.current_list_len() {
                self.selected = self.current_list_len().saturating_sub(1);
            }
            return true;
        }
        false
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let len = self.current_list_len();
        if len > 0 && self.selected < len - 1 {
            self.selected += 1;
        }
    }

    pub fn current_list_len(&self) -> usize {
        match self.screen {
            Screen::List => self.observations.len(),
            Screen::Search => self.search_results.len(),
            _ => 0,
        }
    }

    pub fn selected_observation_id(&self) -> Option<i64> {
        match self.screen {
            Screen::List => self.observations.get(self.selected).map(|o| o.id),
            Screen::Search => self
                .search_results
                .get(self.selected)
                .map(|r| r.observation.id),
            _ => None,
        }
    }
}

/// Run the TUI application.
pub fn run(db: Database) -> anyhow::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db);

    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                app.should_quit = true;
            } else {
                handler::handle_key(&mut app, key);
            }
        }
    }

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/tui_test.rs"]
mod tests;
