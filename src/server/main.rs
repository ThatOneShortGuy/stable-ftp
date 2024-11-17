use std::{
    error::Error,
    io::{self, prelude::*},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use prost::Message;
use rusqlite::Connection;
use stable_ftp::{
    db::{self, DbFile},
    file_size_text,
    logger::{self, Loggable},
    num_packets,
    protos::{
        compare_versions,
        file_description_response::{file_status::FileStatusEnum, Event, FileStatus},
        AuthRequest, AuthResponse, FileDescription, FileDescriptionResponse, FilePart,
        FilePartResponse, VersionCompatibility,
    },
    VecWithLen, MIN_PACKET_SIZE,
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

fn handle_client(mut stream: TcpStream, target_folder: &Path) {
    let mut buf = [0; 1024];

    logger::info(format!(
        "New client connected: {}",
        stream.peer_addr().to_error("Can't get the peer address??")
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
            handle_auth_err(&mut stream, format!("Auth request not understood: {err}"));
            return;
        }
    };

    // Verify Versions are compatible
    let server_version = env!("CARGO_PKG_VERSION").into();
    let client_version = &version;
    if let VersionCompatibility::Incompatible = compare_versions(&server_version, client_version) {
        handle_auth_err(&mut stream, format!("Version types are incompatible! Client version ({client_version}) is not compatible with server version ({server_version})"));
        return;
    }

    // Verify client with SQLite
    let con = db::get_connection("stable-ftp.sqlite");
    if !db::token_exists(&con, &token) {
        let res = AuthResponse {
            success: false,
            failure_reason: "Invalid Token/Token Not Found".to_string(),
        };
        stream
            .write(&res.encode_to_vec())
            .to_error("Failed to write fail to stream");
        return;
    }
    let user_id = match db::get_user_id(&con, &token) {
        Ok(v) => v,
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
            return;
        }
    };

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

    let (file, file_description, db_file) =
        match handle_file_description(&mut stream, &con, user_id, target_folder) {
            Ok(file) => file,
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
                return;
            }
        };

    match recv_files(&con, &mut stream, file, file_description, db_file) {
        Ok(a) => a,
        Err(e) => {
            logger::warning(format!("Failed in recv_files: {}", e.to_string()));
            let res = FilePartResponse {
                success: false,
                message: e.to_string(),
            };
            stream
                .write(&res.encode_to_vec())
                .to_error("Failed to write to stream");
            return;
        }
    }
}

fn recv_files(
    con: &Connection,
    stream: &mut TcpStream,
    mut file: std::fs::File,
    file_status: FileStatus,
    mut db_file: DbFile,
) -> Result<(), Box<dyn Error>> {
    let mut data = Vec::with_len(file_status.packet_size as usize + 48);

    for current_packet in file_status.request_packet..file_status.total_packets {
        let nbytes = stream.read(&mut data)?;
        let FilePart { part_num, data } =
            FilePart::decode(&data[..nbytes]).with_warning(format!(
                "Failed decoding from {nbytes} where buffer is {}",
                data.len()
            ))?;

        assert!(
            part_num == current_packet,
            "Part Num: {part_num} =! Expected Num: {current_packet}"
        );
        file.write_all(&data)
            .with_warning("Failed to write data to file")?;
        db_file = db_file.inc_current_packet(con)?;

        // logger::info(format!(
        //     "File '{}' wrote part {} of {}",
        //     db_file.filename.to_string_lossy(),
        //     part_num,
        //     db_file.total_packets
        // ));

        let res = FilePartResponse {
            success: true,
            message: String::new(),
        };
        stream.write(&res.encode_to_vec())?;
    }

    Ok(())
}

fn handle_file_description(
    stream: &mut TcpStream,
    db: &Connection,
    user_id: u64,
    target_folder: &Path,
) -> Result<(std::fs::File, FileStatus, DbFile), Box<dyn Error>> {
    let mut buf = [0; 1024];
    let nbytes = stream.read(&mut buf)?;

    let FileDescription {
        name,
        size,
        packet_size,
    } = FileDescription::decode(&buf[..nbytes])?;

    if packet_size < MIN_PACKET_SIZE {
        Err(format!(
            "Invalid Packet Size: Packet Size ({}) must be >= {MIN_PACKET_SIZE}",
            packet_size
        ))?
    }

    let file = db::find_filename(db, &name)?;

    let (mut file, response, dbfile) = match file {
        Some(file) => {
            let status = match file.current_packet() == file.total_packets {
                true => FileStatusEnum::Exists,
                false => FileStatusEnum::Resumeable,
            };

            let file_path = target_folder.join(&file.filename);

            // Ensure the file is *actually* there
            let real_file = match std::path::Path::new(&file_path).exists() {
                true => std::fs::File::options()
                    .read(true)
                    .write(true)
                    .open(file_path),
                false => {
                    logger::info(format!(
                        "The file \"{}\" from db doesn't actually exist, creating it now",
                        file.filename
                            .to_str()
                            .to_error("Invalid UTF-8 parsing filename")
                    ));
                    std::fs::File::create_new(file_path)
                }
            }?;

            // Find file entry in db
            let file_status = FileStatus {
                status: status.into(),
                id: file.id,
                request_packet: file.current_packet(),
                packet_size: file.packet_size,
                total_packets: file.total_packets,
            };

            (
                real_file,
                FileDescriptionResponse {
                    event: Some(Event::Status(file_status)),
                },
                file,
            )
        }
        None => {
            let mut file = std::fs::File::create_new(target_folder.join(&name)).to_error(format!(
                "'{name}' already exists in the {} folder",
                target_folder.to_string_lossy()
            ));
            file.seek(io::SeekFrom::Start(size))?;
            file.write(&[69])?;
            logger::info(format!(
                "Adding new file \"{name}\" with size {}",
                file_size_text(size)
            ));

            let total_packets = num_packets(packet_size, size);
            let db_file = DbFile::new(db, name.into(), total_packets, packet_size, user_id)?;

            let file_res = FileDescriptionResponse {
                event: Some(Event::Status(FileStatus {
                    status: FileStatusEnum::Nonexistent.into(),
                    id: db_file.id,
                    request_packet: 0,
                    packet_size,
                    total_packets,
                })),
            };

            (file, file_res, db_file)
        }
    };

    let seek_pos = dbfile.current_packet() * dbfile.packet_size;
    file.seek(io::SeekFrom::Start(seek_pos))
        .with_warning("Failed to seek to the right part of the file")?;

    stream.write(&response.encode_to_vec())?;
    let file_status = match response.event.unwrap() {
        Event::Status(file_status) => file_status,
        Event::FailMessage(_) => {
            logger::error("There shouldn't be a failure response at this point")
        }
    };
    Ok((file, file_status, dbfile))
}

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    about,
    long_about = "Server for recieving incoming files from the internet from authorized users"
)]
struct Args {
    /// IP:Port to attach on
    #[arg(long)]
    #[arg(default_value = "0.0.0.0:35672")]
    ip: SocketAddr,

    /// The folder to dump all files into
    #[arg(short, long)]
    #[arg(default_value = "stable-ftp-ingress")]
    target_folder: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args { ip, target_folder } = Args::parse();

    std::fs::create_dir_all(&target_folder).to_error("Failed to create folder");

    let listener = TcpListener::bind(ip).to_error("Failed to bind to IP");
    logger::info(&format!("Server listening on {ip}"));

    for conn in listener.incoming() {
        let fname = target_folder.clone();
        match conn.with_warning("Failed to connect") {
            Ok(stream) => {
                std::thread::spawn(move || handle_client(stream, &fname));
            }
            _ => (),
        }
    }
    Ok(())
}
