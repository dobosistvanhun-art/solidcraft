//! Cutting colours: a sketch curve can save `cut_colour` (an index of LightBurn's layer palette,
//! 0 to 29), the colour the cutting and nesting DXFs give the closed contour it is in, and a
//! nesting job's part can save `outer_colour` and `inner_colour` over its contours' own. Older
//! designs need no transformation (no colour: red part edges, blue holes, as before); the last
//! release refuses a design saved with this (the newer format number).
