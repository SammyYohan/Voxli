//! `plume-text` — insertion du texte dans Windows.
//!
//! Interface publique : `Inserter`, `InsertPlan`, `InsertReport`,
//! `FocusSnapshot` (lecture seule des capacités) — docs/05 §2.4.
//!
//! - `plan_insertion` est une **fonction pure** de `(capacités du champ,
//!   profil) → ordre des niveaux` : entièrement testable hors Windows ;
//! - l'exécution réelle (`WindowsInserter`, `WindowsFocusReader`) est couverte
//!   par la recette manuelle instrumentée (matrice de 12 applications,
//!   `tests/e2e/matrice-12-apps.md`).

#[cfg(target_os = "windows")]
pub mod focus;
pub mod inserter;
pub mod plan;
pub mod post;

#[cfg(target_os = "windows")]
pub use focus::WindowsFocusReader;
pub use inserter::WindowsInserter;
pub use plan::plan_insertion;
pub use post::{apply_dictionary, apply_french_rules, FrenchProcessor, PassThroughProcessor};
