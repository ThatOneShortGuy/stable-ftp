use std::{
    error::Error,
    io::prelude::*,
    net::{TcpListener, TcpStream},
    time::Duration,
};

use prost::Message;
use rusqlite::Connection;
use stable_ftp::{
    db::{self, DbFile},
    logger::{self, Loggable},
    num_packets,
    protos::{
        compare_versions,
        file_description_response::{Event, FileStatus},
        AuthRequest, AuthResponse, FileDescription, FileDescriptionResponse, VersionCompatibility,
    },
};

fn handle_auth_err(stream: &mut TcpStream, msg: impl AsRef<str>) {
    let response = AuthResponse {
        success: false,
        failure_reason: format!("Failed to Authenitcate: {}", msg.as_ref()),
    };
    stream
        .write(&response.encode_to_vec())
        .to_error("Failed to write to buffer stream");
    return;
}

fn handle_client(mut stream: TcpStream) {
    let mut buf = [0; 1024];

    logger::info(format!(
        "New client connected: {}",
        stream.peer_addr().expect("Can't get the peer address??")
    ));

    let _bytes_read = match stream.read(&mut buf[..]) {
        Ok(a) => a,
        Err(err) => {
            handle_auth_err(&mut stream, err.to_string());
            return;
        }
    };

    let AuthRequest { version, token } = match AuthRequest::decode(&buf[.._bytes_read]) {
        Ok(req) => req,
        Err(err) => {
            handle_auth_err(&mut stream, err.to_string());
            return;
        }
    };

    // Verify Versions are compatible
    let server_version = env!("CARGO_PKG_VERSION").into();
    let client_version = &version;
    if let VersionCompatibility::Incompatible = compare_versions(&server_version, client_version) {
        handle_auth_err(&mut stream, format!("Version types are Incompatible! Client version ({client_version}) is not compatible with server version ({server_version})"));
        return;
    }

    // TODO: Verify client with SQLite
    let db = db::get_connection("stable-ftp.sqlite");
    if !db::token_exists(&db, &token) {
        let res = AuthResponse {
            success: false,
            failure_reason: "Invalid Token/Token Not Found".to_string(),
        };
        stream
            .write(&res.encode_to_vec())
            .to_error("Failed to write fail to stream");
        return;
    }
    let user_id = db::get_user_id(&db, &token);

    let response = AuthResponse {
        success: true,
        failure_reason: String::new(),
    };
    stream
        .write(&response.encode_to_vec())
        .to_error("Failed to return success auth message");

    stream
        .set_read_timeout(Some(Duration::new(5, 0)))
        .to_error("Failed to set the timeout?!?");

    match handle_file_description(&mut stream, &db, user_id) {
        Ok(_) => (),
        Err(err) => {
            let res = FileDescriptionResponse {
                event: Some(
                    stable_ftp::protos::file_description_response::Event::FailMessage(
                        err.to_string(),
                    ),
                ),
            };
            stream
                .write(&res.encode_to_vec())
                .to_error("Failed to write to stream");
        }
    }
}

fn handle_file_description(
    stream: &mut TcpStream,
    db: &Connection,
    user_id: u64,
) -> Result<(), Box<dyn Error>> {
    let mut buf = [0; 1024];

    let nbytes = stream.read(&mut buf)?;

    let FileDescription {
        name,
        size,
        packet_size,
    } = FileDescription::decode(&buf[..nbytes])?;

    let file = db::find_filename(db, &name);

    let response = match file {
        Some(_) => todo!(),
        None => {
            let mut file =
                std::fs::File::create_new(&name).to_error(format!("'{name}' already exists"));
            file.seek_relative(size as i64)?;
            file.write(&[69])?;

            let total_packets = num_packets(packet_size, size);
            let db_file = DbFile::new(name, total_packets, packet_size, user_id);
            let db_file = db::insert_file(db, db_file)?;

            FileDescriptionResponse {
                event: Some(Event::Status(FileStatus {
                    status: stable_ftp::protos::file_description_response::file_status::FileStatusEnum::Nonexistent.into(),
                    id: db_file.id.expect("Should be a Some varient"),
                    request_packet: 0,
                    packet_size,
                    
                })),
            }
        }
    };
    stream.write(&response.encode_to_vec())?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ip = "127.0.0.1:35672";
    let listener = TcpListener::bind(ip).to_error("Failed to bind to IP");
    logger::info(&format!("Server ready on {ip}"));

    for conn in listener.incoming() {
        match conn.with_warning("Failed to connect") {
            Ok(stream) => {
                std::thread::spawn(|| handle_client(stream));
            }
            _ => (),
        }
    }
    Ok(())
}
