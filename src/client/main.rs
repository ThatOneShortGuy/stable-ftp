use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
};

use clap::Parser;
use prost::Message;

use stable_ftp::{
    logger,
    protos::{AuthRequest, AuthResponse, FileDescription},
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
}

fn connect() -> Result<(), Box<dyn std::error::Error>> {
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

    let mut buf = Vec::with_capacity(1024);
    stream.read_to_end(&mut buf)?;

    match AuthResponse::decode(buf.as_slice())? {
        AuthResponse {
            success: false,
            failure_reason: msg,
        } => logger::error(msg),
        _ => (),
    };

    let file_description = FileDescription::try_from(args.file)?;
    stream.write(&file_description.encode_to_vec())?;

    Ok(())
}

fn main() {
    match connect() {
        Err(err) => logger::error(err.to_string()),
        Ok(_) => logger::info("Uploaded file successfully!"),
    }
}
