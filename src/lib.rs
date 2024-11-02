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
}
