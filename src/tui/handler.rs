use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Screen};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Delete confirmation mode
    if app.confirm_delete.is_some() {
        match key.code {
            KeyCode::Char('y') => {
                app.delete_selected();
                app.confirm_delete = None;
            }
            _ => {
                app.confirm_delete = None;
            }
        }
        return;
    }

    // Search input mode
    if app.screen == Screen::Search {
        match key.code {
            KeyCode::Esc => {
                app.screen = Screen::List;
                app.selected = 0;
            }
            KeyCode::Backspace => {
                app.search_input.pop();
                app.run_search();
            }
            KeyCode::Char(c) => {
                app.search_input.push(c);
                app.run_search();
            }
            KeyCode::Enter => {
                if let Some(id) = app.selected_observation_id() {
                    app.screen = Screen::Detail(id);
                }
            }
            KeyCode::Up | KeyCode::Down => {
                if key.code == KeyCode::Up {
                    app.move_up();
                } else {
                    app.move_down();
                }
            }
            _ => {}
        }
        return;
    }

    // Detail view
    if let Screen::Detail(_) = app.screen {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                app.screen = Screen::List;
            }
            _ => {}
        }
        return;
    }

    // Stats view
    if app.screen == Screen::Stats {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                app.screen = Screen::List;
            }
            _ => {}
        }
        return;
    }

    // List view (default)
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => app.move_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_up(),
        KeyCode::Char('/') | KeyCode::Char('2') => {
            app.screen = Screen::Search;
            app.selected = 0;
            app.search_input.clear();
            app.search_results.clear();
        }
        KeyCode::Char('3') => {
            app.refresh_stats();
            app.screen = Screen::Stats;
        }
        KeyCode::Char('1') => {
            app.screen = Screen::List;
            app.selected = 0;
            app.refresh_list();
        }
        KeyCode::Enter => {
            if let Some(id) = app.selected_observation_id() {
                app.screen = Screen::Detail(id);
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = app.selected_observation_id() {
                app.confirm_delete = Some(id);
            }
        }
        _ => {}
    }
}
