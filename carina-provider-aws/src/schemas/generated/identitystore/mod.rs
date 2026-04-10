//! Hand-written — not generated from Smithy
//!
//! The codegen pipeline's `ResourceDef` doesn't model "data source with
//! user-supplied lookup inputs", so `identitystore.user` is maintained
//! manually. The codegen script preserves this module as an orphan (see
//! carina-rs/carina-provider-aws commit 3f77a33).

// Re-export parent types so resource modules can use `super::` to access them.
pub use super::*;

pub mod user;
