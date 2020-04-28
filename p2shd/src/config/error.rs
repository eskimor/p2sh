/// Errors that can happen during configuration handling.

use anyhow::{Context as AnyhowContext, Result};
use anyhow;
use async_std::{io, task};
use futures::prelude::*;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk, Quorum, Record};
use libp2p::{
    build_development_transport, identity,
    identity::ed25519,
    mdns::{Mdns, MdnsEvent},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour, PeerId, Swarm,
};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    task::{Context, Poll},
    fmt::Display
};
use structopt::StructOpt;
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
    #[error(
    "Accessing the keypair at '{0}' failed."
    )]
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
    #[error(
    "Accessing the configuration directory at '{0}' failed."
    )]
    Access(PathBuf),
    #[error(
    "Creating the configuration directory at '{0}' failed."
    )]
    Create(PathBuf),
    #[error(
    "Setting permissons for the configuration directory at '{0}' failed."
    )]
    SetPermissions(PathBuf),
}

/// Build a context out of errors as defined in this file.
///
/// Usage:
///
/// ```
/// use std::path::Path;
/// let path = Path::new("./keyfile");
/// let add_context = mk_context_fn(PathBuf::from(path));
///
/// add_context(std::fs::read(path), Keypair::Read)
/// ```
///
/// For a better understanding of the type signature see `anyhow::Context`.
pub fn mk_context_fn<T, E, C, CFn, MkCFn>(p: &Path) -> CFn
    where
        C: Display + Send + Sync + 'static,
        CFn: FnOnce(Result<T, E>, MkCFn) -> Result<T, anyhow::Error>,
        MkCFn: FnOnce(PathBuf) -> C,
{
    |my_self, mk_err| -> Result<T, anyhow::Error> {
        my_self.with_context(|| mk_err(PathBuf::from(p)))
    }
}
