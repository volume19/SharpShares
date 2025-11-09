//! Error types for SharpShares

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArgsError {
    #[error("Must specify hosts using --ldap, --ou, or --hosts")]
    MissingTargetSpec,

    #[error("Failed to create output file: {0}")]
    OutfileCreation(#[from] std::io::Error),

    #[error("Invalid argument: {0}")]
    Invalid(String),
}
