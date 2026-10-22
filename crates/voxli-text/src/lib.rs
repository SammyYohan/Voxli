//! `voxli-text` — text insertion on Windows.
//!
//! Public interface: `Inserter`, `InsertPlan`, `InsertReport`,
//! `FocusSnapshot` (read-only view of capabilities) — docs/05 §2.4.
//!
//! - `plan_insertion` is a **pure function** from `(field capabilities,
//!   profile) → level order`: fully testable outside Windows;
//! - the real execution (`WindowsInserter`, `WindowsFocusReader`) is covered
//!   by the instrumented manual acceptance procedure (matrix of 12 apps,
//!   `tests/e2e/matrice-12-apps.md`).

#[cfg(target_os = "windows")]
pub mod focus;
#[cfg(target_os = "windows")]
pub mod inserter;
pub mod plan;
pub mod post;

#[cfg(target_os = "windows")]
pub use focus::WindowsFocusReader;
#[cfg(target_os = "windows")]
pub use inserter::WindowsInserter;
pub use plan::plan_insertion;
pub use post::{apply_dictionary, apply_french_rules, FrenchProcessor, PassThroughProcessor};
