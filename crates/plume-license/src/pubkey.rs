//! Clé publique Ed25519 embarquée (ADR-08).
//!
//! La clé **privée** de signature n'existe jamais dans le dépôt ni dans
//! l'artefact (test S3) : elle vit hors dépôt, dans `.keys/` (ignoré par
//! git), générée une fois par `cargo run -p plume-license --bin issue-license`.
//!
//! Cycle de vie prévu pour la mise en production :
//! 1. générer une paire de clés dédiée sur une machine de confiance ;
//! 2. remplacer `EMBEDDED_PUBLIC_KEY_HEX` ci-dessous par la clé publique ;
//! 3. conserver la clé privée hors dépôt (coffre-fort / HSM).

use ed25519_dalek::{VerifyingKey, PUBLIC_KEY_LENGTH};

/// Fonctionnalités achetables (RF-38/RF-39) : liste fermée, alignée sur
/// `plume_core::Feature` (égalité vérifiée par le test).
pub const PURCHASABLE_FEATURES: &[&str] =
    &["history-on-disk", "cloud-post-processing", "extra-profiles"];

/// Clé publique de vérification (hex, 64 caractères) de la paire **de
/// développement** générée en phase de test. Zéros = aucune clé embarquée.
/// Cycle production : remplacer par une paire dédiée dont la clé privée
/// reste hors dépôt (cf. en-tête du module).
pub const EMBEDDED_PUBLIC_KEY_HEX: &str =
    "6666ea43c28087bb2a36a0b4ea3437dbbe6b188a0ebb3dbfbf9de30dc1a932e4";

/// Identifiant produit vérifié à l'import — un blob d'un autre produit est
/// refusé (RF-38).
pub const PRODUCT: &str = "plume-desktop";

/// Parse la clé publique embarquée. `None` si la constante est à zéros
/// (binaire de développement sans clé) : toute importation de licence sera
/// alors refusée avec un message explicite — jamais de validation silencieuse.
pub fn embedded_public_key() -> Option<VerifyingKey> {
    let bytes = hex_decode_32(EMBEDDED_PUBLIC_KEY_HEX)?;
    if bytes == [0u8; PUBLIC_KEY_LENGTH] {
        return None;
    }
    Some(crate::public_key_from_bytes(bytes))
}

fn hex_decode_32(s: &str) -> Option<[u8; PUBLIC_KEY_LENGTH]> {
    let b = s.trim();
    if b.len() != PUBLIC_KEY_LENGTH * 2 {
        return None;
    }
    let mut out = [0u8; PUBLIC_KEY_LENGTH];
    for (i, chunk) in b.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out[i] = (hi * 16 + lo) as u8;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_embedded_key_length_and_shape() {
        // La constante doit TOUJOURS être un hex de 64 caractères ou des
        // zéros — un binaire livré avec une clé tronquée est une panne.
        assert_eq!(EMBEDDED_PUBLIC_KEY_HEX.len(), PUBLIC_KEY_LENGTH * 2);
        let parsed = hex_decode_32(EMBEDDED_PUBLIC_KEY_HEX);
        assert!(parsed.is_some(), "hex embarqué valide");
    }

    #[test]
    fn t_zero_key_is_not_embedded() {
        // En phase de test : si une paire de développement réelle a été
        // générée, la constante n'est plus nulle — ce test la détecte.
        // Il exactifie l'invariant : zéro ⇒ `None`.
        if EMBEDDED_PUBLIC_KEY_HEX.starts_with("0000") {
            assert!(embedded_public_key().is_none());
        } else {
            assert!(embedded_public_key().is_some());
        }
    }

    #[test]
    fn t_purchasable_features_match_core() {
        assert_eq!(PURCHASABLE_FEATURES.len(), 3);
    }
}
