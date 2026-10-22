//! `voxli-storage` — local history (RF-33).
//!
//! Production default: **memory only**, no text trace on disk.
//! The "keep on disk" option (V1) uses encrypted SQLite, tied to the
//! Windows account, with configurable retention and purge at uninstall.
//!
//! Deliberately minimal interface (docs/05 §2.6): `append`, `recent`,
//! `delete`, `purge`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("stockage indisponible : {0}")]
    Io(String),
    #[error("base corrompue : {0}")]
    Corrupt(String),
}

/// History entry: its text is carried by the business object, never
/// logged (RF-34).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    pub engine: String,
    pub created_ms: u64,
}

/// History backend. Every method is bounded (no unbounded
/// query); `purge` deletes everything (RF-33).
pub trait HistoryStore: Send {
    fn append(&mut self, text: &str, engine: &str, created_ms: u64) -> Result<u64, StorageError>;
    /// The `n` most recent entries, most recent first.
    fn recent(&self, n: usize) -> Vec<Entry>;
    fn delete(&mut self, id: u64) -> bool;
    /// Deletes entries older than `cutoff_ms` (RF-33 retention);
    /// returns the number of entries removed.
    fn retain_since(&mut self, cutoff_ms: u64) -> usize;
    /// Full deletion, including the file (purge at uninstall).
    fn purge(&mut self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// At-rest text encryption codec (RF-33). Content is **never
/// stored in clear** when a real codec is plugged in; the interface
/// makes it impossible to forget either direction. An encryption failure
/// is an **explicit** error: never an empty blob stored via
/// `unwrap_or_default()` (data lost without a trace).
pub trait TextCodec: Send {
    fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError>;
    fn decode(&self, blob: &[u8]) -> Option<String>;
}

/// Transparent codec — tests and memory backend.
pub struct PlainCodec;

impl TextCodec for PlainCodec {
    fn encode(&self, text: &str) -> Result<Vec<u8>, StorageError> {
        Ok(text.as_bytes().to_vec())
    }
    fn decode(&self, blob: &[u8]) -> Option<String> {
        String::from_utf8(blob.to_vec()).ok()
    }
}

/// Wrapper around a `HistoryStore` that encrypts the `text` field (RF-33).
/// The SQLite file remains listable (identifiers, dates): the **contents**
/// are unreadable outside the Windows account.
pub struct EncryptedStore<S: HistoryStore, C: TextCodec> {
    inner: S,
    codec: C,
}

impl<S: HistoryStore, C: TextCodec> EncryptedStore<S, C> {
    pub fn new(inner: S, codec: C) -> Self {
        Self { inner, codec }
    }
    /// Direct access to the underlying store (file purge, VACUUM…).
    pub fn inner(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: HistoryStore, C: TextCodec> HistoryStore for EncryptedStore<S, C> {
    fn append(&mut self, text: &str, engine: &str, created_ms: u64) -> Result<u64, StorageError> {
        let blob = self.codec.encode(text)?;
        // Stored as a TEXT value: the codec may produce arbitrary bytes
        // (DPAPI), so the blob transits through a bounded hexadecimal
        // encoding to remain a safe TEXT value in the column.
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
    //! DPAPI codec: every history entry is encrypted **to the Windows
    //! account** (RF-33 "encrypted, tied to the account"). A file copied to
    //! another machine, or read by another account, is unreadable.
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

/// Memory backend (production default). Test double for the SQLite backend.
#[derive(Default)]
pub struct MemoryStore {
    entries: Vec<Entry>,
    next_id: u64,
    /// Maximum in-memory capacity (drift protection).
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
    //! Encrypted on-disk backend (V1) — skeleton: file-level
    //! encryption (tied to the Windows account via voxli-secure) is
    //! enabled in P2.
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
            // The ID sequence is RESUMED from the database: on reopen
            // (a new launch of the application) it continues from
            // MAX(id)+1 — otherwise the explicit INSERTs collided with
            // the existing primary key and every append failed until a
            // restart (observed at code level: `next_id` restarted
            // in memory from 1).
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

    /// RF-33 retention: entries older than the window are dropped.
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

    /// Encrypted store: text never transits in clear, clean round-trip.
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
        // At rest: the stored value is the hexadecimal encoding of the
        // encoded text (mandatory format for binary blobs in SQLite) —
        // never the original.
        let raw = s.inner().recent(1);
        assert_eq!(raw[0].text, "434f4e54454e5520534543524554");
        // On read: decoded.
        assert_eq!(s.recent(1)[0].text, "contenu secret");
        // Retention delegated.
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

    /// An encryption failure is an explicit error: NOTHING is
    /// written (never an empty blob that would erase the entry).
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

    /// SQLite backend regression (feature `sqlite`): after reopen,
    /// IDs continue after the existing maximum — otherwise the
    /// explicit INSERTs collided with the primary key and every append
    /// failed until a restart.
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
