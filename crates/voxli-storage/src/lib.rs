//! `voxli-storage` — historique local (RF-33).
//!
//! Défaut produit : **mémoire seule**, aucune trace de texte sur disque.
//! L'option « conserver sur disque » (V1) utilise SQLite chiffré, lié au
//! compte Windows, avec rétention configurable et purge à la désinstallation.
//!
//! Interface volontairement minimale (docs/05 §2.6) : `append`, `recent`,
//! `delete`, `purge`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("stockage indisponible : {0}")]
    Io(String),
    #[error("base corrompue : {0}")]
    Corrupt(String),
}

/// Entrée d'historique : le texte y est porté par l'objet métier, jamais
/// journalisé (RF-34).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    pub engine: String,
    pub created_ms: u64,
}

/// Backend d'historique. Toutes les méthodes sont bornées (pas de requête non
/// limitée) ; `purge` supprime tout (RF-33).
pub trait HistoryStore: Send {
    fn append(&mut self, text: &str, engine: &str, created_ms: u64) -> Result<u64, StorageError>;
    /// Les `n` entrées les plus récentes, la plus récente d'abord.
    fn recent(&self, n: usize) -> Vec<Entry>;
    fn delete(&mut self, id: u64) -> bool;
    /// Supprime les entrées plus vieilles que `cutoff_ms` (rétention RF-33) ;
    /// retourne le nombre d'entrées retirées.
    fn retain_since(&mut self, cutoff_ms: u64) -> usize;
    /// Suppression totale, y compris le fichier (purge à la désinstallation).
    fn purge(&mut self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Codec de chiffrement du texte au repos (RF-33). Le contenu **n'est
/// jamais stocké en clair** quand un codec réel est branché ; l'interface
/// empêche d'oublier l'un des deux sens. Un échec de chiffrement est une
/// erreur **explicite** : jamais un blob vide stocké sous le coup d'un
/// `unwrap_or_default()` (donnée perdue sans trace).
pub trait TextCodec: Send {
    fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError>;
    fn decode(&self, blob: &[u8]) -> Option<String>;
}

/// Codec transparent — tests et backend mémoire.
pub struct PlainCodec;

impl TextCodec for PlainCodec {
    fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError> {
        Ok(text.as_bytes().to_vec())
    }
    fn decode(&self, blob: &[u8]) -> Option<String> {
        String::from_utf8(blob.to_vec()).ok()
    }
}

/// Enveloppe d'un `HistoryStore` avec chiffrement du champ `text` (RF-33).
/// Le fichier SQLite reste listable (identifiants, dates) : les **contenus**
/// sont illisibles hors du compte Windows.
pub struct EncryptedStore<S: HistoryStore, C: TextCodec> {
    inner: S,
    codec: C,
}

impl<S: HistoryStore, C: TextCodec> EncryptedStore<S, C> {
    pub fn new(inner: S, codec: C) -> Self {
        Self { inner, codec }
    }
    /// Accès direct au store sous-jacent (purge du fichier, VACUUM…).
    pub fn inner(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: HistoryStore, C: TextCodec> HistoryStore for EncryptedStore<S, C> {
    fn append(&mut self, text: &str, engine: &str, created_ms: u64) -> Result<u64, StorageError> {
        let blob = self.codec.encode(text)?;
        // Stockage en tableau d'octets échappés UTF-8 lossy : le codec peut
        // produire des octets arbitraires (DPAPI) ; on transite par encodage
        // hexadécimal borné pour rester un TEXT sûr dans la colonne.
        let hex = bytes_to_hex(&blob);
        self.inner.append(&hex, engine, created_ms)
    }

    fn recent(&self, n: usize) -> Vec<Entry> {
        self.inner
            .recent(n)
            .into_iter()
            .map(|mut e| {
                e.text = decode_hex(&e.text)
                    .and_then(|b| self.codec.decode(&b))
                    .unwrap_or_default();
                e
            })
            .collect()
    }

    fn delete(&mut self, id: u64) -> bool {
        self.inner.delete(id)
    }

    fn retain_since(&mut self, cutoff_ms: u64) -> usize {
        self.inner.retain_since(cutoff_ms)
    }

    fn purge(&mut self) {
        self.inner.purge()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

#[cfg(all(feature = "secure-disk", target_os = "windows"))]
pub mod secure_backend {
    //! Codec DPAPI : chaque entrée d'historique est chiffrée **au compte
    //! Windows** (RFC RF-33 « chiffré, lié au compte »). Un fichier copié sur
    //! une autre machine, ou lu par un autre compte, est illisible.
    use super::TextCodec;

    pub struct DpapiCodec;

    impl TextCodec for DpapiCodec {
        fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError> {
            voxli_secure::dpapi::protect(text.as_bytes())
                .map_err(|e| StorageError::Io(format!("dpapi : {e}")))
        }
        fn decode(&self, blob: &[u8]) -> Option<String> {
            let plain = voxli_secure::dpapi::unprotect(blob).ok()?;
            String::from_utf8(plain).ok()
        }
    }
}

#[cfg(all(feature = "secure-disk", target_os = "windows"))]
pub use secure_backend::DpapiCodec;

/// Backend mémoire (défaut produit). Double de test pour le backend SQLite.
#[derive(Default)]
pub struct MemoryStore {
    entries: Vec<Entry>,
    next_id: u64,
    /// Capacité maximale en mémoire (protection contre une dérive).
    cap: usize,
}

impl MemoryStore {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            cap: cap.max(1),
        }
    }
}

impl HistoryStore for MemoryStore {
    fn append(&mut self, text: &str, engine: &str, created_ms: u64) -> Result<u64, StorageError> {
        if self.entries.len() >= self.cap {
            self.entries.pop();
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.insert(
            0,
            Entry {
                id,
                text: text.to_string(),
                engine: engine.to_string(),
                created_ms,
            },
        );
        Ok(id)
    }

    fn recent(&self, n: usize) -> Vec<Entry> {
        self.entries.iter().take(n).cloned().collect()
    }

    fn delete(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    fn retain_since(&mut self, cutoff_ms: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.created_ms >= cutoff_ms);
        before - self.entries.len()
    }

    fn purge(&mut self) {
        self.entries.clear();
        self.next_id = 1;
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(feature = "sqlite")]
mod sqlite_backend {
    //! Backend disque chiffré (V1) — squelette : le chiffrement au niveau
    //! fichier (lié au compte Windows via voxli-secure) est activé en P2.
    use super::*;

    pub struct SqliteStore {
        conn: rusqlite::Connection,
        next_id: u64,
    }

    impl SqliteStore {
        pub fn open(path: &std::path::Path) -> Result<Self, StorageError> {
            let conn =
                rusqlite::Connection::open(path).map_err(|e| StorageError::Io(e.to_string()))?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history (
                     id INTEGER PRIMARY KEY,
                     text BLOB NOT NULL,
                     engine TEXT NOT NULL,
                     created_ms INTEGER NOT NULL
                 );",
            )
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
            // La séquence des identifiants est RE PRISE de la base : une
            // réouverture (nouveau lancement de l'application) repart de
            // MAX(id)+1 — sinon les INSERT explicites heurtaient la clé
            // primaire existante et tout append échouait jusqu'à redémarrage
            // (constat au niveau code : `next_id` repartait in-memory de 1).
            let max = conn
                .query_row("SELECT COALESCE(MAX(id), 0) FROM history", [], |r| {
                    r.get::<_, i64>(0)
                })
                .map_err(|e| StorageError::Corrupt(e.to_string()))?;
            Ok(Self {
                conn,
                next_id: (max.max(0) as u64) + 1,
            })
        }
    }

    impl HistoryStore for SqliteStore {
        fn append(
            &mut self,
            text: &str,
            engine: &str,
            created_ms: u64,
        ) -> Result<u64, StorageError> {
            let id = self.next_id;
            self.next_id += 1;
            self.conn
                .execute(
                    "INSERT INTO history (id, text, engine, created_ms) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id as i64, text.as_bytes(), engine, created_ms as i64],
                )
                .map_err(|e| StorageError::Io(e.to_string()))?;
            Ok(id)
        }

        fn recent(&self, n: usize) -> Vec<Entry> {
            let mut stmt = match self.conn.prepare(
                "SELECT id, text, engine, created_ms FROM history ORDER BY id DESC LIMIT ?1",
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = stmt.query_map([n as i64], |row| {
                let text_bytes: Vec<u8> = row.get(1)?;
                Ok(Entry {
                    id: row.get::<_, i64>(0)? as u64,
                    text: String::from_utf8_lossy(&text_bytes).into_owned(),
                    engine: row.get(2)?,
                    created_ms: row.get::<_, i64>(3)? as u64,
                })
            });
            match rows {
                Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
                Err(_) => Vec::new(),
            }
        }

        fn delete(&mut self, id: u64) -> bool {
            self.conn
                .execute("DELETE FROM history WHERE id = ?1", [id as i64])
                .map(|n| n > 0)
                .unwrap_or(false)
        }

        fn retain_since(&mut self, cutoff_ms: u64) -> usize {
            self.conn
                .execute(
                    "DELETE FROM history WHERE created_ms < ?1",
                    [cutoff_ms as i64],
                )
                .unwrap_or(0)
        }

        fn purge(&mut self) {
            let _ = self.conn.execute_batch("DELETE FROM history; VACUUM;");
        }

        fn len(&self) -> usize {
            self.conn
                .query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))
                .map(|n| n as usize)
                .unwrap_or(0)
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite_backend::SqliteStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_append_recent_order() {
        let mut s = MemoryStore::new(100);
        s.append("un", "fake", 1).unwrap();
        s.append("deux", "fake", 2).unwrap();
        let recent = s.recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].text, "deux", "la plus récente d'abord");
        assert_eq!(recent[1].text, "un");
    }

    #[test]
    fn t_delete_and_purge() {
        let mut s = MemoryStore::new(100);
        let id = s.append("texte", "fake", 1).unwrap();
        assert!(s.delete(id));
        assert!(!s.delete(id + 999));
        s.append("autre", "fake", 2).unwrap();
        s.purge();
        assert!(s.is_empty());
    }

    /// Rétention RF-33 : les entrées plus vieilles que la fenêtre partent.
    #[test]
    fn t_retention_cutoff() {
        let mut s = MemoryStore::new(100);
        s.append("ancien", "fake", 1).unwrap();
        s.append("récent", "fake", 999).unwrap();
        let removed = s.retain_since(500);
        assert_eq!(removed, 1);
        let texts: Vec<String> = s.recent(10).into_iter().map(|e| e.text).collect();
        assert_eq!(texts, vec!["récent".to_string()]);
    }

    /// Store chiffré : le texte ne transite jamais en clair, round-trip net.
    #[test]
    fn t_encrypted_store_round_trip_and_at_rest() {
        struct UppercaseCodec;
        impl TextCodec for UppercaseCodec {
            fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError> {
                Ok(text.to_uppercase().as_bytes().to_vec())
            }
            fn decode(&self, blob: &[u8]) -> Option<String> {
                Some(String::from_utf8(blob.to_vec()).ok()?.to_lowercase())
            }
        }
        let mut s = EncryptedStore::new(MemoryStore::new(10), UppercaseCodec);
        s.append("contenu secret", "fake", 7).unwrap();
        // Au repos : la valeur stockée est l'encodage hexadécimal du texte
        // codé (format obligatoire pour des blobs binaires dans SQLite) —
        // jamais l'original.
        let raw = s.inner().recent(1);
        assert_eq!(raw[0].text, "434f4e54454e5520534543524554");
        // En lecture : décodé.
        assert_eq!(s.recent(1)[0].text, "contenu secret");
        // Rétention déléguée.
        assert_eq!(s.retain_since(8), 1);
        assert!(s.is_empty());
    }

    #[test]
    fn t_hex_round_trip() {
        let data = vec![0u8, 1, 254, 255, 10, 0x41];
        let hex = bytes_to_hex(&data);
        assert_eq!(decode_hex(&hex).unwrap(), data);
        assert!(decode_hex("abc").is_none());
    }

    /// Un échec de chiffrement est une erreur explicite : RIEN n'est
    /// écrit (jamais de blob vide qui ferait disparaître l'entrée).
    #[test]
    fn t_encrypt_failure_is_surfacable() {
        struct FailingCodec;
        impl TextCodec for FailingCodec {
            fn encode(&self, _text: &str) -> Result<Vec<u8>, StorageError> {
                Err(StorageError::Io("dpapi indisponible".into()))
            }
            fn decode(&self, _blob: &[u8]) -> Option<String> {
                None
            }
        }
        let mut s = EncryptedStore::new(MemoryStore::new(10), FailingCodec);
        assert!(s.append("texte", "fake", 1).is_err());
        assert!(s.is_empty(), "aucune écriture après échec de chiffrement");
    }

    /// Régression backend SQLite (feature `sqlite`) : après réouverture,
    /// les identifiants reprennent après le maximum existant — sinon les
    /// INSERT explicites heurtaient la clé primaire et tout append
    /// échouait jusqu'à redémarrage.
    #[cfg(feature = "sqlite")]
    #[test]
    fn t_sqlite_reopen_continues_id_sequence() {
        let dir = std::env::temp_dir().join("voxli-storage-test-reopen");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hist.db");

        let id1 = {
            let mut s = SqliteStore::open(&path).unwrap();
            s.append("première", "fake", 1).unwrap()
        };
        {
            let mut s = SqliteStore::open(&path).unwrap();
            let id2 = s.append("réouvert", "fake", 2).unwrap();
            assert_ne!(
                id1, id2,
                "l'id repris de la base ne doit pas entrer en collision"
            );
            let recent = s.recent(10);
            assert_eq!(recent[0].text, "réouvert");
            assert_eq!(recent.len(), 2);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn t_capacity_bounded() {
        let mut s = MemoryStore::new(3);
        for i in 0..10 {
            s.append(&format!("n{i}"), "fake", i).unwrap();
        }
        assert_eq!(s.len(), 3);
    }
}
