//! A design may save `drawings`: sheets (size, scale, projection method, title block) holding
//! views of the model (base, projected, section and detail views), dimensions that measure model
//! points, and notes. The lines are not saved: they are projected from the model when a drawing
//! is shown or exported. Older designs need no transformation: they have no drawings. The last
//! release refuses a design saved with one (the newer format number).
