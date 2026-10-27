//! Drawings put together by hand: a dimension may carry `at` (where it was put, in its view's own
//! coordinates), a drawing `text_height` (its dimensions' text, mm) and a view `free` (a projected
//! or section view not kept in line with its parent). Older designs need no transformation; the
//! last release refuses a design saved with them (the newer format number).
