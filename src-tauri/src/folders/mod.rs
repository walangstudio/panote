//! Folders: nested, one folder per note.
//!
//! Names are encrypted at rest with the same framing tags use, so the tree can
//! be listed with the device key alone but a folder name never sits in the clear.

pub mod commands;
pub mod queries;

#[cfg(test)]
mod tests;

/// Bounds tree recursion, matching the cap the checklist editor uses for the
/// same reason. Enforced here rather than only in the UI: a deep or cyclic tree
/// would blow the stack while rendering.
pub const MAX_DEPTH: usize = 20;
