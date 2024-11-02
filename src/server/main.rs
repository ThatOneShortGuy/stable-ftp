use std::{
    borrow::BorrowMut,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};

use prost::Message;
use stable_ftp::{
    logger,
    protos::{compare_versions, AuthRequest, AuthResponse, VersionCompatibility},
};

fn handle_auth_err(stream: &mut TcpStream, msg: impl AsRef<str>) {
    let response = AuthResponse {
        success: false,
        failure_reason: format!("Failed to authenitcate: {}", msg.as_ref()),
    };
    stream
        .write(&response.encode_to_vec())
        .expect("Failed to write to buffer stream");
    return;
}

fn handle_client(mut stream: TcpStream) {
    let mut buf = [0; 1024];

    logger::info(format!(
        "New client connected: {}",
        stream.peer_addr().unwrap()
    ));

    let _bytes_read = match stream.read(&mut buf[..]) {
        Ok(a) => a,
        Err(err) => {
            handle_auth_err(stream.borrow_mut(), err.to_string());
            return;
        }
    };

    let auth_req = match AuthRequest::decode(&buf[..]) {
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
        handle_auth_err(&mut stream, format!("Version types are Incompatible! Client version ({}) is not compatible with server version ({server_version})", auth_req.version));
        return;
    }

    // TODO: Verify client with SQLite

    let response = AuthResponse {
        success: true,
        failure_reason: String::new(),
    };
    stream
        .write(&response.encode_to_vec())
        .unwrap_or_else(|err| {
            logger::error(format!("Failed to return success auth message: {err}"))
        });

    stream
        .set_read_timeout(Some(Duration::new(5, 0)))
        .expect("Failed to set the timeout?!?");
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
