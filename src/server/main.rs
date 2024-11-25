use std::{
    error::Error,
    io::{self, prelude::*},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use lazy_marshal::prelude::*;
use rusqlite::Connection;

use stable_ftp::{
    compare_versions,
    db::{self, DbFile},
    file_size_text,
    logger::{self, Loggable},
    num_packets,
    structs::{
        AuthRequest, AuthResponse, FileDescription, FileDescriptionResponse, FilePartResponse,
        FileStatus, FileStatusEnum,
    },
    StreamIterator, VersionCompatibility, MIN_PACKET_SIZE,
};

fn handle_auth_err(stream: &mut TcpStream, msg: impl AsRef<str>) {
    let response = AuthResponse {
        success: false,
        failure_reason: format!("Failed to Authenitcate: {}", msg.as_ref()),
    };
    stream
        .write(&response.marshal().collect::<Vec<_>>())
        .to_error("Failed to write to buffer stream");
    return;
}

fn handle_client(mut stream: TcpStream, target_folder: &Path) {
    logger::info(format!(
        "New client connected: {}",
        stream.peer_addr().to_error("Can't get the peer address??")
    ));

    let mut response_stream = StreamIterator(stream.try_clone().unwrap().bytes());

    let AuthRequest { version, token } = match AuthRequest::unmarshal(&mut response_stream) {
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
            .write(&res.marshal().collect::<Vec<_>>())
            .to_error("Failed to write fail to stream");
        return;
    }
    let user_id = match db::get_user_id(&con, &token) {
        Ok(v) => v,
        Err(err) => {
            let res = FileDescriptionResponse::FailMessage(err.to_string());
            stream
                .write(&res.marshal().collect::<Vec<_>>())
                .to_error("Failed to write to stream");
            return;
        }
    };

    let response = AuthResponse {
        success: true,
        failure_reason: String::new(),
    };
    stream
        .write(&response.marshal().collect::<Vec<_>>())
        .to_error("Failed to return success auth message");

    stream
        .set_read_timeout(Some(Duration::new(5, 0)))
        .to_error("Failed to set the timeout?!?");

    let (file, file_description, db_file) = match handle_file_description(
        &mut stream,
        &mut response_stream,
        &con,
        user_id,
        target_folder,
    ) {
        Ok(file) => file,
        Err(err) => {
            let res = FileDescriptionResponse::FailMessage(err.to_string());
            stream
                .write(&res.marshal().collect::<Vec<_>>())
                .to_error("Failed to write to stream");
            return;
        }
    };

    match recv_files(
        &con,
        &mut stream,
        &mut response_stream,
        file,
        file_description,
        db_file,
    ) {
        Ok(a) => a,
        Err(e) => {
            logger::warning(format!("Failed in recv_files: {}", e.to_string()));
            let res = FilePartResponse {
                success: false,
                message: e.to_string(),
            };
            stream
                .write(&res.marshal().collect::<Vec<_>>())
                .to_error("Failed to write to stream");
            return;
        }
    }
}

fn handle_file_description(
    stream: &mut TcpStream,
    response_stream: &mut StreamIterator,
    db: &Connection,
    user_id: u64,
    target_folder: &Path,
) -> Result<(std::fs::File, FileStatus, DbFile), Box<dyn Error>> {
    let FileDescription {
        name,
        size,
        packet_size,
    } = FileDescription::unmarshal(response_stream)?;

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

            logger::info(format!(
                "Resuming file download for \"{}\" on {}/{}",
                file.filename.to_string_lossy(),
                file.current_packet(),
                file.total_packets
            ));
            (
                real_file,
                FileDescriptionResponse::Status(file_status),
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

            let file_res = FileDescriptionResponse::Status(FileStatus {
                status: FileStatusEnum::Nonexistent.into(),
                id: db_file.id,
                request_packet: 0,
                packet_size,
                total_packets,
            });

            (file, file_res, db_file)
        }
    };

    let seek_pos = dbfile.current_packet() * dbfile.packet_size;
    file.seek(io::SeekFrom::Start(seek_pos))
        .with_warning("Failed to seek to the right part of the file")?;

    stream.write(&response.clone().marshal().collect::<Vec<_>>())?;
    let file_status = match response {
        FileDescriptionResponse::Status(file_status) => file_status,
        FileDescriptionResponse::FailMessage(_) => {
            logger::error("There shouldn't be a failure response at this point")
        }
    };
    Ok((file, file_status, dbfile))
}

fn recv_files(
    con: &Connection,
    stream: &mut TcpStream,
    response_stream: &mut StreamIterator,
    mut file: std::fs::File,
    file_status: FileStatus,
    mut db_file: DbFile,
) -> Result<(), Box<dyn Error>> {
    for current_packet in file_status.request_packet..file_status.total_packets {
        let part_num = u64::unmarshal(response_stream)?;
        let len = usize::unmarshal(response_stream)?;
        let mut data = Vec::with_capacity(len);
        unsafe {
            data.set_len(len);
        }
        stream.read_exact(&mut data)?;

        assert!(
            part_num == current_packet,
            "Part Num: {part_num} =! Expected Num: {current_packet}"
        );
        file.write_all(&data)
            .with_warning("Failed to write data to file")?;
        db_file = db_file
            .inc_current_packet(con)
            .with_warning("Failed to increment current packet in db")?;

        let res = FilePartResponse {
            success: true,
            message: String::new(),
        };
        stream
            .write(&res.marshal().collect::<Vec<_>>())
            .with_warning("Failed to write FilePartResponse to stream")?;
    }

    logger::info(format!(
        "Successfully recieved all the data for \"{}\"",
        db_file.filename.to_string_lossy()
    ));
    Ok(())
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
    ip: String,

    /// The folder to dump all files into
    #[arg(short, long)]
    #[arg(default_value = "stable-ftp-ingress")]
    target_folder: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args { ip, target_folder } = Args::parse();

    std::fs::create_dir_all(&target_folder).to_error("Failed to create folder");

    let listeners = ip
        .to_socket_addrs()?
        .map(|ip| {
            let target_folder = target_folder.clone();
            std::thread::spawn(move || {
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
            })
        })
        .collect::<Vec<_>>();

    for listener in listeners {
        match listener.join() {
            Ok(_) => (),
            Err(_) => (),
        };
    }
    Ok(())
}
