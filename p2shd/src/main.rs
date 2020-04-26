//! A basic key value store demonstrating libp2p and the mDNS and Kademlia protocols.
//!
//! 1. Using two terminal windows, start two instances. If you local network
//!    allows mDNS, they will automatically connect.
//!
//! 2. Type `PUT my-key my-value` in terminal one and hit return.
//!
//! 3. Type `GET my-key` in terminal two and hit return.
//!
//! 4. Close with Ctrl-c.

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


#[derive(StructOpt, Debug)]
#[structopt(name = "p2shd")]
struct Config {
    /// The directory to read configuation files from. Defaults to the '.p2shd' directory in the
    /// current directory.
    #[structopt(long, parse(from_os_str), default_value = ".p2shd")]
    config_dir: PathBuf,
    /// Path to the file storing our Ed25519 keypair. A file named "node_key" in `config_dir`
    /// will be used.
    #[structopt(long, parse(from_os_str))]
    key_file: Option<PathBuf>,
}

impl Config {
    /// Get the configured key_file, picking a default if not specified.
    fn get_key_file (&self) -> PathBuf {
        match &self.key_file {
            None => {
                [self.config_dir.as_path(), Path::new("node_key")].iter().collect()
            }
            Some(key_file) => key_file.clone()
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(
    "Invalid keyfile '{0}'.

Make sure '{0}' is a valid ED25519 keypair,
which is a private + public key concatenated in binary format.

If you don't mind the node to have a new identity,
you can simply delete the file to have p2shd
generate a valid one for you.
    "
    )]
    DecodeKeypair(PathBuf),
    #[error(
    "Accessing the keypair at '{0}' failed."
    )]
    AccessKeypair(PathBuf),
    #[error("Reading keyfile '{0}' failed.")]
    ReadKeypair(PathBuf),
    #[error("Writing keyfile '{0}' failed.")]
    WriteKeypair(PathBuf),
    #[error("Setting permissions for keyfile '{0}' failed.")]
    SetPermissions(PathBuf),
}

/// Load key from given file path (if present) or generate one and store it.
///
/// # Errors
///
/// 1. File cannot be read for other reasons than "Not Found".
/// 3. Decoding of key fails.
/// 2. File cannot be written.
///
/// If the given file exists but does not contain a valid Ed25519 key.
pub fn gen_or_get_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let key_exists = path_exists(key_path)
        .with_context(|| Error::AccessKeypair(PathBuf::from(key_path)))?;

    if key_exists {
        read_key(key_path)
    } else {
        gen_and_write_key(key_path)
    }
}

/// Read key file.
fn read_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let mut raw = fs::read(key_path)
        .with_context(|| Error::ReadKeypair(PathBuf::from(key_path)))?;
    ed25519::Keypair::decode(&mut raw)
        .with_context(|| Error::DecodeKeypair(PathBuf::from(key_path)))
}

/// Generate a key and write it to the file given by path.
fn gen_and_write_key(key_path: &Path) -> Result<ed25519::Keypair> {
    let key = ed25519::Keypair::generate();
    let encoded: &[u8] = &key.encode();
    fs::write(key_path, encoded)
        .with_context(|| Error::WriteKeypair(PathBuf::from(key_path)))?;
    // Only user should be able to read the file:
    fs::set_permissions(key_path, PermissionsExt::from_mode(0o400))
        .with_context(|| Error::SetPermissions(PathBuf::from(key_path)))?;
    Ok(key)
}

/// Check whether a path exists.
///
/// In contrast to Path::exists() this function really checks whether the path exists, instead of
/// just returning false in case of any error, we only return false on `NotFound`, on all other
/// errors we return an error.
///
/// This improves reporting errors early and more correctly. E.g. Don't tell user that a write
/// failed, when in reality a a faild read should have been reported.
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

fn create_config_dir(config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        fs::create_dir_all(config_path)?;
        fs::set_permissions(config_path, PermissionsExt::from_mode(0o700))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let cfg = Config::from_args();
    create_config_dir(&cfg.config_dir)?;

    // Create a random key for ourselves.
    let local_key = identity::Keypair::Ed25519(gen_or_get_key(&cfg.get_key_file())?);
    let local_peer_id = PeerId::from(local_key.public());
    println!("Our peer id: {}", &local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = build_development_transport(local_key)?;

    // We create a custom network behaviour that combines Kademlia and mDNS.
    #[derive(NetworkBehaviour)]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: Mdns,
    }

    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        // Called when `mdns` produces an event.
        fn inject_event(&mut self, event: MdnsEvent) {
            if let MdnsEvent::Discovered(list) = event {
                for (peer_id, multiaddr) in list {
                    println!(
                        "MDNS, discovered peer {} with address {}!",
                        peer_id, multiaddr
                    );
                    self.kademlia.add_address(&peer_id, multiaddr);
                    self.kademlia.bootstrap();
                }
            }
        }
    }

    impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
        // Called when `kademlia` produces an event.
        fn inject_event(&mut self, message: KademliaEvent) {
            match message {
                KademliaEvent::GetRecordResult(Ok(result)) => {
                    for Record { key, value, .. } in result.records {
                        println!(
                            "Got record {:?} {:?}",
                            std::str::from_utf8(key.as_ref()).unwrap(),
                            std::str::from_utf8(&value).unwrap(),
                        );
                    }
                }
                KademliaEvent::GetClosestPeersResult(peers_result) => {
                    println!("Found closest peers: {:?}", &peers_result);
                    for p in self.kademlia.kbuckets_entries() {
                        println!("Entry in our buckets: {:?}", p);
                    }
                }
                KademliaEvent::Discovered {
                    peer_id,
                    addresses,
                    ty,
                } => {
                    println!("Discovered peer: {}", peer_id);
                    println!("Addresses of that peer: {:?}", addresses);
                    println!("Connection status: {:?}", ty);
                }
                KademliaEvent::GetRecordResult(Err(err)) => {
                    eprintln!("Failed to get record: {:?}", err);
                }
                KademliaEvent::PutRecordResult(Ok(PutRecordOk { key })) => {
                    println!(
                        "Successfully put record {:?}",
                        std::str::from_utf8(key.as_ref()).unwrap()
                    );
                }
                KademliaEvent::PutRecordResult(Err(err)) => {
                    eprintln!("Failed to put record: {:?}", err);
                }
                _ => {}
            }
        }
    }

    // Create a swarm to manage peers and events.
    let mut swarm = {
        // Create a Kademlia behaviour.
        let store = MemoryStore::new(local_peer_id.clone());
        let kademlia = Kademlia::new(local_peer_id.clone(), store);
        let mdns = Mdns::new()?;
        let behaviour = MyBehaviour { kademlia, mdns };
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Listen on all interfaces and whatever port the OS assigns.
    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Kick it off.
    let mut listening = false;
    task::block_on(future::poll_fn(move |cx: &mut Context| {
        loop {
            match stdin.try_poll_next_unpin(cx)? {
                Poll::Ready(Some(line)) => handle_input_line(&mut swarm.kademlia, line),
                Poll::Ready(None) => panic!("Stdin closed"),
                Poll::Pending => break,
            }
        }
        loop {
            match swarm.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => println!("{:?}", event),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => {
                    if !listening {
                        if let Some(a) = Swarm::listeners(&swarm).next() {
                            println!("Listening on {:?}", a);
                            listening = true;
                        }
                    }
                    break;
                }
            }
        }
        Poll::Pending
    }))
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) {
    let mut args = line.split(" ");

    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_record(&key, Quorum::One);
            kademlia.get_closest_peers(key);
        }
        Some("PUT") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        eprintln!("Expected value");
                        return;
                    }
                }
            };
            let record = Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            kademlia.put_record(record, Quorum::One);
        }
        _ => {
            eprintln!("expected GET or PUT");
        }
    }
}

// fn main() {
//     let raw_stdin = 0;
//     let mut termios = Termios::from_fd(raw_stdin).expect("Stdin is not a tty!");
//     println!("Your terminal is: {:?}", get_tty_path());
//     println!("Terminal settings: {:?}", termios);
//     if termios.c_lflag & ICANON != 0 {
//         println!("Terminal is canon!");
//     }
//     else {
//         println!("Terminal is not canon");
//     }
//     println!("VTIME: {}", termios.c_cc[VTIME]);
//     println!("VMIN: {}", termios.c_cc[VMIN]);
// }

// fn get_tty_path() -> PathBuf {
//     let pid = process::id();
//     let path = format!("/proc/{}/fd/0", pid);
//     let path = Path::new(&path);
//     path.canonicalize().expect("Invalid path")
// }
