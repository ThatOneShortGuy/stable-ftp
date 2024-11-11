use std::{error::Error, ffi::OsString, path::Path};

use rusqlite::{params, Connection};

use crate::logger::Loggable;

#[derive(Debug, Clone)]
pub struct DbFile {
    pub id: Option<u64>,
    pub filename: OsString,
    pub current_packet: u64,
    pub total_packets: u64,
    pub packet_size: u64,
    pub inserted_by_id: u64,
}

impl DbFile {
    pub fn new(
        filename: OsString,
        total_packets: u64,
        packet_size: u64,
        inserted_by_id: u64,
    ) -> Self {
        DbFile {
            id: None,
            filename,
            current_packet: 0,
            total_packets,
            packet_size,
            inserted_by_id,
        }
    }
}

pub fn get_connection(filename: &str) -> Connection {
    if !Path::new(filename).exists() {
        let conn = Connection::open(filename).to_error("Failed to create db");
        conn.execute_batch(include_str!("init.sql"))
            .to_error("Failed to execute init.sql");
        conn
    } else {
        Connection::open(filename).to_error("Failed opening db")
    }
}

pub fn token_exists(db: &Connection, token: &str) -> bool {
    let query = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM user_tokens WHERE token=?1)",
            params![token],
            |row| row.get::<_, bool>(0),
        )
        .to_error("Failed to query db in token_exists");
    query
}

pub fn get_user_id(db: &Connection, token: &str) -> Result<u64, Box<dyn Error>> {
    Ok(db.query_row(
        "SELECT id from user_tokens WHERE TOKEN = ?1",
        params![token],
        |row| row.get(0),
    )?)
}

pub fn find_filename(
    db: &Connection,
    filename: &str,
) -> Result<Option<DbFile>, Box<dyn std::error::Error>> {
    let mut binding = db
        .prepare("SELECT * FROM files WHERE filename = ?1")
        .to_error("SELECT WHERE filename=?1 should compile");
    let mut row = binding.query(params![filename])?.mapped(|row| {
        let filename: String = row.get("filename")?;
        let id = row.get("id")?;
        let filename = filename.into();
        let current_packet = row.get("current_packet")?;
        let total_packets = row.get("total_packets")?;
        let packet_size = row.get("packet_size")?;
        let inserted_by_id = row.get("inserted_by_id")?;

        Ok(DbFile {
            id,
            filename,
            current_packet,
            total_packets,
            packet_size,
            inserted_by_id,
        })
    });
    Ok(match row.next() {
        Some(file) => Some(file?),
        None => None,
    })
}

pub fn insert_file(db: &Connection, mut file: DbFile) -> Result<DbFile, Box<dyn Error>> {
    let filename = file.filename.to_string_lossy();
    db.execute(
        "INSERT INTO files (filename, current_packet, total_packets, packet_size, inserted_by_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![filename, file.current_packet, file.total_packets, file.packet_size, file.inserted_by_id]
    )?;
    file.id = Some(
        db.last_insert_rowid()
            .try_into()
            .to_error("Failed to convert row_id to u64"),
    );
    Ok(file)
}
