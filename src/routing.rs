use crate::message::*;
use crate::session::*;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy)]
pub struct ClientKey {
    pub user_id: u64,
    pub cluster_id: u64,
    pub is_leader: bool,
}

impl fmt::Display for ClientKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "client [ID: {} / Cluster ID: {} / Leader?: {}]",
            self.user_id, self.cluster_id, self.is_leader
        )
    }
}

pub fn route_message(app_msg: &ChatMessage, state: &SessionState, sender_key: Option<ClientKey>) {
    for (&tx_client, tx) in state.active_conn.iter() {
        // Check so we do not send back to sender.
        if let Some(sk) = sender_key
            && tx_client == sk
            && app_msg.msg_type != MessageType::Pong
        {
            continue;
        }

        if applies_route(app_msg, tx_client) {
            _ = tx.send(app_msg.to_json());
        }
    }
}

#[derive(Debug, Clone)]
pub enum RouteType {
    Broadcast,
    Direct,
    ClusterBroadcast,
    LeaderDirect,
    LeaderBroadcast,
    None,
}

pub fn applies_route(app_msg: &ChatMessage, tx_client: ClientKey) -> bool {
    match app_msg.route_type() {
        RouteType::Broadcast => true,
        RouteType::Direct => {
            let is_same_cluster = app_msg.cluster_id == tx_client.cluster_id;
            let client_match = app_msg.recv_id == tx_client.user_id;

            is_same_cluster && client_match
        }
        RouteType::LeaderDirect => {
            let leader_match = tx_client.cluster_id == app_msg.recv_id;

            tx_client.is_leader && leader_match
        }
        RouteType::ClusterBroadcast => tx_client.cluster_id == app_msg.cluster_id,
        RouteType::LeaderBroadcast => tx_client.is_leader,
        _ => false,
    }
}
