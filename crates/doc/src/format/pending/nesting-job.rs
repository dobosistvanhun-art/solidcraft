//! A design can keep a nesting job (`nesting`): the parts cut from material sheets (sketches, sheet
//! metal flat patterns, or contours imported from a DXF and kept with the job, with counts, grain
//! and turn limits), the sheet's size and margin, the gap and cut width, the quality, and the
//! layout found (each copy's sheet, place and angle, locked or not) with other layouts to choose
//! from. Older designs need no transformation (no `nesting`: no job); the last release refuses a
//! design saved with it (the newer format number).
