//! Post-traitement — niveaux 0 et 1 (RF-26).
//!
//! Niveau 0 (`PassThroughProcessor`) : identité — le texte du moteur est
//! inséré tel quel. C'est le **défaut produit** : aucune réécriture, mode
//! verbatim garanti (RF-27). Les règles locales déterministes (niveau 1 :
//! ponctuation française, espaces insécables, casse en début de phrase) et le
//! petit modèle local (niveau 2) arrivent en V1/P3 — même trait, même
//! point de couture. Le niveau 3 (cloud) est **jamais activé sans
//! consentement explicite et par utilisation** (RF-26, RF-36).

use plume_core::{PostError, PostProcessor, Profile, Style};

/// Niveau 0 : aucun post-traitement.
pub struct PassThroughProcessor;

impl PostProcessor for PassThroughProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError> {
        match profile.style {
            // Verbatim garanti : identité, jamais altérée.
            Style::Verbatim => Ok(text.to_string()),
            // Niveau 1 : les règles arrivent en V1 — comportement identité
            // assumé et documenté en P0.
            Style::Punctuated => Ok(text.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_verbatim_is_identity() {
        let p = PassThroughProcessor;
        let profile = Profile {
            style: Style::Verbatim,
            ..Default::default()
        };
        let out = p.process("  texte   brut  ", &profile).unwrap();
        assert_eq!(out, "  texte   brut  ");
    }

    #[test]
    fn t_punctuated_level_1_is_identity_in_p0() {
        let p = PassThroughProcessor;
        let profile = Profile::default();
        let out = p.process("texte", &profile).unwrap();
        assert_eq!(out, "texte");
    }
}
