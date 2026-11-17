// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Encrypted storage with SQLCipher (security-critical). Without the key, the file is unreadable.

use rusqlite::{Connection, OptionalExtension, params};
use zeroize::Zeroizing;

use crate::error::CoreError;
use crate::events::SignedEvent;
use crate::model::Notice;

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
CREATE TABLE IF NOT EXISTS notices (
    id        TEXT PRIMARY KEY,
    json      TEXT NOT NULL,
    at_ms     INTEGER NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0
);
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

    pub fn path(&self) -> Option<String> {
        self.conn.path().map(str::to_owned)
    }

    /// "Delete all my data": empties every table and re-encrypts the file with `new_key`. Once the
    /// old key is overwritten in the keystore, nothing written with it (freed pages, the WAL,
    /// copies the flash memory keeps) can be read again.
    pub fn wipe_and_rekey(&self, new_key: &[u8]) -> Result<(), CoreError> {
        self.conn.execute_batch("DELETE FROM events; DELETE FROM meta; DELETE FROM notices;")?;
        // Out of WAL mode the WAL is checkpointed and removed; VACUUM drops the freed pages; then
        // SQLCipher re-encrypts every remaining page.
        self.conn.pragma_update_and_check(None, "journal_mode", "DELETE", |r| r.get::<_, String>(0))?;
        self.conn.execute_batch("VACUUM;")?;
        let pragma =
            Zeroizing::new(format!("PRAGMA rekey = \"x'{}'\";", Zeroizing::new(hex::encode(new_key)).as_str()));
        self.conn.execute_batch(&pragma)?;
        self.conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
        Ok(())
    }

    /// Whether this build encrypts (SQLCipher). Tests of the encryption itself skip without it.
    #[cfg(test)]
    pub fn encrypted(&self) -> bool {
        self.conn.query_row("PRAGMA cipher_version", [], |r| r.get::<_, String>(0)).is_ok()
    }

    pub fn insert(&self, ev: &SignedEvent) -> Result<(), CoreError> {
        let json = serde_json::to_vec(ev).map_err(CoreError::storage)?;
        // `uploaded` is a leftover of the relay: always 0, kept so older databases still open.
        self.conn.execute(
            "INSERT OR IGNORE INTO events (id, author, seq, at_ms, json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ev.id(), ev.body.author, ev.body.seq as i64, ev.body.at_ms, json],
        )?;
        Ok(())
    }

    pub fn all(&self) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events ORDER BY rowid", [])
    }

    pub fn all_by(&self, author: &str) -> Result<Vec<SignedEvent>, CoreError> {
        self.query("SELECT json FROM events WHERE author = ?1 ORDER BY seq", [author])
    }

    fn query<P: rusqlite::Params>(&self, sql: &str, p: P) -> Result<Vec<SignedEvent>, CoreError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| r.get::<_, Vec<u8>>(0))?;
        rows.map(|b| serde_json::from_slice(&b?).map_err(CoreError::storage)).collect()
    }

    pub fn meta(&self, k: &str) -> Result<Option<String>, CoreError> {
        Ok(self.conn.query_row("SELECT v FROM meta WHERE k = ?1", [k], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&self, k: &str, v: &str) -> Result<(), CoreError> {
        self.conn
            .execute("INSERT INTO meta (k, v) VALUES (?1, ?2) ON CONFLICT(k) DO UPDATE SET v = excluded.v", [k, v])?;
        Ok(())
    }

    /// Local retention: deletes events older than `before_ms`, except those `keep` says the
    /// profile needs forever (members, configuration). Also forgets delivered notices.
    pub fn prune_before(&self, before_ms: i64, keep: impl Fn(&SignedEvent) -> bool) -> Result<usize, CoreError> {
        let old = self.query("SELECT json FROM events WHERE at_ms < ?1", [before_ms])?;
        let tx = self.conn.unchecked_transaction()?;
        let mut n = 0;
        for ev in old.iter().filter(|e| !keep(e)) {
            n += tx.execute("DELETE FROM events WHERE id = ?1", [ev.id()])?;
        }
        tx.execute("DELETE FROM notices WHERE delivered = 1 AND at_ms < ?1", [before_ms])?;
        tx.commit()?;
        Ok(n)
    }

    /// Saves a notice once (same id = same notice, even after it was delivered).
    pub fn add_notice(&self, n: &Notice) -> Result<(), CoreError> {
        let json = serde_json::to_string(n).map_err(CoreError::storage)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO notices (id, json, at_ms) VALUES (?1, ?2, ?3)",
            params![n.id, json, n.at_ms],
        )?;
        Ok(())
    }

    /// Notices not yet delivered, oldest first; marks them delivered.
    pub fn take_notices(&self) -> Result<Vec<Notice>, CoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let out = {
            let mut stmt = tx.prepare("SELECT json FROM notices WHERE delivered = 0 ORDER BY at_ms, id")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|j| serde_json::from_str(&j?).map_err(CoreError::storage)).collect::<Result<Vec<Notice>, _>>()?
        };
        tx.execute("UPDATE notices SET delivered = 1 WHERE delivered = 0", [])?;
        tx.commit()?;
        Ok(out)
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

    #[test]
    fn wipe_and_rekey_leaves_an_empty_database_only_the_new_key_opens() {
        let dir = std::env::temp_dir().join(format!("pco-wipe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        let path_str = path.to_str().unwrap();
        let db = Db::open(path_str, &[1u8; 32]).unwrap();
        db.set_meta("a", "erased-secret").unwrap();
        db.wipe_and_rekey(&[3u8; 32]).unwrap();
        db.set_meta("b", "after").unwrap(); // still usable, back in WAL mode
        let encrypted = db.encrypted();
        drop(db);
        let db = Db::open(path_str, &[3u8; 32]).unwrap();
        assert_eq!(db.meta("a").unwrap(), None);
        assert_eq!(db.meta("b").unwrap().as_deref(), Some("after"));
        if encrypted {
            assert!(Db::open(path_str, &[1u8; 32]).is_err(), "the old key opens nothing");
        }
        drop(db);
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(13).any(|w| w == b"erased-secret"));
        std::fs::remove_dir_all(dir).ok();
    }
}
