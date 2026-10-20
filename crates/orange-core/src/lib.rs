//! orange-core: shared base types for Orange (kinds, dependency detection,
//! process execution, output naming, job context).
//!
//! FROZEN API: other crates build against this. Additive changes only.

pub mod ctx;
pub mod deps;
pub mod error;
pub mod exec;
pub mod kind;
pub mod paths;

pub use ctx::{CancelToken, Ctx, Progress};
pub use deps::{Capabilities, Dep};
pub use error::{Error, Result};
pub use kind::Kind;
