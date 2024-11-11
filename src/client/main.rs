use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
};

use clap::Parser;
use prost::Message;

use stable_ftp::{
    file_size_text, logger, num_packets,
    protos::{
        self,
        file_description_response::{file_status::FileStatusEnum, FileStatus},
        AuthRequest, AuthResponse, FileDescription, FileDescriptionResponse,
    },
};

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    about,
    long_about = "Program for communicating files over the network"
)]
struct Args {
    /// Target ip to send file
    #[arg(short, long)]
    target: SocketAddr,

    /// The file to send
    #[arg(short, long)]
    file: PathBuf,

    /// Personal Access Token to the Server (optional with environment variables)
    #[arg(long)]
    token: Option<String>,

    /// Packet size to use went sending the file.
    /// Larger packets have to do less writing to the database, but may have to send more data if the connection drops
    #[arg(short, long)]
    packet_size: Option<u64>,
}

fn connect() -> Result<FileStatus, Box<dyn std::error::Error>> {
    let args = Args::parse();

    let token = match args.token {
        Some(tok) => tok,
        None => {
            std::env::vars()
                .find(|(k, _)| k == "STABLE_FTP_TOKEN").unwrap_or_else(|| logger::error("Token not specified! Specify it with `--token <TOKEN>` or set as environment variable `STABLE_FTP_TOKEN`"))
                .1
        }
    };

    let auth_request = AuthRequest {
        version: env!("CARGO_PKG_VERSION").into(),
        token,
    };
    let mut stream = TcpStream::connect(args.target)?;
    stream.write(&auth_request.encode_to_vec())?;

    let mut buf = [0; 1024];
    let num_read = stream.read(&mut buf)?;

    match AuthResponse::decode(&buf[..num_read])? {
        AuthResponse {
            success: false,
            failure_reason: msg,
        } => logger::error(format!("Authentication failure: {msg}")),
        _ => logger::info("Auth succeeded!"),
    };

    let file_description = FileDescription::try_from(args.file)?;

    // Add packet size if specified
    let file_description = if let Some(packet_size) = args.packet_size {
        file_description.with_packet_size(packet_size)
    } else {
        file_description
    };

    stream.write(&file_description.encode_to_vec())?;

    let nread = stream.read(&mut buf)?;
    let f_response = FileDescriptionResponse::decode(&buf[..nread])?;

    // Should always return the Some variant
    let file_status = match f_response.event.unwrap() {
        protos::file_description_response::Event::Status(file_status) => file_status,
        protos::file_description_response::Event::FailMessage(message) => logger::error(message),
    };
    let FileStatus {
        request_packet,
        packet_size,
        ..
    } = file_status;

    let num_packets = num_packets(packet_size, file_description.size);

    let file_exists = match file_status.get_status() {
        FileStatusEnum::Exists => {
            assert!(num_packets - request_packet == 0);
            true
        }
        FileStatusEnum::Resumeable => {
            logger::info(format!("File already exists! Resuming with packet size {} on packet number {request_packet}", file_size_text(packet_size)));
            false
        }
        FileStatusEnum::Nonexistent => {
            logger::info(format!("File created!"));
            false
        }
    };

    if file_exists {
        Err(std::io::Error::other("File already exists"))?
    }

    Ok(file_status)
}

fn main() {
    match connect() {
        Err(err) => logger::error(format!("{}", err.to_string())),
        Ok(_) => logger::info("Uploaded file successfully!"),
    }
}
