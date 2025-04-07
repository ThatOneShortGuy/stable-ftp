use std::{
    error::Error,
    fs,
    io::{Read, Seek, Write},
    net::TcpStream,
    path::PathBuf,
};

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_marshal::prelude::*;

use stable_ftp::{
    DEFAULT_PACKET_SIZE, MIN_PACKET_SIZE, StreamIterator, file_size_text,
    logger::{self, Loggable},
    num_packets,
    structs::{
        AuthRequest, AuthResponse, FileDescription, FileDescriptionResponse, FilePart,
        FilePartResponse, FileStatus, FileStatusEnum,
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
    target: String,

    /// The file to send
    #[arg(short, long)]
    file: PathBuf,

    /// Personal Access Token to the Server (optional with environment variables)
    #[arg(long)]
    token: Option<String>,

    /// Packet size to use went sending the file.
    /// Larger packets have to do less writing to the database, but may have to send more data if the connection drops
    #[arg(short, long)]
    #[arg(default_value_t = DEFAULT_PACKET_SIZE)]
    packet_size: u64,
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

    if args.packet_size < MIN_PACKET_SIZE {
        logger::error(format!(
            "packet size ({}) must be >= {MIN_PACKET_SIZE}",
            args.packet_size
        ))
    }

    let auth_request = AuthRequest {
        version: env!("CARGO_PKG_VERSION").into(),
        token,
    };
    logger::info(format!("Connecting to {}", args.target));
    let mut stream = TcpStream::connect(args.target)?;
    logger::info(format!("Connected to {}", stream.peer_addr()?));
    stream.write(&auth_request.marshal().collect::<Vec<_>>())?;

    let mut response_stream = StreamIterator(stream.try_clone().unwrap().bytes());

    match AuthResponse::unmarshal(&mut response_stream)? {
        AuthResponse {
            success: false,
            failure_reason: msg,
        } => logger::error(format!("Authentication failure: {msg}")),
        _ => logger::info("Auth succeeded!"),
    };

    let file_description =
        FileDescription::try_from(&args.file)?.with_packet_size(args.packet_size);

    stream.write(&file_description.clone().marshal().collect::<Vec<_>>())?;

    let f_response = FileDescriptionResponse::unmarshal(&mut response_stream)?;

    // Should always return the Some variant
    let file_status = match f_response {
        FileDescriptionResponse::Status(file_status) => file_status,
        FileDescriptionResponse::FailMessage(message) => logger::error(message),
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
            logger::info(format!(
                "File already exists! Resuming with packet size {} on packet number {request_packet}/{num_packets}",
                file_size_text(packet_size)
            ));
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

    let filename = args.file;
    let mut file = fs::File::open(&filename)?;
    file.seek(std::io::SeekFrom::Start(request_packet * packet_size))?;
    let mut buf: Vec<u8> = vec![69; packet_size as usize];

    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] [{human_pos}/{human_len}] {wide_bar} ETA: {eta_precise}",
    )?;
    let bar = ProgressBar::new(num_packets)
        .with_style(style)
        .with_position(request_packet);
    for part_num in request_packet..num_packets {
        let r = file.read(&mut buf[..packet_size as usize])?;

        if r < packet_size as usize {
            assert!(file.read(&mut buf[..])? == 0) // Ensure we've actually read to the end of the file
        }
        let file_part = FilePart {
            part_num,
            data: buf[..r].to_vec(),
        };
        stream.write(&file_part.marshal().collect::<Vec<_>>())?;

        let res = FilePartResponse::unmarshal(&mut response_stream)?;
        if res.success == false {
            logger::error(format!("Failed to upload file: {}", res.message))
        }
        bar.inc(1);
    }
    bar.finish_and_clear();

    Ok(file_status)
}

fn main() -> Result<(), Box<dyn Error>> {
    connect().to_error("");
    logger::info("Uploaded file successfully!");
    Ok(())
}
