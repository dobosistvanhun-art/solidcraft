//! A sheet metal rule may save its `material` (an id such as `dc01`, `1.4301` or `almg3`) and
//! `k_factor: "auto"` (the K-factor from the material's class and the bend radius over the
//! thickness); an edge flange may save `corner_gap`, the gap between its flanges where they meet
//! at a corner in place of the rule's. Older designs need no transformation: a rule without a
//! material names none, and a flange without `corner_gap` takes the rule's gap, as before. The
//! last release refuses a design saved with these (the newer format number).
