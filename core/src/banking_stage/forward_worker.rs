//! Forward worker: relays transactions to peers when the local node decides not
//! to include them immediately (e.g. under queue pressure).

use solana_sdk::packet::Packet;
use std::net::UdpSocket;

pub struct ForwardWorker {
    socket: UdpSocket,
}

impl ForwardWorker {
    pub fn new(socket: UdpSocket) -> Self { Self { socket } }

    pub fn forward(&self, packets: &[Packet], peer_addr: std::net::SocketAddr) {
        for p in packets {
            let _ = self.socket.send_to(p.data(..).unwrap_or(&[]), peer_addr);
        }
    }
}
