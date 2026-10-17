//! `plume-secure` — stockage des secrets (ADR-07).
//!
//! Les clés d'API des services distants et le jeton d'activation ne vivent
//! jamais dans `config.json`, jamais dans le stockage du webview. Deux
//! backends :
//! - `MemorySecretStore` : double de test et mode dégradé ;
//! - `DpapiSecretStore` (feature `dpapi`, Windows) : blobs chiffrés liés au
//!   compte Windows (`CryptProtectData`), fichier par clé.

use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecureError {
    #[error("chiffrement DPAPI indisponible : {0}")]
    Dpapi(String),
    #[error("écriture du coffre impossible : {0}")]
    Io(String),
    #[error("secret illisible : {0}")]
    Corrupt(String),
}

/// Stockage clé → valeur binaire (get/set/delete) — interface minimale
/// (docs/05 §2.6).
pub trait SecretStore: Send {
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), SecureError>;
    fn delete(&mut self, key: &str) -> bool;
}

/// Coffre mémoire (test et mode dégradé).
#[derive(Default)]
pub struct MemorySecretStore {
    map: HashMap<String, Vec<u8>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.map.get(key).cloned()
    }
    fn set(&mut self, key: &str, value: &[u8]) -> Result<(), SecureError> {
        self.map.insert(key.to_string(), value.to_vec());
        Ok(())
    }
    fn delete(&mut self, key: &str) -> bool {
        self.map.remove(key).is_some()
    }
}

/// Protection blob liée à l'utilisateur Windows (DPAPI). Utilisée pour
/// l'historique chiffré sur disque (RF-33) et le cache de récupération.
#[cfg(all(target_os = "windows", feature = "dpapi"))]
pub mod dpapi {
    use super::{SecretStore, SecureError};
    use windows::core::BSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// Chiffre un blob lié au compte Windows courant.
    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, SecureError> {
        unsafe {
            let in_blob = CRYPT_INTEGER_BLOB {
                cbData: plain.len() as u32,
                pbData: plain.as_ptr() as *mut u8,
            };
            let mut out = CRYPT_INTEGER_BLOB::default();
            let desc = BSTR::from("plume");
            CryptProtectData(
                &in_blob,
                &desc,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
            .map_err(|e| SecureError::Dpapi(e.to_string()))?;
            let slice = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let data = slice.to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
            Ok(data)
        }
    }

    /// Déchiffre un blob DPAPI (échoue sous un autre compte Windows).
    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, SecureError> {
        unsafe {
            let in_blob = CRYPT_INTEGER_BLOB {
                cbData: cipher.len() as u32,
                pbData: cipher.as_ptr() as *mut u8,
            };
            let mut out = CRYPT_INTEGER_BLOB::default();
            CryptUnprotectData(
                &in_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
            .map_err(|e| SecureError::Dpapi(e.to_string()))?;
            let slice = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let data = slice.to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
            Ok(data)
        }
    }

    /// Coffre fichier : un blob DPAPI par clé, dans un dossier privé.
    pub struct DpapiSecretStore {
        dir: std::path::PathBuf,
    }

    impl DpapiSecretStore {
        pub fn open(dir: impl Into<std::path::PathBuf>) -> Result<Self, SecureError> {
            let dir = dir.into();
            std::fs::create_dir_all(&dir).map_err(|e| SecureError::Io(e.to_string()))?;
            Ok(Self { dir })
        }

        fn path_for(&self, key: &str) -> std::path::PathBuf {
            // La clé est un identifiant interne (jamais du contenu).
            self.dir.join(format!("{key}.dpapi"))
        }
    }

    impl SecretStore for DpapiSecretStore {
        fn get(&self, key: &str) -> Option<Vec<u8>> {
            let bytes = std::fs::read(self.path_for(key)).ok()?;
            unprotect(&bytes).ok()
        }
        fn set(&mut self, key: &str, value: &[u8]) -> Result<(), SecureError> {
            let cipher = protect(value)?;
            std::fs::write(self.path_for(key), cipher).map_err(|e| SecureError::Io(e.to_string()))
        }
        fn delete(&mut self, key: &str) -> bool {
            std::fs::remove_file(self.path_for(key)).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_memory_store_round_trip() {
        let mut s = MemorySecretStore::default();
        assert!(s.get("cle").is_none());
        s.set("cle", b"valeur").unwrap();
        assert_eq!(s.get("cle"), Some(b"valeur".to_vec()));
        assert!(s.delete("cle"));
        assert!(!s.delete("cle"));
    }
}
