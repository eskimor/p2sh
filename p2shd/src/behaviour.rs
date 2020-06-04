use {
    anyhow,
    anyhow::Result,
    async_std::{io, task},
    futures::prelude::*,
    libp2p::{
        build_development_transport,
        kad::handler::KademliaHandler,
        kad::record::store::MemoryStore,
        kad::{record::Key, Kademlia, KademliaEvent, PutRecordOk, Quorum, Record, GetClosestPeersResult},
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
    },
    std::{
        task::{Context, Poll},
        error,
    },
    structopt::StructOpt,
    tokio::sync::{
        watch
    },
};

use crate::handler::{
    P2shdHandlerIn, P2shdHandler
};

mod full;


#[derive(NetworkBehaviour)]
pub struct P2shd {
}

enum P2shdEvent {
    MdnsEvent(MdnsEvent),
    KademliaEvent(KademliaEvent),
}



// For educational purposes we implement this behaviour by hand. This can be changed anytime into 
// implementing a behaviour for just the p2sh functionality and build the overall behaviour consisting
// of mdns, kademlia and p2sh by means of `NetworkBehaviourEventProcess` and deriving.
impl NetworkBehaviour for P2shd {

    type ProtocolsHandler = P2shdHandler;
    type OutEvent = P2shdEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        P2shdHandler { kad : self.kademlia.new_handler() }
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        // Return mdns addresses first, if there are any, they are the best and fastest addresses
        // (local). If there are none, the vector is empty anyway.
        self.mdns.addresses_of_peer(peer_id).append(self.kademlia.addresses_of_peer(peer_id))
    }


    fn inject_connection_established(&mut self, peer: &PeerId, conn_id: &ConnectionId, endpoint: &ConnectedPoint) {
        self.kademlia.inject_connection_established(peer, conn_id, endpoint);
        self.mdns.inject_connection_established(peer, conn_id, endpoint);
    }

    fn inject_connected(&mut self, peer: &PeerId) {
        self.kademlia.inject_connected(peer);
        self.mdns.inject_connected(peer);
    }

    fn inject_addr_reach_failure(
        &mut self, peer_id: Option<&PeerId>, addr: &Multiaddr,
        err: &dyn error::Error
        ) {
        self.kademlia.inject_addr_reach_failure(peer_id, addr);
        self.mdns.inject_addr_reach_failure(peer_id, addr);
    }
    fn inject_dial_failure(&mut self, peer_id: &PeerId) {
        self.kademlia.inject_dial_failure(peer_id);
        self.mdns.inject_dial_failure(peer_id);
    }
    fn inject_disconnected(&mut self, id: &PeerId) {
        self.kademlia.inject_disconnected(id);
        self.mdns.inject_disconnected(id);
    }
    fn inject_event(
        &mut self,
        source: PeerId,
        connection: ConnectionId,
        event: KademliaHandlerEvent<QueryId>
        ){
        match event {
            KademliaEvent(ev) => self.kademlia.inject_event(ev),
            MdnsEvent(ev) => self.mdns.inject_event(ev),
        }
    }
    fn poll(&mut self, cx: &mut Context, parameters: &mut impl PollParameters) -> Poll<
        NetworkBehaviourAction<
            <P2shdHandler<QueryId> as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        let kad_r = self.kademlia.poll(cx, parameters);
        match kad_r {
            Poll::Pending => self.mdns.poll(cx.parameters),
            _ => kad_r,
        }
    }
}

