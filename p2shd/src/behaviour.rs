use {
    anyhow,
    anyhow::Result,
    async_std::{io, task},
    futures::prelude::*,
    libp2p::{
        build_development_transport,
        kad::record::store::MemoryStore,
        kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk, Quorum, Record, GetClosestPeersResult},
        mdns::{Mdns, MdnsEvent},
        swarm::NetworkBehaviourEventProcess,
        swarm::NetworkBehaviour,
        NetworkBehaviour, PeerId, Swarm,
        Multiaddr,
    },
    std::task::{Context, Poll},
    structopt::StructOpt,
    tokio::sync::{
        watch
    },
};

#[derive(NetworkBehaviour)]
pub struct P2shd {
    kademlia: Kademlia<MemoryStore>,
    mdns: Mdns,
    /// Will be `None` if the event `GetClosestPeersResult` never fired.
    #[behaviour(ignore)]
    closest_peers_recv: watch::Receiver<Option<GetClosestPeersResult>>,
    #[behaviour(ignore)]
    closest_peers_send: watch::Sender<Option<GetClosestPeersResult>>,
}

impl P2shd {
    pub fn new(peer_id: PeerId) -> Result<P2shd> {
        let store = MemoryStore::new(peer_id.clone());
        let kademlia = Kademlia::new(peer_id, store);

        let mdns = Mdns::new()?;
        let (closest_peers_send, mut closest_peers_recv) = watch::channel::<Option<GetClosestPeersResult>>(None);
        // Get rid of that dummy default `None` default value:
        // TODO: Use better abstraction if we need that more than once.
        closest_peers_recv.recv();
        Ok(P2shd { kademlia, mdns, closest_peers_recv, closest_peers_send })
    }

    pub async fn find_node(&mut self, peer_id: &PeerId) -> Result<Vec<Multiaddr>> {
       let cached = self.addresses_of_peer(peer_id);
       // In any case: refresh cache:
       self.kademlia.get_closest_peers(peer_id.clone());
       Ok(if cached.is_empty() {
           self.closest_peers_recv.recv().await;
           // Try again:
           self.addresses_of_peer(peer_id)
       }
       else {
           cached
       }
       )
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
                self.kademlia.add_address(&peer_id, multiaddr);
                self.kademlia.bootstrap();
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for P2shd {
    // Called when `kademlia` produces an event.
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::GetRecordResult(Ok(result)) => {
                for Record { key, value, .. } in result.records {
                    log::trace!(
                        "Got record {:?} {:?}",
                        std::str::from_utf8(key.as_ref()).unwrap(),
                        std::str::from_utf8(&value).unwrap(),
                    );
                }
            }
            KademliaEvent::GetClosestPeersResult(peers_result) => {
                log::trace!("Found closest peers: {:?}", &peers_result);
                self.closest_peers_send.broadcast(Some(peers_result));
                for p in self.kademlia.kbuckets_entries() {
                    log::trace!("Entry in our buckets: {:?}", p);
                }
            }
            KademliaEvent::Discovered {
                peer_id,
                addresses,
                ty,
            } => {
                log::trace!("Discovered peer: {}", peer_id);
                log::trace!("Addresses of that peer: {:?}", addresses);
                log::trace!("Connection status: {:?}", ty);
            }
            KademliaEvent::GetRecordResult(Err(err)) => {
                log::error!("Failed to get record: {:?}", err);
            }
            KademliaEvent::PutRecordResult(Ok(PutRecordOk { key })) => {
                log::trace!(
                    "Successfully put record {:?}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );
            }
            KademliaEvent::PutRecordResult(Err(err)) => {
                log::error!("Failed to put record: {:?}", err);
            }
            _ => {}
        }
    }
}
