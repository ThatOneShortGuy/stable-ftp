use lazy_marshal::prelude::*;

pub type Id = i32;

#[derive(Debug, Clone, Copy, Marshal, UnMarshal)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct AuthRequest {
    pub version: Version,
    pub token: String,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct AuthResponse {
    pub success: bool,
    pub failure_reason: String,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct FileDescription {
    pub name: String,
    pub size: u64,
    pub packet_size: u64,
}

#[derive(Debug, Clone, Copy, Marshal, UnMarshal)]
pub enum FileStatusEnum {
    Exists,
    Resumeable,
    Nonexistent,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct FileStatus {
    pub id: Id,
    pub status: FileStatusEnum,
    pub request_packet: u64,
    pub packet_size: u64,
    pub total_packets: u64,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub enum FileDescriptionResponse {
    Status(FileStatus),
    FailMessage(String),
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct FilePart {
    pub part_num: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Marshal, UnMarshal)]
pub struct FilePartResponse {
    pub success: bool,
    pub message: String,
}
