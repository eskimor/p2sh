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

pub struct P2shdHandler<TUserData> {
    kad: KademliaHandler<TUserData>,
}


#[derive(Debug)]
pub enum P2shdHandlerEvent<TUserData> {
    KademliaHandlerEvent(KademliaHandlerEvent),
}

pub enum P2shdHandlerIn<TUserData> {
    KademliaHandlerIn(KademliaHandlerIn<TUserData>)
}

impl<TUserData> ProtocolsHandler for P2shdHandler<TUserData>
    where TUserData: Clone + Send + 'static {
    type InEvent = P2shdHandlerIn<TUserData>;
    type OutEvent = P2shdHandlerEvent<TUserData>;
    type Error = io::Error;
    type InboundProtocol = upgrade::EitherUpgrade<P2shdProtocolConfig, upgrade::DeniedUpgrade>;
    fn into_handler(self, remote_peer_id: &PeerId, connection_point: &ConnectedPoint) -> Self::Handler {
        P2ShdHandler { kad: self.kad.into_handler(remote_peer_id, connection_point) }
    }
}

