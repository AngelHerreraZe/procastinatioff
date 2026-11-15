// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use rusqlite::{Connection, OptionalExtension, params};

pub const MAX_MEMBERS: i64 = 20;
pub const MAX_EVENTS_PER_MAILBOX: i64 = 20_000;
pub const EVENT_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
pub const MAILBOX_IDLE_MS: i64 = 90 * 24 * 3600 * 1000;
pub const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;
pub const PAGE_SIZE: i64 = 200;

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS mailboxes (
    id          TEXT PRIMARY KEY,
    created_at  INTEGER NOT NULL,
    last_active INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS members (
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    pubkey     TEXT NOT NULL,
    added_at   INTEGER NOT NULL,
    PRIMARY KEY (mailbox_id, pubkey)
);
CREATE TABLE IF NOT EXISTS events (
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,          -- secuencia POR buzón (no revela el volumen global)
    blob       BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (mailbox_id, seq)
);
CREATE TABLE IF NOT EXISTS push_tokens (
    pubkey     TEXT PRIMARY KEY,
    platform   TEXT NOT NULL,
    token      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pairing (
    token      TEXT PRIMARY KEY,
    blob       BLOB NOT NULL,
    expires_at INTEGER NOT NULL
);
";

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
    conn.execute_batch("PRAGMA secure_delete = ON; PRAGMA busy_timeout = 5000;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn create_mailbox(c: &Connection, id: &str, owner: &str, now: i64) -> rusqlite::Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute("INSERT INTO mailboxes (id, created_at, last_active) VALUES (?1, ?2, ?2)", params![id, now])?;
    tx.execute("INSERT INTO members (mailbox_id, pubkey, added_at) VALUES (?1, ?2, ?3)", params![id, owner, now])?;
    tx.commit()
}

pub fn is_member(c: &Connection, mailbox: &str, pubkey: &str) -> rusqlite::Result<bool> {
    c.query_row("SELECT 1 FROM members WHERE mailbox_id = ?1 AND pubkey = ?2", params![mailbox, pubkey], |_| Ok(()))
        .optional()
        .map(|o| o.is_some())
}

pub fn member_count(c: &Connection, mailbox: &str) -> rusqlite::Result<i64> {
    c.query_row("SELECT count(*) FROM members WHERE mailbox_id = ?1", [mailbox], |r| r.get(0))
}

pub fn add_member(c: &Connection, mailbox: &str, pubkey: &str, now: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR IGNORE INTO members (mailbox_id, pubkey, added_at) VALUES (?1, ?2, ?3)",
        params![mailbox, pubkey, now],
    )?;
    Ok(())
}

pub fn remove_member(c: &Connection, mailbox: &str, pubkey: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM members WHERE mailbox_id = ?1 AND pubkey = ?2", params![mailbox, pubkey])?;
    Ok(())
}

pub fn delete_mailbox(c: &Connection, mailbox: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM mailboxes WHERE id = ?1", [mailbox])?;
    Ok(())
}

/// Inserta un evento y devuelve su secuencia. El Mutex de la conexión serializa las escrituras.
pub fn insert_event(c: &Connection, mailbox: &str, blob: &[u8], now: i64) -> rusqlite::Result<Option<i64>> {
    let count: i64 = c.query_row("SELECT count(*) FROM events WHERE mailbox_id = ?1", [mailbox], |r| r.get(0))?;
    if count >= MAX_EVENTS_PER_MAILBOX {
        return Ok(None);
    }
    let seq: i64 =
        c.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE mailbox_id = ?1", [mailbox], |r| r.get(0))?;
    c.execute(
        "INSERT INTO events (mailbox_id, seq, blob, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![mailbox, seq, blob, now],
    )?;
    c.execute("UPDATE mailboxes SET last_active = ?2 WHERE id = ?1", params![mailbox, now])?;
    Ok(Some(seq))
}

pub fn events_after(c: &Connection, mailbox: &str, after: i64) -> rusqlite::Result<Vec<(i64, Vec<u8>)>> {
    let mut st = c.prepare("SELECT seq, blob FROM events WHERE mailbox_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3")?;
    let rows = st.query_map(params![mailbox, after, PAGE_SIZE], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

pub fn other_members_push(c: &Connection, mailbox: &str, except: &str) -> rusqlite::Result<Vec<(String, String)>> {
    let mut st = c.prepare(
        "SELECT p.platform, p.token FROM members m JOIN push_tokens p ON p.pubkey = m.pubkey
         WHERE m.mailbox_id = ?1 AND m.pubkey <> ?2",
    )?;
    let rows = st.query_map(params![mailbox, except], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

pub fn put_push(c: &Connection, pubkey: &str, platform: &str, token: &str, now: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO push_tokens (pubkey, platform, token, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(pubkey) DO UPDATE SET platform = excluded.platform, token = excluded.token, updated_at = excluded.updated_at",
        params![pubkey, platform, token, now],
    )?;
    Ok(())
}

/// Escribe la ranura de vinculación. Solo una vez por token.
pub fn put_pairing(c: &Connection, token: &str, blob: &[u8], now: i64) -> rusqlite::Result<bool> {
    let n = c.execute(
        "INSERT OR IGNORE INTO pairing (token, blob, expires_at) VALUES (?1, ?2, ?3)",
        params![token, blob, now + PAIRING_TTL_MS],
    )?;
    Ok(n == 1)
}

pub fn get_pairing(c: &Connection, token: &str, now: i64) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT blob FROM pairing WHERE token = ?1 AND expires_at > ?2", params![token, now], |r| r.get(0))
        .optional()
}

/// Limpieza periódica (cada hora).
pub fn cleanup(c: &Connection, now: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM events WHERE created_at < ?1", [now - EVENT_TTL_MS])?;
    c.execute("DELETE FROM pairing WHERE expires_at < ?1", [now])?;
    c.execute("DELETE FROM mailboxes WHERE last_active < ?1", [now - MAILBOX_IDLE_MS])?;
    c.execute("DELETE FROM push_tokens WHERE pubkey NOT IN (SELECT pubkey FROM members)", [])?;
    Ok(())
}
