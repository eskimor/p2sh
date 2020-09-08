= p2sh =

The goal of this project is an alternative secure shell that works over a p2p
network. Instead of knowing the (dynamic) IP of a node, one only needs to know
the static id of node. The connection will be made via the p2p network,
offering features like NAT traversal and robust connections via QUIC.


= Status =

Right now this project is a PoC - you can connect to public IP addresses via
their id. But it is rather crude, unreliable and does not yet support NAT
traversal or QUIC. In fact the actual connection is still made via the plain
ssh executable at the moment.


= Roadmap =

1. Replace calling of ssh executable with
   [thrussh](https://crates.io/crates/thrussh/). This is both a preparing
   step and also improves the process of filtering out valid IP addresses.
2. Use the P2P connection itself for tunnelling of the ssh traffic.
3. Use [Quic](https://tools.ietf.org/html/draft-ietf-quic-transport-29) instead of TCP.
4. Implement relay nodes in rust-ipfs for NAT traversal.
5. Implement UDP hole punching in rust-ipfs for NAT traversal.

= Future work =

Initially this project was inspired by [mosh](https://mosh.org/), which is a
super reliable shell that never dies. We should be able to get the most
important benefits of mosh by just using Quic, but if it seems worthwhile we
might want to implement its state synchronization protocol as well.
