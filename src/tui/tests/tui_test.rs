use super::*;
use crate::db::Database;

fn test_app_with_data() -> App {
    let db = Database::open_in_memory().expect("failed to create in-memory db");
    for i in 0..5 {
        db.save_observation(
            &format!("Obs {i}"),
            &format!("Content {i}"),
            "manual",
            Some("proj"),
            "project",
            None,
            None,
            None,
        )
        .unwrap();
    }
    App::new(db)
}

#[test]
fn app_loads_observations() {
    let app = test_app_with_data();
    assert_eq!(app.observations.len(), 5);
    assert_eq!(app.selected, 0);
    assert_eq!(app.screen, Screen::List);
}

#[test]
fn app_state_navigation() {
    let mut app = test_app_with_data();
    assert_eq!(app.selected, 0);

    app.move_down();
    assert_eq!(app.selected, 1);

    app.move_down();
    app.move_down();
    app.move_down();
    assert_eq!(app.selected, 4);

    // Can't go past end
    app.move_down();
    assert_eq!(app.selected, 4);

    app.move_up();
    assert_eq!(app.selected, 3);

    // Can't go before 0
    app.selected = 0;
    app.move_up();
    assert_eq!(app.selected, 0);
}

#[test]
fn app_search_filters() {
    let mut app = test_app_with_data();
    app.screen = Screen::Search;
    app.search_input = "Obs".to_string();
    app.run_search();
    assert!(!app.search_results.is_empty(), "search should find results");
}

#[test]
fn app_delete_marks_deleted() {
    let mut app = test_app_with_data();
    assert_eq!(app.observations.len(), 5);

    let deleted = app.delete_selected();
    assert!(deleted);
    assert_eq!(
        app.observations.len(),
        4,
        "list should refresh after delete"
    );
}

#[test]
fn app_empty_search_clears_results() {
    let mut app = test_app_with_data();
    app.screen = Screen::Search;
    app.search_input = "Obs".to_string();
    app.run_search();
    assert!(!app.search_results.is_empty());

    app.search_input.clear();
    app.run_search();
    assert!(app.search_results.is_empty());
}

#[test]
fn app_selected_observation_id() {
    let app = test_app_with_data();
    let id = app.selected_observation_id();
    assert!(id.is_some());
}
