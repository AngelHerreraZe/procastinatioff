// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Encrypted storage with SQLCipher (security-critical). Without the key, the file is unreadable.

use rusqlite::{Connection, OptionalExtension, params};
use zeroize::Zeroizing;

use crate::error::CoreError;
use crate::events::SignedEvent;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS events (
    id        TEXT PRIMARY KEY,
    author    TEXT NOT NULL,
    seq       INTEGER NOT NULL,
    at_ms     INTEGER NOT NULL,
    json      BLOB NOT NULL,
    uploaded  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS events_at ON events(at_ms);
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
";

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (or creates) the encrypted database. Fails if the key is wrong.
    pub fn open(path: &str, key: &[u8]) -> Result<Self, CoreError> {
        let conn = Connection::open(path)?;
        // SECURITY: The hex key is wiped from memory when leaving this block.
        let pragma = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", Zeroizing::new(hex::encode(key)).as_str()));
        conn.execute_batch(&pragma)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(|_| CoreError::storage("wrong key or damaged database"))?;
        conn.execute_batch("PRAGMA cipher_memory_security = ON; PRAGMA secure_delete = ON; PRAGMA foreign_keys = ON;")?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn insert(&self, ev: &SignedEvent, uploaded: bool) -> Result<(), CoreError> {
        let json = serde_json::to_vec(ev).map_err(CoreError::storage)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO events (id, author, seq, at_ms, json, uploaded) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![ev.id(), ev.body.author, ev.body.seq as i64, ev.body.at_ms, json, uploaded as i64],
        )?;
        Ok(())
    }

    pub fn all(&self) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events ORDER BY rowid", [])
    }

    pub fn pending_uploads(&self, author: &str) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events WHERE author = ?1 AND uploaded = 0 ORDER BY seq", [author])
    }

    pub fn all_by(&self, author: &str) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events WHERE author = ?1 ORDER BY seq", [author])
    }

    /// Everything pending upload, in the order it was accepted.
    pub fn pending_all(&self) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events WHERE uploaded = 0 ORDER BY rowid", [])
    }

    /// Marks every event for re-upload (encrypted for the new member list).
    pub fn mark_all_for_reupload(&self) -> Result<(), CoreError> {
        self.conn.execute("UPDATE events SET uploaded = 0", [])?;
        Ok(())
    }

    fn query<P: rusqlite::Params>(&self, sql: &str, p: P) -> Result<Vec<SignedEvent>, CoreError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| serde_json::from_slice(&b?).map_err(CoreError::storage)).collect()
    }

    pub fn mark_uploaded(&self, id: &str) -> Result<(), CoreError> {
        self.conn.execute("UPDATE events SET uploaded = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn meta(&self, k: &str) -> Result<Option<String>, CoreError> {
        Ok(self.conn.query_row("SELECT v FROM meta WHERE k = ?1", [k], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&self, k: &str, v: &str) -> Result<(), CoreError> {
        self.conn
            .execute("INSERT INTO meta (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v", [k, v])?;
        Ok(())
    }

    /// Local retention (90 days by default).
    pub fn prune_before(&self, before_ms: i64) -> Result<usize, CoreError> {
        Ok(self.conn.execute("DELETE FROM events WHERE at_ms < ?1 AND uploaded = 1", [before_ms])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_key_cannot_open_and_file_is_not_plain_sqlite() {
        let dir = std::env::temp_dir().join(format!("pco-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let path_str = path.to_str().unwrap();
        Db::open(path_str, &[1u8; 32]).unwrap().set_meta("a", "readable-secret").unwrap();
        assert!(Db::open(path_str, &[2u8; 32]).is_err());
        assert_eq!(Db::open(path_str, &[1u8; 32]).unwrap().meta("a").unwrap().as_deref(), Some("readable-secret"));
        // The plan's manual test, automated: no SQLite header nor readable text on disk.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"));
        assert!(!raw.windows(15).any(|w| w == b"readable-secret"));
        std::fs::remove_dir_all(dir).ok();
    }
}
