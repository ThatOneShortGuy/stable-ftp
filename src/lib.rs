pub mod db;
pub mod logger;
pub mod structs;

pub const DEFAULT_PACKET_SIZE: u64 = 2_u64.pow(22);
pub const MIN_PACKET_SIZE: u64 = 2u64.pow(20);
const POSTFIX_SIZES: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

impl FileStatus {
    pub fn get_status(&self) -> FileStatusEnum {
        self.status
    }
}

mod version {
    use std::fmt::Display;

    use crate::structs::Version;

    impl From<&str> for Version {
        fn from(value: &str) -> Self {
            let parts = value
                .split(".")
                .map(|val| val.parse().unwrap())
                .collect::<Vec<_>>();
            assert!(parts.len() == 3);

            Version {
                major: parts[0],
                minor: parts[1],
                patch: parts[2],
            }
        }
    }

    pub enum VersionCompatibility {
        Compatible,
        Incompatible,
    }

    pub fn compare_versions(
        server_version: &Version,
        client_version: &Version,
    ) -> VersionCompatibility {
        if server_version.major != client_version.major {
            VersionCompatibility::Incompatible
        } else if server_version.minor < client_version.minor {
            VersionCompatibility::Incompatible
        } else {
            VersionCompatibility::Compatible
        }
    }

    impl Display for Version {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}
use std::{io::Bytes, net::TcpStream};

use structs::{FileStatus, FileStatusEnum};
pub use version::*;

mod file_description {
    use crate::{structs::FileDescription, DEFAULT_PACKET_SIZE};
    use std::path::PathBuf;

    impl TryFrom<&PathBuf> for FileDescription {
        type Error = std::io::Error;

        fn try_from(value: &PathBuf) -> Result<Self, Self::Error> {
            let filename = match value.file_name() {
                Some(f) => match f.to_str() {
                    Some(s) => s.to_string(),
                    None => Err(std::io::Error::other("Can't convert path to normal String"))?,
                },
                None => Err(std::io::Error::other("Failed to get the filename"))?,
            };
            let size = std::fs::File::open(value)?.metadata()?.len();

            Ok(Self {
                name: filename,
                size,
                packet_size: DEFAULT_PACKET_SIZE,
            })
        }
    }

    impl FileDescription {
        pub fn with_packet_size(mut self, packet_size: u64) -> Self {
            self.packet_size = packet_size;
            self
        }
    }
}

pub fn num_packets(packet_size: u64, file_size: u64) -> u64 {
    (file_size as f64 / packet_size as f64).ceil() as u64
}

pub fn file_size_text(file_size: u64) -> String {
    let (divisor, postfix) = POSTFIX_SIZES
        .iter()
        .enumerate()
        .reduce(
            |acc, (i, val)| match 1 > 2_i64.pow(i as u32 * 10) - file_size as i64 {
                true => (i, val),
                false => acc,
            },
        )
        .unwrap();

    format!(
        "{:0.2} {postfix}",
        file_size as f64 / 2_i64.pow(divisor as u32 * 10) as f64
    )
}

pub struct StreamIterator(pub Bytes<TcpStream>);

impl Iterator for StreamIterator {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        Some(unsafe { self.0.next()?.unwrap_unchecked() })
    }
}
