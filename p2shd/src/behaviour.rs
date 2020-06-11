use {
    async_std::{io, task},
    futures::prelude::*,
    libp2p::{
        build_development_transport,
        kad::handler::KademliaHandler,
        kad::record::store::MemoryStore,
        kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk,
            Quorum, Record, GetClosestPeersResult,
            QueryId,
            handler::KademliaHandlerIn,
        },
        mdns::{Mdns, MdnsEvent},
        swarm::{
            NetworkBehaviourEventProcess,
            NetworkBehaviourAction,
            NetworkBehaviour,
            PollParameters
        },
        NetworkBehaviour, PeerId, Swarm,
        Multiaddr,
        multiaddr::Protocol,
        core::connection::ConnectionId,
        core::either::EitherOutput,
    },
    std::{
        task::{Context, Poll, Waker},
        mem,
        process::Command,
        result,
        convert::From,
    },
    structopt::StructOpt,
    tokio::sync::{
        watch
    },
};

pub mod error;

/// Result type with errors specific to this module.
type Result<T> = result::Result<T, error::P2shd>;

#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll")]
pub struct P2shd {
    kad: Kademlia<MemoryStore>,
    mdns: Mdns,
    #[behaviour(ignore)]
    local_peer: PeerId,
    #[behaviour(ignore)]
    /// The peer we are supposed to connect to.
    remote_peer: PeerId,
    #[behaviour(ignore)]
    /// Waker of the poll function.
    waker: Option<Waker>,
    #[behaviour(ignore)]
    querying: bool,
}

impl P2shd {
    pub fn new(local_peer: PeerId, remote_peer: PeerId) -> Result<P2shd> {
        let store = MemoryStore::new(local_peer.clone());
        let mut kad = Kademlia::new(local_peer.clone(), store);
        P2shd::add_bootstrap_nodes(&mut kad);
        kad.bootstrap();

        let mdns = Mdns::new().map_err(error::P2shd::MdnsInitialization)?;

        Ok(P2shd {
            kad, mdns,
            local_peer,
            remote_peer,
            waker: None,
            querying: false,
        })
    }

    fn add_bootstrap_nodes(kad: &mut Kademlia<MemoryStore>) {
        let gm_addr = "/ip4/81.223.86.162/tcp/22222".parse().expect("Bootstrap GM node has invalid format!");
        let gm_id = "12D3KooWRmrTKbuneCQMHAjiGyUTZZu6NZP1XpTMuJJZotTdgYTm".parse().expect("GM ipfs node id is invalid!");
        // let gm_ipfs_addr = "/ip4/81.223.86.162/tcp/4001".parse().expect("Bootstrap GM node has invalid format!");
        // let gm_ipfs_id = "QmPqXagznBmhiX48Nd52XEcf8xpabE8d97ExLz7oWKQvd7".parse().expect("GM ipfs node id is invalid!");
        kad.add_address(&gm_id, gm_addr);
        // kad.add_address(&gm_ipfs_id, gm_ipfs_addr);
    }

    // pub async fn find_node(&mut self, peer_id: &PeerId) -> Result<Vec<Multiaddr>> {
    //    let cached = self.addresses_of_peer(peer_id);
       // In any case: refresh cache:
    //    self.kad.get_closest_peers(peer_id.clone());
    //    Ok(if cached.is_empty() {
    //        self.closest_peers_recv.recv().await;
           // Try again:
    //        self.addresses_of_peer(peer_id)
    //    }
    //    else {
    //        cached
    //    }
    //    )
    // }



    fn poll(&mut self, cx: &mut Context, params: &mut impl PollParameters)
        -> Poll<NetworkBehaviourAction<EitherOutput<KademliaHandlerIn<QueryId>, void::Void>, ()>> {
        self.waker = Some(cx.waker().clone());
        let cached  = self.addresses_of_peer(&self.remote_peer.clone());
        if cached.is_empty() && !self.querying {
            self.querying = true;
            self.kad.get_closest_peers(self.remote_peer.clone());
            Poll::Pending
        }
        else if self.querying {
            Poll::Pending
        } else {
            println!("Found peer addresses {:?}!", cached);
            let node_addrs = cached.iter()
                .filter_map(|x| host_addr_from_multiaddr(x).ok())
                .filter(|a| a != "127.0.0.1" && a != "::1" && a != "localhost");
            for addr in node_addrs {
                log::info!("Connecting to: {}", &addr);
                let r = Command::new("ssh")
                    .arg(&addr)
                    .spawn();
                match r {
                    Ok(mut h) => {
                        h.wait();
                        std::process::exit(0);
                    }
                    Err(e) => {
                        log::info!("Failed running ssh for {}, with: {:?} ", addr, e);
                    }
                }
            }
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(()))
        }
    }

    /// Wake if the given peer_id matches `remote_peer`.
    ///
    /// Clearing the waker afterwards (only one
    /// wake).
    fn wake_on_found(&mut self, peer_id: &PeerId) {
        if *peer_id == self.remote_peer {
            match mem::replace(&mut self.waker, None) {
                None => (),
                Some(w) => w.wake(),
            }
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for P2shd {
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, multiaddr) in list {
                log::trace!(
                    "MDNS, discovered peer {} with address {}!",
                    peer_id, multiaddr
                );
                self.kad.add_address(&peer_id, multiaddr);
                self.kad.bootstrap();
                self.wake_on_found(&peer_id);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for P2shd {
    // Called when `kademlia` produces an event.
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::Discovered {
                peer_id,
                addresses,
                ty,
            } => {
                log::trace!("Discovered peer: {}", peer_id);
                log::trace!("Addresses of that peer: {:?}", addresses);
                log::trace!("Connection status: {:?}", ty);
                self.wake_on_found(&peer_id);
            }
            _ => { log::debug!("Kademlia event: {:?}", message);
            }
        }
    }
}

/// Get host addr (dns name, IPv4, IPv6 address) from the given multiaddr as `String` ready to be
/// passed to ssh for example.
fn host_addr_from_multiaddr(m_addr: &Multiaddr) -> Result<String> {
    let ips = m_addr
        .iter()
        .filter_map(to_host_addr);
    match ips.collect::<Vec<String>>().as_slice() {
        [] => Err(error::P2shd::NoIPAddrInMultiaddr(m_addr.clone())),
        [a] => Ok(a.clone()),
        _ => Err(error::P2shd::MultipleIPAddrInMultiaddr(m_addr.clone())),
    }
}

fn to_host_addr(p: Protocol) -> Option<String> {
    use Protocol::{*};
    match p {
        Dnsaddr(a)  => Some(format!("{}", a)),
        Dns6(a) => Some(format!("{}", a)),
        Dns4(a) => Some(format!("{}", a)),
        Ip4(a)  => Some(format!("{}", a)),
        Ip6(a)  => Some(format!("{}", a)),
        _ => None,
    }
}
