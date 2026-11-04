//! A perforation's holes can be square or rectangular as well as round: a `sheet_perforate` feature
//! saves an optional `shape` ("round", "square", "rectangle") and, for a rectangle, `height` (the
//! hole's size along the flat's y; `diameter` is then its width, and a square's side). Older designs
//! need no transformation (no `shape`: round holes as before); the last release refuses a design
//! saved with this (the newer format number).
