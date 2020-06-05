//! Runtime configuration, config files, command line parsing, ...

use anyhow::{Context as AnyhowContext, Result};
use async_std::io;

use libp2p::{identity, identity::ed25519};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

mod error;

#[derive(StructOpt, Debug)]
/// Command line options.
pub struct Opts {
    /// The directory to read configuation files from. 
    #[structopt(long, parse(from_os_str), default_value = ".p2shd")]
    config_dir: PathBuf,

    /// Path to the file storing our Ed25519 keypair. If not given, a file named "node_key" in
    /// `config_dir` will be used.
    #[structopt(long, parse(from_os_str))]
    key_file: Option<PathBuf>,

    /// Peer id of the remote node to connect to. If not given, this program will just print our
    /// own peer id and exit.
    #[structopt()]
    pub remote_id: Option<libp2p::PeerId>,
}

/// Runtime configuration, read from config files and command line arguments.
pub struct Config {
    pub opts: Opts,
}

impl Config {
    /// Set up runtime configuration.
    ///
    /// This includes creating the configuration directory and a node key if
    /// necessary.
    pub fn new(opts: Opts) -> Result<Config> {
        create_config_dir(&opts.config_dir)?;

        Ok(Config { opts })
    }

    /// Read key from file retrieved by `get_key_file`.
    ///
    /// Or create a new one if it does not exist, storing it in the path
    /// returned by `get_key_file` for the next time.
    pub fn get_node_key(&self) -> Result<identity::Keypair> {
        Ok(identity::Keypair::Ed25519(gen_or_get_key(
            &self.get_key_file(),
        )?))
    }

    /// Get the configured key_file, picking a default if not specified.
    fn get_key_file(&self) -> PathBuf {
        match &self.opts.key_file {
            None => [self.opts.config_dir.as_path(), Path::new("node_key")]
                .iter()
                .collect(),
            Some(key_file) => key_file.clone(),
        }
    }
}

/// Create configuration directory if not yet present.
fn create_config_dir(config_path: &Path) -> Result<()> {
    log::debug!("Creating config dir: {:?}", config_path);
    let config_path_exists = path_exists(config_path)
        .with_context(|| error::ConfigDir::Access(PathBuf::from(config_path)))?;

    if !config_path_exists {
        fs::create_dir_all(config_path)
            .with_context(|| error::ConfigDir::Create(PathBuf::from(config_path)))?;

        fs::set_permissions(config_path, PermissionsExt::from_mode(0o700))
            .with_context(|| error::ConfigDir::SetPermissions(PathBuf::from(config_path)))?;
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
    let key_exists =
        path_exists(key_path).with_context(|| error::Keypair::Access(PathBuf::from(key_path)))?;

    if key_exists {
        read_key(key_path)
    } else {
        log::debug!("Writting key: {:?}", key_path);
        gen_and_write_key(key_path)
    }
}

/// Read key file.
fn read_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let mut raw =
        fs::read(key_path).with_context(|| error::Keypair::Read(PathBuf::from(key_path)))?;

    ed25519::Keypair::decode(&mut raw)
        .with_context(|| error::Keypair::Decode(PathBuf::from(key_path)))
}

/// Generate a key and write it to the file given by path.
fn gen_and_write_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let key = ed25519::Keypair::generate();
    let encoded: &[u8] = &key.encode();
    fs::write(key_path, encoded).with_context(|| error::Keypair::Write(PathBuf::from(key_path)))?;

    // Only user should be able to read the file:
    fs::set_permissions(key_path, PermissionsExt::from_mode(0o400))
        .with_context(|| error::Keypair::SetPermissions(PathBuf::from(key_path)))?;
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
