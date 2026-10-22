//! Embedded Ed25519 public key (ADR-08).
//!
//! The signing **private** key never exists in the repository or in the
//! artifact (test S3): it lives outside the repo, in `.keys/` (git-ignored),
//! generated once by `cargo run -p voxli-license --bin issue-license`.
//!
//! Planned go-to-production lifecycle:
//! 1. generate a dedicated key pair on a trusted machine;
//! 2. replace `EMBEDDED_PUBLIC_KEY_HEX` below with the public key;
//! 3. keep the private key outside the repo (safe / HSM).

use ed25519_dalek::{VerifyingKey, PUBLIC_KEY_LENGTH};

/// Purchasable features (RF-38/RF-39): a closed list, aligned on
/// `voxli_core::Feature` (equality checked by a test).
pub const PURCHASABLE_FEATURES: &[&str] =
    &["history-on-disk", "cloud-post-processing", "extra-profiles"];

/// Public verification key (hex, 64 characters) of the **development**
/// pair generated during the test phase. Zeros = no embedded key.
/// Production cycle: replace with a dedicated pair whose private key
/// stays out of the repo (cf. module header).
pub const EMBEDDED_PUBLIC_KEY_HEX: &str =
    "6666ea43c28087bb2a36a0b4ea3437dbbe6b188a0ebb3dbfbf9de30dc1a932e4";

/// Product identifier verified at redemption — a blob from another product
/// is refused (RF-38).
pub const PRODUCT: &str = "voxli-desktop";

/// Parses the embedded public key. `None` if the constant is zeros
/// (development binary without a key): every license redemption will then
/// be refused with an explicit message — never silent validation.
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
        // The constant must ALWAYS be a 64-character hex string or
        // zeros — a shipped binary with a truncated key is a failure.
        assert_eq!(EMBEDDED_PUBLIC_KEY_HEX.len(), PUBLIC_KEY_LENGTH * 2);
        let parsed = hex_decode_32(EMBEDDED_PUBLIC_KEY_HEX);
        assert!(parsed.is_some(), "hex embarqué valide");
    }

    #[test]
    fn t_zero_key_is_not_embedded() {
        // During the test phase: if a real development pair has been
        // generated, the constant is no longer zeros — this test detects
        // it. It makes the invariant precise: zero ⇒ `None`.
        if EMBEDDED_PUBLIC_KEY_HEX.starts_with("0000") {
            assert!(embedded_public_key().is_none());
        } else {
            assert!(embedded_public_key().is_some());
        }
    }

    #[test]
    fn t_purchasable_features_match_core() {
        // Exact equality (closed list) with the kernel's `Feature` enum:
        // a purchasable string matching no `Feature`, or a `Feature` that
        // is not purchasable, is a consistency failure.
        let mut expected: Vec<&str> = [
            voxli_core::Feature::HistoryOnDisk,
            voxli_core::Feature::CloudPostProcessing,
            voxli_core::Feature::ExtraProfiles,
        ]
        .iter()
        .map(|f| f.as_str())
        .collect();
        let mut purchasable: Vec<&str> = PURCHASABLE_FEATURES.to_vec();
        purchasable.sort_unstable();
        expected.sort_unstable();
        assert_eq!(purchasable, expected);
        assert_eq!(PURCHASABLE_FEATURES.len(), 3);
    }
}
