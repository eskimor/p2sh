use {
    anyhow,
    anyhow::Result,
    async_std::{io, task},
    futures::prelude::*,
    libp2p::{
        build_development_transport,
        kad::record::store::MemoryStore,
        kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk, Quorum, Record},
        mdns::{Mdns, MdnsEvent},
        swarm::NetworkBehaviourEventProcess,
        NetworkBehaviour, PeerId, Swarm,
    },
    std::task::{Context, Poll},
    structopt::StructOpt,
};

use p2shd::{behaviour::P2shd, config, config::Config};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = Config::new(config::Opts::from_args())?;

    match &cfg.opts.remote_id {
        None => {
            let local_key = cfg.get_node_key()?;
            let local_peer_id = PeerId::from(local_key.public());
            println!("Our peer id: {}", &local_peer_id);
            Ok(())
        }
        Some(remote_id) => {
            start(&cfg, remote_id)
        }
    }
}

fn start(cfg: &Config, remote_peer_id: &PeerId) -> Result<()> {
    let local_key = cfg.get_node_key()?;
    let local_peer_id = PeerId::from(local_key.public());
    log::info!("Our peer id: {}", &local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = build_development_transport(local_key)?;

    // We create a custom network behaviour that combines Kademlia and mDNS.

    // Create a swarm to manage peers and events.
    let mut swarm = {
        let behaviour = P2shd::new(local_peer_id.clone(), remote_peer_id.clone())?;
        Swarm::new(transport, behaviour, local_peer_id)
    };

    // Listen on all interfaces and whatever port the OS assigns.
    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/4001".parse()?)?;

    let mut listening = false;
    task::block_on(future::poll_fn(move |cx: &mut Context| {
        loop {
            match swarm.poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => log::info!("{:?}", event),
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => {
                    if !listening {
                        if let Some(a) = Swarm::listeners(&swarm).next() {
                            log::info!("Listening on {:?}", a);
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
