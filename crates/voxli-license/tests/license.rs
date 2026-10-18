use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use voxli_core::Entitlement;
use voxli_license::*;

const PRODUCT: &str = "voxli";

fn test_keypair() -> (SigningKey, ed25519_dalek::VerifyingKey) {
    // Graine fixe : aucune dépendance à `rand`, tests déterministes.
    let sk = SigningKey::from_bytes(&[42u8; 32]);
    let vk = sk.verifying_key();
    (sk, vk)
}

fn doc() -> LicenseDoc {
    LicenseDoc {
        product: PRODUCT.into(),
        licensee: "Cabinet Test".into(),
        features: vec!["history-on-disk".into()],
        issued_at: "2026-10-17T00:00:00Z".into(),
        expires_at: None,
        seats: Some(1),
    }
}

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_000_000) // 17/10/2026 env.
}

#[test]
fn t_valid_license_accepted() {
    let (sk, vk) = test_keypair();
    let blob = sign(&doc(), &sk);
    let ent = verify(&blob, &vk, PRODUCT, now(), Duration::ZERO).unwrap();
    assert!(ent.feature(voxli_core::Feature::HistoryOnDisk));
    assert!(!ent.feature(voxli_core::Feature::CloudPostProcessing));
    assert_eq!(ent.licensee(), "Cabinet Test");
}

#[test]
fn t_altered_byte_rejected() {
    let (sk, vk) = test_keypair();
    let blob = sign(&doc(), &sk);
    // Altérer un octet du payload sans casser la structure base64 : remplacer
    // un caractère du milieu par un autre caractère de l'alphabet base64
    // (le payload est encodé : on altère par position, pas par contenu).
    let mut altered = blob.clone();
    let mid = altered.len() / 2;
    let c = altered.as_bytes()[mid] as char;
    let sub = if c == 'A' { 'B' } else { 'A' };
    altered.replace_range(mid..mid + 1, &sub.to_string());
    // Si la substitution casse le base64, altérer la signature plutôt :
    let ok = verify(&altered, &vk, PRODUCT, now(), Duration::ZERO).is_err();
    assert!(ok, "une licence altérée doit être refusée");
    // Altération directe de la signature :
    let mut sig_flipped = blob.clone();
    let last = sig_flipped.len() - 2;
    sig_flipped.replace_range(
        last..last + 1,
        if sig_flipped.as_bytes()[last] == b'A' {
            "B"
        } else {
            "A"
        },
    );
    assert!(verify(&sig_flipped, &vk, PRODUCT, now(), Duration::ZERO).is_err());
}

#[test]
fn t_other_product_rejected() {
    let (sk, vk) = test_keypair();
    let blob = sign(&doc(), &sk);
    assert!(matches!(
        verify(&blob, &vk, "autre-produit", now(), Duration::ZERO),
        Err(LicenseError::WrongProduct(_))
    ));
}

#[test]
fn t_expired_rejected_without_grace() {
    let (sk, vk) = test_keypair();
    let mut d = doc();
    d.expires_at = Some("2026-01-01T00:00:00Z".into());
    let blob = sign(&d, &sk);
    assert!(matches!(
        verify(&blob, &vk, PRODUCT, now(), Duration::ZERO),
        Err(LicenseError::Expired { .. })
    ));
}

#[test]
fn t_grace_period_allows_offline_use() {
    let (sk, vk) = test_keypair();
    let mut d = doc();
    // Expirée depuis 5 jours ; grâce de 30 jours (RF-38).
    d.expires_at = Some("2026-10-12T00:00:00Z".into());
    let blob = sign(&d, &sk);
    assert!(verify(&blob, &vk, PRODUCT, now(), Duration::from_secs(30 * 86_400)).is_ok());
}

#[test]
fn t_bad_format_rejected_cleanly() {
    let (_, vk) = test_keypair();
    for bad in ["", "pas-un-blob", "voxli-license-v1\nxx\nyy"] {
        assert!(verify(bad, &vk, PRODUCT, now(), Duration::ZERO).is_err());
    }
}

#[test]
fn t_base64_round_trip() {
    for len in [0usize, 1, 2, 3, 10, 64, 255] {
        let data: Vec<u8> = (0..len as u8).collect();
        let enc = b64_encode(&data);
        let dec = b64_decode(&enc).unwrap();
        assert_eq!(dec, data, "longueur {len}");
    }
}

#[test]
fn t_rfc3339_parse() {
    assert_eq!(parse_rfc3339_secs("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(
        parse_rfc3339_secs("2026-01-01T00:00:00Z"),
        Some(1_767_225_600)
    );
    assert_eq!(parse_rfc3339_secs("n'importe quoi"), None);
}
