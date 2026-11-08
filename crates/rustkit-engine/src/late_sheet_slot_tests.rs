//! Portable S6 (#877) / CH2.E20 (#984) document-order assembly of the
//! external stylesheets.
//!
//! The headless companions wait on real fetches. These pin
//! `sheets_in_link_order`, which keeps cascade order when a middle sheet
//! arrives after the load's budget (the bug that left microsoft-scale pages
//! unstyled when a slow sheet sat between two fast ones), gives a link its
//! sheet wherever it stands, and makes the same URL in two links two sheets.

use super::sheets_in_link_order;
use rustkit_dom::NodeId;
use std::collections::HashMap;
use url::Url;

fn link(id: usize, path: &str) -> (NodeId, Url) {
    (NodeId::new(id), Url::parse(&format!("http://sheets.test{path}")).unwrap())
}

fn fetched(entries: &[(&str, Option<&'static str>)]) -> HashMap<Url, Option<&'static str>> {
    entries
        .iter()
        .map(|(path, sheet)| (Url::parse(&format!("http://sheets.test{path}")).unwrap(), *sheet))
        .collect()
}

#[test]
fn a_middle_late_sheet_lands_between_its_neighbours() {
    let links = [link(1, "/0.css"), link(2, "/1.css"), link(3, "/2.css")];
    let early = fetched(&[("/0.css", Some("fast-0")), ("/2.css", Some("fast-2"))]);
    assert_eq!(sheets_in_link_order(&links, &early), vec!["fast-0", "fast-2"]);
    let mut all = early;
    all.insert(Url::parse("http://sheets.test/1.css").unwrap(), Some("late-1"));
    assert_eq!(sheets_in_link_order(&links, &all), vec!["fast-0", "late-1", "fast-2"]);
}

#[test]
fn a_link_cascades_where_it_stands_not_where_it_was_found() {
    // The load found /b.css; a script then put a link to /a.css before it.
    let links = [link(9, "/a.css"), link(1, "/b.css")];
    let got = sheets_in_link_order(&links, &fetched(&[("/b.css", Some("b")), ("/a.css", Some("a"))]));
    assert_eq!(got, vec!["a", "b"]);
}

#[test]
fn the_same_url_in_two_links_is_two_sheets_and_a_failed_fetch_none() {
    let links = [link(1, "/a.css"), link(2, "/bad.css"), link(3, "/a.css")];
    let got = sheets_in_link_order(&links, &fetched(&[("/a.css", Some("a")), ("/bad.css", None)]));
    assert_eq!(got, vec!["a", "a"]);
}

#[test]
fn a_link_whose_sheet_has_not_landed_and_no_links_at_all() {
    let links = [link(1, "/slow.css")];
    assert!(sheets_in_link_order(&links, &fetched(&[])).is_empty());
    assert!(sheets_in_link_order(&[], &fetched(&[("/a.css", Some("a"))])).is_empty());
}
