//! # Navigation Tests
//!
//! Tests for the navigation system, History API, and page lifecycle.

use rustkit_core::{
    history::{HistoryEntry, HistoryState, NavigationType, SessionHistory},
    lifecycle::{DocumentReadyState, PageLifecycle, VisibilityState},
};
use url::Url;

/// Test history entry creation.
#[test]
fn test_history_entry() {
    let url = Url::parse("https://example.com/page").unwrap();
    let entry = HistoryEntry::new(url.clone(), "Test Page".into());

    assert_eq!(entry.url, url);
    assert_eq!(entry.title, "Test Page");
    assert!(entry.state.is_none());
    assert_eq!(entry.scroll_position.x, 0.0);
    assert_eq!(entry.scroll_position.y, 0.0);
}

/// Test history entry with state.
#[test]
fn test_history_entry_with_state() {
    let url = Url::parse("https://example.com/").unwrap();
    let entry = HistoryEntry::new(url, "Home".into())
        .with_state(HistoryState::string("test data"))
        .with_scroll_position(100.0, 200.0);

    assert!(entry.state.is_some());
    assert_eq!(entry.scroll_position.x, 100.0);
    assert_eq!(entry.scroll_position.y, 200.0);
}

/// Test session history push.
#[test]
fn test_session_history_push() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/1").unwrap();
    let url2 = Url::parse("https://example.com/2").unwrap();

    history.push(HistoryEntry::new(url1.clone(), "Page 1".into()));
    assert_eq!(history.length(), 1);
    assert_eq!(history.current_url(), Some(&url1));

    history.push(HistoryEntry::new(url2.clone(), "Page 2".into()));
    assert_eq!(history.length(), 2);
    assert_eq!(history.current_url(), Some(&url2));
}

/// Test session history push_state.
#[test]
fn test_session_history_push_state() {
    let mut history = SessionHistory::new();

    // Need an initial entry
    let url1 = Url::parse("https://example.com/").unwrap();
    history.push(HistoryEntry::new(url1, "Home".into()));

    // Push state with new URL
    let url2 = Url::parse("https://example.com/page").unwrap();
    let result = history.push_state(
        Some(HistoryState::string("state data")),
        "Page".into(),
        Some(url2.clone()),
    );

    assert!(result.is_ok());
    assert_eq!(history.length(), 2);
    assert_eq!(history.current_url(), Some(&url2));
}

/// Test session history replace_state.
#[test]
fn test_session_history_replace_state() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/").unwrap();
    history.push(HistoryEntry::new(url1, "Home".into()));

    let url2 = Url::parse("https://example.com/replaced").unwrap();
    let result = history.replace_state(None, "Replaced".into(), Some(url2.clone()));

    assert!(result.is_ok());
    assert_eq!(history.length(), 1); // Still one entry
    assert_eq!(history.current_url(), Some(&url2));
    assert_eq!(history.current_entry().unwrap().title, "Replaced");
}

/// Test cross-origin security in push_state.
#[test]
fn test_cross_origin_push_state_blocked() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/").unwrap();
    history.push(HistoryEntry::new(url1, "Home".into()));

    // Try to push to different origin - should fail
    let evil_url = Url::parse("https://evil.com/").unwrap();
    let result = history.push_state(None, "Evil".into(), Some(evil_url));

    assert!(result.is_err());
}

/// Test history back navigation.
#[test]
fn test_history_back() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/1").unwrap();
    let url2 = Url::parse("https://example.com/2").unwrap();
    let url3 = Url::parse("https://example.com/3").unwrap();

    history.push(HistoryEntry::new(url1.clone(), "1".into()));
    history.push(HistoryEntry::new(url2.clone(), "2".into()));
    history.push(HistoryEntry::new(url3, "3".into()));

    assert!(history.can_go_back());
    assert!(!history.can_go_forward());

    history.back();
    assert_eq!(history.current_url(), Some(&url2));

    history.back();
    assert_eq!(history.current_url(), Some(&url1));
    assert!(!history.can_go_back());
}

/// Test history forward navigation.
#[test]
fn test_history_forward() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/1").unwrap();
    let url2 = Url::parse("https://example.com/2").unwrap();

    history.push(HistoryEntry::new(url1.clone(), "1".into()));
    history.push(HistoryEntry::new(url2.clone(), "2".into()));

    history.back();
    assert_eq!(history.current_url(), Some(&url1));
    assert!(history.can_go_forward());

    history.forward();
    assert_eq!(history.current_url(), Some(&url2));
    assert!(!history.can_go_forward());
}

/// Test history go() with delta.
#[test]
fn test_history_go() {
    let mut history = SessionHistory::new();

    for i in 0..5 {
        let url = Url::parse(&format!("https://example.com/{}", i)).unwrap();
        history.push(HistoryEntry::new(url, format!("Page {}", i)));
    }

    assert_eq!(history.current_index(), 4);

    // Go back 3
    history.go(-3);
    assert_eq!(history.current_index(), 1);

    // Go forward 2
    history.go(2);
    assert_eq!(history.current_index(), 3);
}

/// Test scroll position saving.
#[test]
fn test_scroll_position_save() {
    let mut history = SessionHistory::new();

    let url = Url::parse("https://example.com/").unwrap();
    history.push(HistoryEntry::new(url, "Home".into()));

    history.save_scroll_position(500.0, 1000.0);

    let entry = history.current_entry().unwrap();
    assert_eq!(entry.scroll_position.x, 500.0);
    assert_eq!(entry.scroll_position.y, 1000.0);
}

/// Test navigation type properties.
#[test]
fn test_navigation_type() {
    assert!(NavigationType::Navigate.creates_entry());
    assert!(NavigationType::PushState.creates_entry());
    assert!(!NavigationType::Replace.creates_entry());
    assert!(!NavigationType::Reload.creates_entry());

    assert!(NavigationType::BackForward.is_history_navigation());
    assert!(NavigationType::Traverse.is_history_navigation());
    assert!(!NavigationType::Navigate.is_history_navigation());
}

/// Test page lifecycle state transitions.
#[test]
fn test_lifecycle_transitions() {
    let mut lifecycle = PageLifecycle::new();

    assert_eq!(lifecycle.ready_state(), DocumentReadyState::Loading);

    lifecycle.dom_content_loaded();
    assert_eq!(lifecycle.ready_state(), DocumentReadyState::Interactive);

    lifecycle.load_complete();
    assert_eq!(lifecycle.ready_state(), DocumentReadyState::Complete);
}

/// Test page visibility.
#[test]
fn test_page_visibility() {
    let mut lifecycle = PageLifecycle::new();

    assert_eq!(lifecycle.visibility_state(), VisibilityState::Visible);
    assert!(!lifecycle.hidden());

    lifecycle.set_visibility(VisibilityState::Hidden);
    assert_eq!(lifecycle.visibility_state(), VisibilityState::Hidden);
    assert!(lifecycle.hidden());
}

/// Test page freeze/resume.
#[test]
fn test_page_freeze_resume() {
    let mut lifecycle = PageLifecycle::new();

    assert!(!lifecycle.frozen());

    lifecycle.freeze();
    assert!(lifecycle.frozen());

    lifecycle.resume();
    assert!(!lifecycle.frozen());
}

/// Test try_unload without handler.
#[test]
fn test_try_unload() {
    let mut lifecycle = PageLifecycle::new();

    // Without beforeunload handler, should allow navigation
    assert!(lifecycle.try_unload());
}

/// Test ready state string representation.
#[test]
fn test_ready_state_strings() {
    assert_eq!(DocumentReadyState::Loading.as_str(), "loading");
    assert_eq!(DocumentReadyState::Interactive.as_str(), "interactive");
    assert_eq!(DocumentReadyState::Complete.as_str(), "complete");
}

/// Test visibility state string representation.
#[test]
fn test_visibility_state_strings() {
    assert_eq!(VisibilityState::Visible.as_str(), "visible");
    assert_eq!(VisibilityState::Hidden.as_str(), "hidden");
}

/// Test history state types.
#[test]
fn test_history_state_types() {
    let null = HistoryState::null();
    assert!(null.is_null());

    let string = HistoryState::string("test");
    assert!(!string.is_null());

    let object = HistoryState::object();
    assert!(!object.is_null());
}

/// Test history navigate method.
#[test]
fn test_history_navigate() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/").unwrap();
    history.navigate(url1.clone(), NavigationType::Navigate, "Home".into());

    assert_eq!(history.length(), 1);
    assert_eq!(history.current_url(), Some(&url1));

    let url2 = Url::parse("https://example.com/page").unwrap();
    history.navigate(url2.clone(), NavigationType::Navigate, "Page".into());

    assert_eq!(history.length(), 2);
    assert_eq!(history.current_url(), Some(&url2));
}

/// Test history replace navigation.
#[test]
fn test_history_navigate_replace() {
    let mut history = SessionHistory::new();

    let url1 = Url::parse("https://example.com/").unwrap();
    history.navigate(url1, NavigationType::Navigate, "Home".into());

    let url2 = Url::parse("https://example.com/replaced").unwrap();
    history.navigate(url2.clone(), NavigationType::Replace, "Replaced".into());

    assert_eq!(history.length(), 1); // Still one entry
    assert_eq!(history.current_url(), Some(&url2));
}

