use super::{Event, OutAddr, OutPacket, Packet, PacketSender, Peer, SendError};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

pub struct PeerManagerInfo {
    /// The number of online clients
    pub online: i32,
    /// The number of idle clients(not sending packets for 30s)
    pub idle: i32,
}

struct InnerPeerManager {
    /// real ip to peer map
    cache: HashMap<SocketAddr, Peer>,
    /// key is the inner ip in virtual LAN, value is cache's key
    /// a client may have more than one inner ip.
    map: HashMap<Ipv4Addr, SocketAddr>,

    ignore_idle: bool,

    packet_tx: PacketSender,
}

impl InnerPeerManager {
    fn new(packet_tx: PacketSender, ignore_idle: bool) -> Self {
        Self {
            cache: HashMap::new(),
            map: HashMap::new(),
            ignore_idle,
            packet_tx,
        }
    }
}

#[derive(Clone)]
pub struct PeerManager {
    inner: Arc<RwLock<InnerPeerManager>>,
}

impl PeerManager {
    pub fn new(packet_tx: PacketSender, ignore_idle: bool) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerPeerManager::new(packet_tx, ignore_idle))),
        }
    }
    pub async fn remove(&self, addr: &SocketAddr) {
        let cache = &mut self.inner.write().await.cache;
        cache.remove(&addr);
    }
    pub async fn peer_mut<F>(&self, addr: &SocketAddr, event_send: &mpsc::Sender<Event>, func: F)
    where
        F: FnOnce(&mut Peer) -> (),
    {
        let cache = &mut self.inner.write().await.cache;
        let peer = cache
            .entry(*addr)
            .or_insert_with(|| Peer::new(*addr, event_send.clone()));
        func(peer)
    }
    pub async fn send_broadcast(&self, packet: OutPacket) -> std::result::Result<usize, SendError> {
        let (packet, _) = packet.split();
        let len = packet.len();
        let inner = &mut self.inner.write().await;
        let mut packet_tx = inner.packet_tx.clone();
        let addrs = inner
            .cache
            .iter()
            .map(|(addr, _)| *addr)
            .collect::<Vec<_>>();
        let size: usize = addrs.len() * len;
        packet_tx.send((packet, addrs)).await?;
        Ok(size)
    }
    pub async fn get_dest_sockaddr(&self, from: SocketAddr, out_addr: OutAddr) -> Vec<SocketAddr> {
        let inner = &mut self.inner.write().await;
        inner.map.insert(*out_addr.src_ip(), from);
        if let Some(addr) = inner.map.get(&out_addr.dst_ip()) {
            vec![*addr]
        } else {
            let addrs = inner
                .cache
                .iter()
                .filter(|(_, i)| !inner.ignore_idle || i.state.is_connected())
                .filter(|(addr, _)| &&from != addr)
                .map(|(addr, _)| *addr)
                .collect::<Vec<_>>();
            addrs
        }
    }
    pub async fn send_lan(
        &self,
        packet: Packet,
        addrs: Vec<SocketAddr>,
    ) -> std::result::Result<usize, SendError> {
        let len = packet.len();
        let size: usize = addrs.len() * len;
        let inner = &mut self.inner.write().await;
        let mut packet_tx = inner.packet_tx.clone();
        packet_tx.send((packet, addrs)).await?;
        Ok(size)
    }
    pub async fn server_info(&self) -> PeerManagerInfo {
        let inner = &self.inner.read().await;
        let online = inner.cache.len() as i32;
        let idle = inner.cache.values().filter(|i| i.state.is_idle()).count() as i32;
        PeerManagerInfo { online, idle }
    }
}
