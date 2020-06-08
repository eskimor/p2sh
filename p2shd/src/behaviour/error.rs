//! Errors that can happen in functions of the P2shd behaviour.

use thiserror::Error;

use libp2p::Multiaddr;

/// Errors related to keypair serialization.
#[derive(Error, Debug)]
pub enum P2shd {
    #[error("No IP addr/host name found in given multiaddr: '{0}'")]
    NoIPAddrInMultiaddr(Multiaddr),
    #[error(
"Multiple IP addr/host names found in given multiaddr: '{0}'.
Such addresses are not yet supported by p2shd.")
    ]
    MultipleIPAddrInMultiaddr(Multiaddr),
    #[error("Initializing mdns for LAN IP discovery failed.")]
    MdnsInitialization(#[source] std::io::Error),
    #[error("Spawning ssh failed for address '{0}'")]
    SpawningSshFailed(String, #[source] std::io::Error),
}
