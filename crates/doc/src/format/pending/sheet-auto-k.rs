//! Sheet-metal bodies whose rules set the K-factor to automatic keep the material class as `k_auto`,
//! so each bend works out its own K from its own radius (r/t) instead of one K for the whole body.
//! Older designs need no transformation (no `k_auto`: the stored `k` as before); the last release
//! refuses a design saved with it (the newer format number).
