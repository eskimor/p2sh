use {
    anyhow,
    anyhow::Result,
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
            NetworkBehaviour,
            NetworkBehaviourAction,
            PollParameters
        },
        NetworkBehaviour, PeerId, Swarm,
        Multiaddr,
        core::connection::ConnectionId,
        core::either::EitherOutput,
    },
    std::{
        task::{Context, Poll, Waker},
        error,
        mem
    },
    structopt::StructOpt,
    tokio::sync::{
        watch
    },
};


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
}

impl P2shd {
    pub fn new(local_peer: PeerId, remote_peer: PeerId) -> Result<P2shd> {
        let store = MemoryStore::new(local_peer.clone());
        let kad = Kademlia::new(local_peer.clone(), store);

        let mdns = Mdns::new()?;

        Ok(P2shd {
            kad, mdns,
            local_peer,
            remote_peer,
            waker: None,
        })
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
        if cached.is_empty() {
            self.kad.get_closest_peers(self.remote_peer.clone());
        }
        else {
            println!("Found peer addresses {:?}!", cached);
            // Poll::Ready(NetworkBehaviourAction::GenerateEvent(()))
        }
        Poll::Pending
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
            }
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
