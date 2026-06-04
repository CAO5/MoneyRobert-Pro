use axum::extract::ws::{Message, WebSocket, Utf8Bytes};
use futures::{SinkExt, StreamExt};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

type UserId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    #[serde(rename = "type")]
    pub subscription_type: String,
    pub symbols: Vec<String>,
    pub channels: Vec<String>,
}

pub struct WsConnection {
    pub user_id: Option<i64>,
    pub subscriptions: Vec<SubscriptionRequest>,
}

pub struct WebSocketManager {
    connections: Arc<RwLock<HashMap<String, broadcast::Sender<Message>>>>,
    user_connections: Arc<RwLock<HashMap<i64, Vec<String>>>>,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            user_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn handle_connection(
        &self,
        socket: WebSocket,
        user_id: Option<i64>,
    ) {
        let (mut sender, mut receiver) = socket.split();

        let connection_id = Uuid::new_v4().to_string();
        let (tx, mut rx) = broadcast::channel::<Message>(100);

        self.connections
            .write()
            .insert(connection_id.clone(), tx);

        if let Some(uid) = user_id {
            self.user_connections
                .write()
                .entry(uid)
                .or_default()
                .push(connection_id.clone());
        }

        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(request) = serde_json::from_str::<SubscriptionRequest>(&*text) {
                        tracing::info!(
                            user_id = user_id,
                            subscription_type = request.subscription_type,
                            "New WebSocket subscription"
                        );
                    }
                } else if let Message::Close(_) = msg {
                    break;
                }
            }
        });

        let connections = self.connections.clone();
        let conn_id = connection_id.clone();
        let mut send_task = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sender.send(msg).await.is_err() {
                    break;
                }
            }
            
            connections.write().remove(&conn_id);
        });

        tokio::select! {
            _ = (&mut recv_task) => send_task.abort(),
            _ = (&mut send_task) => recv_task.abort(),
        }

        if let Some(uid) = user_id {
            if let Some(mut conns) = self.user_connections.write().get_mut(&uid) {
                conns.retain(|id| id != &connection_id);
            }
        }
    }

    pub fn broadcast_to_user(&self, user_id: i64, message: &str) {
        let _msg: serde_json::Value = serde_json::from_str(message).unwrap_or_default();
        let ws_msg = Message::Text(Utf8Bytes::from(message.to_string()));
        if let Some(conn_ids) = self.user_connections.read().get(&user_id) {
            for conn_id in conn_ids {
                if let Some(tx) = self.connections.read().get(conn_id) {
                    let _ = tx.send(ws_msg.clone());
                }
            }
        }
    }

    pub fn broadcast_to_all(&self, message: Message) {
        let connections = self.connections.read();
        for (_, tx) in connections.iter() {
            let _ = tx.send(message.clone());
        }
    }

    pub fn connection_count(&self) -> usize {
        self.connections.read().len()
    }
}

impl Default for WebSocketManager {
    fn default() -> Self {
        Self::new()
    }
}
