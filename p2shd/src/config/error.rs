//! Errors that can happen during configuration handling.

use std::path::PathBuf;
use thiserror::Error;

/// Errors related to keypair serialization.
#[derive(Error, Debug)]
pub enum Keypair {
    #[error(
        "Invalid keyfile '{0}'.

Make sure '{0}' is a valid ED25519 keypair,
which is a private + public key concatenated in binary format.

If you don't mind the node to have a new identity,
you can simply delete the file to have p2shd
generate a valid one for you.
    "
    )]
    Decode(PathBuf),
    #[error("Accessing the keypair at '{0}' failed.")]
    Access(PathBuf),
    #[error("Reading keyfile '{0}' failed.")]
    Read(PathBuf),
    #[error("Writing keyfile '{0}' failed.")]
    Write(PathBuf),
    #[error("Setting permissions for keyfile '{0}' failed.")]
    SetPermissions(PathBuf),
}

/// Errors related to configuration directory handling.
#[derive(Error, Debug)]
pub enum ConfigDir {
    #[error("Accessing the configuration directory at '{0}' failed.")]
    Access(PathBuf),
    #[error("Creating the configuration directory at '{0}' failed.")]
    Create(PathBuf),
    #[error("Setting permissons for the configuration directory at '{0}' failed.")]
    SetPermissions(PathBuf),
}
