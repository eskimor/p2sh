/// Runtime configuration, config files, command line parsing, ...

use anyhow::{Context as AnyhowContext, Result};
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
};
use structopt::StructOpt;
use thiserror::Error;

mod error;


#[derive(StructOpt, Debug)]
#[structopt(name = "p2shd")]
/// Command line options.
pub struct Opts {

    /// The directory to read configuation files from. Defaults to the '.p2shd'
    /// directory in the current directory.
    #[structopt(long, parse(from_os_str), default_value = ".p2shd")]
    config_dir: PathBuf,

    /// Path to the file storing our Ed25519 keypair. A file named "node_key" in
    /// `config_dir` will be used.
    #[structopt(long, parse(from_os_str))]
    key_file: Option<PathBuf>,
}

/// Runtime configuration, read from config files and command line arguments.
pub struct Config {
    opts: Opts,
}

impl Config {
    /// Set up runtime configuration.
    ///
    /// This includes creating the configuration directory and a node key if
    /// necessary.
    pub fn new(opts: Opts) -> Config {
        create_config_dir(&opts.config_dir);
        Config {
            opts
        }
    }

    /// Read key from file retrieved by `get_key_file`.
    ///
    /// Or create a new one if it does not exist, storing it in the path
    /// returned by `get_key_file` for the next time.
    pub fn get_node_key(&self) -> Result<identity::Keypair> {
        Ok(identity::Keypair::Ed25519(gen_or_get_key(&self.get_key_file())?))
    }

    /// Get the configured key_file, picking a default if not specified.
    fn get_key_file (&self) -> PathBuf {
        match &self.opts.key_file {
            None => {
                [self.opts.config_dir.as_path(), Path::new("node_key")].iter().collect()
            }
            Some(key_file) => key_file.clone()
        }
    }
}

/// Create configuration directory if not yet present.
fn create_config_dir(config_path: &Path) -> Result<()> {
    let mk_err = |constr: fn (PathBuf) -> error::ConfigDir|
        || constr(PathBuf::from(config_path));

    let config_path_exists = path_exists(config_path)
        .with_context(mk_err(error::ConfigDir::Access))?;

    if config_path_exists {
        fs::create_dir_all(config_path)
            .with_context(mk_err(error::ConfigDir::Create))?;

        fs::set_permissions(config_path, PermissionsExt::from_mode(0o700))
            .with_context(mk_err(error::ConfigDir::SetPermissions))?;
    }
    Ok(())
}

/// Load key from given file path (if present) or generate one and store it.
///
/// # Errors
///
/// 1. File cannot be read for other reasons than "Not Found".
/// 2. Decoding of key fails.
/// 3. File cannot be written.
///
/// If the given file exists but does not contain a valid Ed25519 key.
fn gen_or_get_key(key_path: &Path) -> Result<ed25519::Keypair> {

    let key_exists = path_exists(key_path)
        .with_context(|| error::Keypair::Access(PathBuf::from(key_path)))?;

    if key_exists {
        read_key(key_path)
    } else {
        gen_and_write_key(key_path)
    }
}

/// Read key file.
fn read_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let add_context= |my_self, mk_err| my_self.with_context(|| mk_err(PathBuf::from(key_path)))

    let mut raw = add_context(fs::read(key_path), error::Keypair::Read)?;

    add_context(ed25519::Keypair::decode(&mut raw), error::Keypair::Decode);
}

/// Generate a key and write it to the file given by path.
fn gen_and_write_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let mk_err = |constr| || constr(PathBuf::new(key_path));

    let key = ed25519::Keypair::generate();
    let encoded: &[u8] = &key.encode();
    fs::write(key_path, encoded)
        .with_context(mk_err(error::Keypair::Write))?;

    // Only user should be able to read the file:
    fs::set_permissions(key_path, PermissionsExt::from_mode(0o400))
        .with_context(mk_err(error::Keypair::SetPermissions))?;
    Ok(key)
}

/// Check whether a path exists.
///
/// In contrast to Path::exists() this function really checks whether the path
/// exists, instead of just returning false in case of any error, we only return
/// false on `NotFound`, on all other errors we return the error.
///
/// This improves reporting errors early and more correctly. E.g. Don't tell
/// user that a write failed, when in reality a failed read should have been
/// reported.
fn path_exists(key_path: &Path) -> io::Result<bool> {
    match fs::metadata(key_path) {
        Ok(_) => Ok(true),
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(err)
            }
        }
    }
}
