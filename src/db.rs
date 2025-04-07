use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use typed_db::prelude::*;

use crate::{logger::Loggable, structs::Id};

#[derive(Debug, Clone, DbTable)]
pub struct DbFile {
    #[primary_key]
    pub id: Id,
    #[unique]
    pub filename: String,
    #[default(0)]
    current_packet: u64,
    pub total_packets: u64,
    pub packet_size: u64,
    #[foreign_key(UserAuth::id)]
    pub inserted_by_id: i32,
    #[default(CURRENT_TIMESTAMP)]
    pub created_date: DateTime<Utc>,
}

#[derive(Debug, Clone, DbTable)]
pub struct UserAuth {
    #[primary_key]
    pub id: Id,
    #[unique]
    pub token: String,
    pub notes: Option<String>,
    #[default(CURRENT_TIMESTAMP)]
    pub created_date: DateTime<Utc>,
}

impl DbFile {
    pub fn current_packet(&self) -> u64 {
        self.current_packet
    }

    pub fn inc_current_packet(mut self, con: &Connection) -> Result<Self, rusqlite::Error> {
        con.execute(
            &format!(
                "UPDATE {} SET current_packet = current_packet + 1 WHERE id == ?1",
                Self::TABLE_NAME
            ),
            params![self.id],
        )?;
        self.current_packet += 1;
        Ok(self)
    }

    pub fn find_filename(
        db: &Connection,
        filename: impl AsRef<str>,
    ) -> Result<Option<Self>, rusqlite::Error> {
        let rows = Self::select(
            &db,
            "WHERE filename = ? LIMIT 1",
            params![filename.as_ref()],
        )?;
        Ok(match rows.into_iter().next() {
            Some(file) => Some(file),
            None => None,
        })
    }
}

impl UserAuth {
    pub fn from_token(db: &Connection, token: &str) -> Result<Option<Self>, rusqlite::Error> {
        let rows = Self::select(&db, "WHERE token = ? LIMIT 1", params![token])?;
        Ok(rows.into_iter().next())
    }
}

static WRITE_CONNECTION: OnceLock<Mutex<Connection>> = OnceLock::new();
const DB_FILENAME: &'static str = "stable-ftp.sqlite";

pub fn get_write_connection() -> &'static Mutex<Connection> {
    WRITE_CONNECTION.get_or_init(|| {
        rusqlite::Connection::open(DB_FILENAME)
            .to_error("Failed to open Database file")
            .into()
    })
}

pub fn get_read_connection() -> Result<Connection, rusqlite::Error> {
    rusqlite::Connection::open_with_flags(DB_FILENAME, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
}
