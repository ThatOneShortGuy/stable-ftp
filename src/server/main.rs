use std::{
    borrow::BorrowMut,
    error::Error,
    io::prelude::*,
    net::{TcpListener, TcpStream},
    time::Duration,
};

use prost::Message;
use stable_ftp::{
    logger::{self, Loggable},
    protos::{
        compare_versions, file_description_response::Event, AuthRequest, AuthResponse,
        FileDescriptionResponse, VersionCompatibility,
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
            handle_auth_err(stream.borrow_mut(), err.to_string());
            return;
        }
    };

    let auth_req = match AuthRequest::decode(&buf[.._bytes_read]) {
        Ok(req) => req,
        Err(err) => {
            handle_auth_err(stream.borrow_mut(), err.to_string());
            return;
        }
    };

    // Verify Versions are compatible
    let server_version = env!("CARGO_PKG_VERSION").into();
    let client_version = &auth_req.version;
    if let VersionCompatibility::Incompatible = compare_versions(&server_version, client_version) {
        handle_auth_err(&mut stream, format!("Version types are Incompatible! Client version ({client_version}) is not compatible with server version ({server_version})"));
        return;
    }

    // TODO: Verify client with SQLite

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

    match handle_file_description(&mut stream) {
        Ok(_) => (),
        Err(err) => {
            let res = FileDescriptionResponse {
                event: Some(Event::FailMessage(err.to_string())),
            };
            stream
                .write(&res.encode_to_vec())
                .to_error("Failed to write to stream");
        }
    }
}

fn handle_file_description(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let mut buf = [0; 1024];

    stream.read(&mut buf)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ip = "127.0.0.1:35672";
    let listener = TcpListener::bind(ip).expect("Failed to bind to IP");
    logger::info(&format!("Server ready on {ip}"));

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                std::thread::spawn(|| handle_client(stream));
            }
            Err(err) => logger::warning(format!("Failed to connect: {err}")),
        }
    }
    Ok(())
}
