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

use async_std::{io, task};
use futures::prelude::*;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk, Quorum, Record};
use libp2p::{
    NetworkBehaviour,
    PeerId,
    Swarm,
    build_development_transport,
    identity,
    mdns::{Mdns, MdnsEvent},
    swarm::NetworkBehaviourEventProcess
};
use std::{error::Error, task::{Context, Poll}};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Our peer id: {}", &local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = build_development_transport(local_key)?;

    // We create a custom network behaviour that combines Kademlia and mDNS.
    #[derive(NetworkBehaviour)]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: Mdns
    }

    impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
        // Called when `mdns` produces an event.
        fn inject_event(&mut self, event: MdnsEvent) {
            if let MdnsEvent::Discovered(list) = event {
                for (peer_id, multiaddr) in list {
                    self.kademlia.add_address(&peer_id, multiaddr);
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
                },
                KademliaEvent::GetClosestPeersResult(peers_result) => {
                    println!("Found closest peers: {:?}", &peers_result);
                    for p in self.kademlia.kbuckets_entries() {
                        println!("Entry in our buckets: {:?}", p);
                    }
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
                Poll::Pending => break
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
                    break
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
