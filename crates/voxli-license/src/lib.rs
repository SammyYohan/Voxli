//! `voxli-license` — vérification de licence locale signée (ADR-08, RF-38).
//!
//! Fonctionnement :
//! - la licence est un JSON signé **Ed25519** ; la vérification est hors
//!   ligne avec une clé **publique** embarquée dans le binaire ;
//! - aucune clé privée ni secret de signature n'existe dans le dépôt ni dans
//!   l'artefact (test T-38 / S3 : recherche de motifs dans les binaires) ;
//! - période de grâce hors ligne configurable (défaut 30 jours).

pub mod pubkey;

pub use pubkey::{embedded_public_key, EMBEDDED_PUBLIC_KEY_HEX, PRODUCT, PURCHASABLE_FEATURES};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
use voxli_core::Feature;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Format de blob de licence : deux lignes, payload JSON base64 puis
/// signature Ed25519 base64.
pub const BLOB_FORMAT: &str = "voxli-license-v1";

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("format de licence inconnu")]
    BadFormat,
    #[error("payload ou signature illisible")]
    Decode,
    #[error("signature invalide : licence altérée ou d'un autre produit")]
    BadSignature,
    #[error("licence expirée depuis {days} jours")]
    Expired { days: u64 },
    #[error("licence d'un autre produit : {0}")]
    WrongProduct(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LicenseDoc {
    /// Identifiant produit — la vérification refuse tout autre produit.
    pub product: String,
    pub licensee: String,
    /// Noms de fonctionnalités (voir `Feature::as_str`).
    pub features: Vec<String>,
    /// RFC 3339 (UTC).
    pub issued_at: String,
    /// `None` = perpétuelle (fenêtre de mises à jour gérée côté produit).
    pub expires_at: Option<String>,
    pub seats: Option<u32>,
}

/// Droits dérivés d'une licence vérifiée.
#[derive(Clone, Debug)]
pub struct Entitlement {
    features: Vec<String>,
    expires_at_secs: Option<u64>,
    licensee: String,
}

impl Entitlement {
    pub fn free() -> Self {
        Self {
            features: Vec::new(),
            expires_at_secs: None,
            licensee: "gratuit".into(),
        }
    }

    pub fn licensee(&self) -> &str {
        &self.licensee
    }

    /// Date de péremption en secondes depuis l'époque (une licence
    /// perpétuelle en est dépourvue).
    pub fn expires_at_secs(&self) -> Option<u64> {
        self.expires_at_secs
    }
}

impl voxli_core::Entitlement for Entitlement {
    fn feature(&self, f: Feature) -> bool {
        self.features.iter().any(|s| s == f.as_str())
    }
}

/// Clé publique embarquée dans le binaire (32 octets bruts).
pub fn public_key_from_bytes(bytes: [u8; PUBLIC_KEY_LENGTH]) -> VerifyingKey {
    VerifyingKey::from_bytes(&bytes).expect("clé publique embarquée invalide (invariant)")
}

/// Vérification complète : format, signature, produit, expiration.
/// Fonction **pure** sur des octets — testable hors ligne (docs/05 §2.6).
pub fn verify(
    blob: &str,
    public_key: &VerifyingKey,
    product: &str,
    now: SystemTime,
    grace: Duration,
) -> Result<Entitlement, LicenseError> {
    let mut lines = blob.trim().lines();
    let header = lines.next().ok_or(LicenseError::BadFormat)?.trim();
    if header != BLOB_FORMAT && header != "plume-license-v1" {
        return Err(LicenseError::BadFormat);
    }
    let payload_b64 = lines.next().ok_or(LicenseError::BadFormat)?.trim();
    let sig_b64 = lines.next().ok_or(LicenseError::BadFormat)?.trim();

    let payload_bytes = b64_decode(payload_b64).ok_or(LicenseError::Decode)?;
    let sig_bytes = b64_decode(sig_b64).ok_or(LicenseError::Decode)?;
    let sig_array: [u8; SIGNATURE_LENGTH] =
        sig_bytes.try_into().map_err(|_| LicenseError::Decode)?;
    let signature = Signature::from_bytes(&sig_array);

    use ed25519_dalek::Verifier;
    public_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| LicenseError::BadSignature)?;

    let payload: String = String::from_utf8(payload_bytes).map_err(|_| LicenseError::Decode)?;
    let doc: LicenseDoc = serde_json::from_str(&payload).map_err(|_| LicenseError::Decode)?;

    if doc.product != product {
        return Err(LicenseError::WrongProduct(doc.product));
    }

    let now_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Some(exp) = &doc.expires_at {
        let exp_secs = parse_rfc3339_secs(exp).ok_or(LicenseError::Decode)?;
        if now_secs > exp_secs + grace.as_secs() {
            return Err(LicenseError::Expired {
                days: (now_secs - exp_secs) / 86_400,
            });
        }
        return Ok(Entitlement {
            features: doc.features,
            expires_at_secs: Some(exp_secs),
            licensee: doc.licensee,
        });
    }

    Ok(Entitlement {
        features: doc.features,
        expires_at_secs: None,
        licensee: doc.licensee,
    })
}

/// Sérialise un blob signé (outil d'émission, hors dépôt produit).
pub fn sign(doc: &LicenseDoc, signing_key: &ed25519_dalek::SigningKey) -> String {
    let payload = serde_json::to_vec(doc).expect("sérialisation licence");
    let sig = signing_key.sign(&payload);
    format!(
        "{BLOB_FORMAT}\n{}\n{}\n",
        b64_encode(&payload),
        b64_encode(&sig.to_bytes())
    )
}

// ------------------------------------------------------------ helpers -----

/// Base64 variante standard, sans dépendance externe (≈ 30 lignes, testée).
/// Public : sert aussi à l'outil d'émission hors dépôt et aux tests.
pub fn b64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s.trim().bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let mut n: u32 = 0;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Analyse RFC 3339 minimaliste (`YYYY-MM-DDTHH:MM:SSZ`) — sufisant pour les
/// dates de licence, générées par l'émetteur.
pub fn parse_rfc3339_secs(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    // Jours depuis l'époque (algorithme de Howard Hinnant, domaine public).
    let days = {
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (month + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    };
    Some(((days * 86_400) + hour * 3600 + min * 60 + sec) as u64)
}
