//! A loft may save `thin` (`thickness`, `side`, `closed_start`, `closed_end`): the loft hollowed
//! to a wall of constant thickness, its end sections open. Older designs need no transformation:
//! a loft without `thin` is the solid loft, as before. The last release refuses a design saved
//! with this (the newer format number).
