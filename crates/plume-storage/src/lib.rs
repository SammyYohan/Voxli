//! `plume-storage` — historique local (RF-33).
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
    /// Suppression totale, y compris le fichier (purge à la désinstallation).
    fn purge(&mut self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

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
    //! fichier (lié au compte Windows via plume-secure) est activé en P2.
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
            Ok(Self { conn, next_id: 1 })
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

    #[test]
    fn t_capacity_bounded() {
        let mut s = MemoryStore::new(3);
        for i in 0..10 {
            s.append(&format!("n{i}"), "fake", i).unwrap();
        }
        assert_eq!(s.len(), 3);
    }
}
