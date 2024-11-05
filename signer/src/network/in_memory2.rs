//! New version of the in-memory network

use std::{collections::VecDeque, sync::{Arc, RwLock}, time::Duration};

use tokio::sync::broadcast::Sender;

use crate::error::Error;

use super::{MessageTransfer, Msg, MsgId};

/// In-memory representation of a WAN network between different signers.
pub struct WanNetwork {
    tx: Sender<Msg>
}

impl WanNetwork {
    /// Create a new in-memory WAN network
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(10_000);
        Self { tx }
    }

    /// Connect to the in-memory WAN network, returning a new signer-scoped
    /// network instance.
    pub async fn connect(&self) -> SignerNetwork {
        let network = SignerNetwork::new(self.tx.clone());
        network.start().await;
        network
    }
}

/// In-memory representation of the network for a single signer. This is used in
/// tests to simulate the disperate signer components which each take their own
/// `MessageTransfer` instance, but in reality are all connected to the same
/// in-memory network and should behave as such.
pub struct SignerNetwork(Arc<InnerSignerNetwork>);

impl Clone for SignerNetwork {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl SignerNetwork {
    /// Spawns a new instance of the in-memory signer network.
    pub fn spawn(&self) -> SignerNetworkInstance {
        SignerNetworkInstance {
            signer_network: self.clone(),
            instance_rx: self.0.signer_tx.subscribe(),
        }
    }

    /// Create a new in-memory signer network
    fn new(wan_tx: Sender<Msg>) -> Self {
        Self(Arc::new(InnerSignerNetwork::new(wan_tx)))
    }

    /// Start the in-memory signer network
    pub async fn start(&self) {
        // We listen to the WAN network and forward messages to the signer network.
        let mut rx = self.0.wan_tx.subscribe();
        // We clone the sender to the signer network to be able to send messages
        let tx = self.0.signer_tx.clone();
        // We clone the inner state to be able to check if a message has been sent
        let inner = Arc::clone(&self.0);

        // We spawn a task that listens to the WAN network and forwards messages
        // to the signer network, but only if this signer instance isn't the
        // sender.
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(5)).await;
                let msg = rx.recv().await.unwrap();
                if inner.sent.read().unwrap().contains(&msg.id()) {
                    continue;
                }
                tx.send(msg).unwrap();
            }
        });
    }
}

/// Inner state of the in-memory signer network
pub struct InnerSignerNetwork {
    wan_tx: Sender<Msg>,
    signer_tx: Sender<Msg>,
    sent: RwLock<VecDeque<MsgId>>
}

impl InnerSignerNetwork {
    /// Create a new in-memory signer network.
    pub fn new(wan_tx: Sender<Msg>) -> Self {
        // We create a new broadcast channel for this signer's network.
        let (signer_tx, _) = tokio::sync::broadcast::channel(1_000);

        Self { 
            wan_tx, 
            signer_tx,
            sent: RwLock::new(VecDeque::new()) 
        }
    }

    /// Sends a message to the WAN network.
    fn send(&self, msg: &Msg) -> Result<(), Error> {
        self.dedup_buffer(msg);        
        let _ = self.wan_tx.send(msg.clone());
        Ok(())
    }

    /// Buffer a message to prevent it from being received by the same signer.
    fn dedup_buffer(&self, msg: &Msg) {
        let mut sent_buffer = self.sent.write().unwrap();
        sent_buffer.push_back(msg.id());
        if sent_buffer.len() > 500 {
            sent_buffer.pop_front();
        }
    }
}

/// Represents a single instance of the in-memory signer network. This is used
/// in tests to simulate the disperate signer components which each take their
/// own `MessageTransfer` instance, but in reality are all connected to the same
/// in-memory network and should behave as such.
pub struct SignerNetworkInstance {
    signer_network: SignerNetwork,
    instance_rx: tokio::sync::broadcast::Receiver<Msg>,
}

impl Clone for SignerNetworkInstance {
    fn clone(&self) -> Self {
        Self {
            signer_network: self.signer_network.clone(),
            instance_rx: self.signer_network.0.signer_tx.subscribe(),
        }
    }
}

impl MessageTransfer for SignerNetworkInstance {
    async fn broadcast(&mut self, msg: Msg) -> Result<(), Error> {
        self.signer_network.0.send(&msg)
    }

    async fn receive(&mut self) -> Result<Msg, Error> {
        loop {
            if let Ok(msg) = self.instance_rx.recv().await {
                return Ok(msg);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU16, Ordering};

    use futures::future::join_all;
    use rand::rngs::OsRng;

    use super::*;

    #[tokio::test]
    async fn signer_2_can_receive_messages_from_signer_1() {
        let network = WanNetwork::new();

        let signer_1 = network.connect().await;
        let signer_2 = network.connect().await;

        let mut client_1 = signer_1.spawn();
        let mut client_2 = signer_2.spawn();

        let msg = Msg::random(&mut OsRng);

        tokio::spawn(async {
            tokio::time::timeout(
                Duration::from_secs(1), 
                async move {
                    client_2.receive().await.unwrap();
                }
            ).await.expect("client 2 did not receive message in time")
        });

        client_1.broadcast(msg).await.unwrap();
    }

    #[tokio::test]
    async fn signer_2_can_receive_messages_from_signer_1_concurrent_send() {
        let network = WanNetwork::new();

        let signer_1 = network.connect().await;
        let signer_2 = network.connect().await;

        let mut client_1a = signer_1.spawn();
        let mut client_1b = signer_1.spawn();
        let mut client_2 = signer_2.spawn();

        let recv_count = Arc::new(AtomicU16::new(0));
        let recv_count_clone = Arc::clone(&recv_count);
        let client2_handle = tokio::spawn(async {
            tokio::time::timeout(
                Duration::from_secs(3), 
                async move {
                    while recv_count_clone.load(Ordering::SeqCst) < 200 {
                        client_2.receive().await.unwrap();
                        recv_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            ).await.expect("client 2 did not receive all messages in time")
        });

        let send1_handle = tokio::spawn(async move {
            for _ in 0..100 {
                client_1a.broadcast(Msg::random(&mut OsRng)).await.unwrap();
            }
        });

        let send2_handle = tokio::spawn(async move {
            for _ in 0..100 {
                client_1b.broadcast(Msg::random(&mut OsRng)).await.unwrap();
            }
        });

        join_all([send1_handle, send2_handle, client2_handle]).await;
        assert_eq!(recv_count.load(Ordering::SeqCst), 200);

    }

    #[tokio::test]
    async fn network_instance_does_not_receive_messages_from_same_signer_network() {
        let network = WanNetwork::new();

        let client = network.connect().await;

        let mut client_a = client.spawn();
        let mut client_b = client.spawn();

        let msg = Msg::random(&mut OsRng);

        tokio::spawn(async {
            tokio::time::timeout(
                Duration::from_secs(1), 
                async move {
                    client_b.receive().await.unwrap();
                }
            ).await.expect_err("client received its own message")
        });

        client_a.broadcast(msg).await.unwrap();
    }

    #[tokio::test]
    async fn two_clients_can_exchange_messages_simple() {
        let network = WanNetwork::new();

        let client_1 = network.connect().await;
        let client_2 = network.connect().await;

        let mut client_1 = client_1.spawn();
        let mut client_2 = client_2.spawn();
        let mut client_2b = client_2.clone();

        let msg = Msg::random(&mut OsRng);

        tokio::spawn(async {
            tokio::time::timeout(
                Duration::from_secs(1), 
                async move {
                    client_2.receive().await.unwrap();
                }
            ).await.expect("client 2 did not receive message in time")
        });

        client_1.broadcast(msg.clone()).await.unwrap();

        tokio::spawn(async {
            tokio::time::timeout(
                Duration::from_secs(1), 
                async move {
                    client_1.receive().await.unwrap();
                }
            ).await.expect("client 1 did not receive message in time")
        });

        client_2b.broadcast(msg).await.unwrap();
    }

    #[tokio::test]
    async fn two_clients_can_exchange_messages_advanced() {
        let network = WanNetwork::new();

        let client_1 = network.connect().await;
        let client_2 = network.connect().await;

        let instance_1 = client_1.spawn();
        let instance_2 = client_2.spawn();

        crate::testing::network::assert_clients_can_exchange_messages(instance_1, instance_2)
            .await;
    }
}