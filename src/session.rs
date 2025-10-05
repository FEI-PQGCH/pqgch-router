use crate::message::*;
use crate::routing::*;
use crate::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, oneshot::Sender},
    time::interval,
};
use tracing::{info, warn};

pub enum SessionCommand {
    Join {
        key: ClientKey,
        tx: Tx,
        stop_tx: Sender<()>,
    },
    Leave {
        key: ClientKey,
    },
    SendMessage {
        msg: ChatMessage,
        sender_key: ClientKey,
    },
    Ping {
        key: ClientKey,
    },
}

pub struct SessionState {
    pub active_conn: HashMap<ClientKey, Tx>,
    joined_once: HashSet<ClientKey>,
    last_seen: HashMap<ClientKey, Instant>,
    stop_txs: HashMap<ClientKey, Sender<()>>,
    messages: Vec<ChatMessage>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            active_conn: HashMap::new(),
            joined_once: HashSet::new(),
            last_seen: HashMap::new(),
            stop_txs: HashMap::new(),
            messages: Vec::new(),
        }
    }

    fn leave_session(&mut self, client_key: &ClientKey) {
        if self.active_conn.remove(client_key).is_some() {
            info!("{client_key} disconnected");
            self.last_seen.remove(client_key);

            if let Some(tx) = self.stop_txs.remove(client_key) {
                _ = tx.send(());
            }

            if self.active_conn.is_empty() {
                info!("no members left in session, clearing");
                self.reset_session();
            }
        }
    }

    fn reset_session(&mut self) {
        self.messages.clear();
        self.active_conn.clear();
        self.joined_once.clear();
        self.last_seen.clear();

        let keys: Vec<ClientKey> = self.stop_txs.iter().map(|kv| *kv.0).collect();
        for key in keys {
            if let Some(tx) = self.stop_txs.remove(&key) {
                _ = tx.send(());
            }
        }
    }

    async fn timeout_checker(state: Arc<Mutex<SessionState>>) {
        let mut interval = interval(Duration::from_secs(15));
        loop {
            interval.tick().await;

            let mut s = state.lock().await;
            let timed_out: Vec<ClientKey> = s
                .last_seen
                .iter()
                .filter_map(|(&key, last)| {
                    if last.elapsed() > Duration::from_secs(30) {
                        Some(key)
                    } else {
                        None
                    }
                })
                .collect();

            for client in timed_out {
                warn!("{client} timed out");
                s.leave_session(&client);
            }
        }
    }

    pub async fn session_actor(mut cmd_rx: UnboundedReceiver<SessionCommand>) {
        let state = Arc::new(Mutex::new(SessionState::new()));

        tokio::spawn(Self::timeout_checker(Arc::clone(&state)));

        while let Some(cmd) = cmd_rx.recv().await {
            let mut s = state.lock().await;

            match cmd {
                SessionCommand::Join { key, tx, stop_tx } => {
                    if s.joined_once.contains(&key) {
                        warn!("duplicate join of {key}, restarting session");

                        for (_, old_tx) in s.active_conn.iter() {
                            let _ = old_tx.send(
                                ChatMessage::err(
                                    "session terminated due to duplicate join".to_owned(),
                                )
                                .to_json(),
                            );
                        }

                        s.reset_session();
                    }

                    for message in &s.messages {
                        if applies_route(message, key) {
                            _ = tx.send(message.to_json());
                        }
                    }

                    s.active_conn.insert(key, tx);
                    s.joined_once.insert(key);
                    s.last_seen.insert(key, Instant::now());
                    s.stop_txs.insert(key, stop_tx);
                }
                SessionCommand::Leave { key } => {
                    s.leave_session(&key);
                }
                SessionCommand::SendMessage { msg, sender_key } => {
                    route_message(&msg, &s, Some(sender_key));
                    s.messages.push(msg);
                }
                SessionCommand::Ping { key } => {
                    s.last_seen.insert(key, Instant::now());

                    let pong = ChatMessage {
                        send_id: 0,
                        recv_id: key.user_id,
                        cluster_id: key.cluster_id,
                        msg_type: MessageType::Pong,
                        sender: "pqgch-router".to_owned(),
                        content: "".to_owned(),
                    };
                    route_message(&pong, &s, None);
                }
            }
        }
    }
}
