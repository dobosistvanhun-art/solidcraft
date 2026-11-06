//! About this chain: every catalogue chain says what it is, and the sprocket dialog shows it
//! with the chain's sketch.

use solidcraft_engine::doc::gear::{CHAINS, ChainFamily};

use super::*;

#[test]
fn every_chain_says_what_it_is_and_where_it_is_used() {
    for c in CHAINS {
        assert!(!family_text(c).is_empty(), "{}", c.name);
        assert!(!use_text(c).is_empty(), "{}", c.name);
        assert_ne!(ChainFamily::of(c), ChainFamily::Custom, "{}", c.name);
    }
    assert_eq!(mm(15.875), "15.875 mm");
    assert_eq!(mm(9.4), "9.4 mm");
}

#[test]
fn the_sprocket_dialog_shows_the_chain_and_its_sketch() {
    let mut h = crate::scenario::Harness::new();
    h.app.start("solid.gear.sprocket");
    h.frames(4);
    assert!(h.app.dialog.is_some(), "the sprocket dialog opened");
    assert!(crate::scenario::handle_at("dialog:section:About this chain").is_some(), "About this chain is under Chain");
    assert!(crate::scenario::rect_of("dialog:chain_sketch").is_some(), "the chain's sketch is drawn");
}
