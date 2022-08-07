pub use super::{InPacket, OutPacket, Packet, PeerManager};
pub use async_trait::async_trait;
use downcast_rs::{impl_downcast, Downcast};
pub use std::net::SocketAddr;

pub struct Context<'a> {
    pub peer_manager: &'a PeerManager,
}

impl<'a> Context<'a> {
    pub fn new(peer_manager: &'a PeerManager) -> Self {
        Self { peer_manager }
    }
}

pub trait PluginType<T = BoxPlugin> {
    fn name(&self) -> String;
    fn new(&self, context: Context) -> BoxPlugin;
}

pub type BoxPluginType<T = BoxPlugin> = Box<dyn PluginType<T> + Send + Sync + 'static>;

#[async_trait]
pub trait Plugin: Downcast {
    async fn in_packet(&mut self, packet: &InPacket) -> Result<(), ()>;
    async fn out_packet(&mut self, packet: &Packet, addrs: &[SocketAddr]) -> Result<(), ()>;
}
impl_downcast!(Plugin);

pub type BoxPlugin = Box<dyn Plugin + Send + 'static>;
