pub mod logger;

pub mod protos {

    include!(concat!(env!("OUT_DIR"), "/structure.rs"));

    mod version {
        use std::fmt::Display;

        use super::auth_request::Version;

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
    pub use version::*;

    mod file_description {

        use std::path::PathBuf;

        use super::FileDescription;

        impl TryFrom<PathBuf> for FileDescription {
            fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
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
                })
            }

            type Error = std::io::Error;
        }
    }
}