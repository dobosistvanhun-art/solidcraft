//! A thin loft may save `sheet` (a sheet metal rule's name, empty for the active rule) and
//! `bend_step` (an angle expression): a sheet metal loft, its wall a sheet of that rule, ripped
//! open, with a flat pattern. Older designs need no transformation: a thin loft without `sheet`
//! is the plain thin loft, as before. The last release refuses a design saved with these (the
//! newer format number).
