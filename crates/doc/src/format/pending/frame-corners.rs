//! A frame member feature may save `corners` (none | miter | butt: how members are cut where two
//! lines meet at their ends; also, unless none, a member ending on another line is cut to fit
//! against it), `butt_flip` and `joints` (corners cut differently: `{lines, trim, through?}`).
//! Older designs need no transformation: without them the members run square from end to end,
//! as before. The last release refuses a design saved with these (the newer format number).
