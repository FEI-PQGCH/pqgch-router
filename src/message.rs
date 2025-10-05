use crate::routing::RouteType;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    #[serde(rename = "sendId")]
    pub send_id: u64,
    #[serde(rename = "recvId")]
    pub recv_id: u64,
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    #[serde(rename = "clusterId")]
    pub cluster_id: u64,
    pub sender: String,
    pub content: String,
}

impl ChatMessage {
    pub fn route_type(&self) -> RouteType {
        match self.msg_type {
            MessageType::Text => RouteType::Broadcast,
            MessageType::AkeOne | MessageType::AkeTwo | MessageType::Pong => RouteType::Direct,
            MessageType::XiRiCommitment | MessageType::Key | MessageType::QKDIDMember => {
                RouteType::ClusterBroadcast
            }
            MessageType::LeadAkeOne | MessageType::LeadAkeTwo | MessageType::QKDIDLeader => {
                RouteType::LeaderDirect
            }
            MessageType::LeaderXiRiCommitment => RouteType::LeaderBroadcast,
            _ => RouteType::None,
        }
    }

    pub fn err(message: String) -> ChatMessage {
        ChatMessage {
            send_id: 0,
            recv_id: 0,
            msg_type: MessageType::Err,
            cluster_id: 0,
            sender: String::new(),
            content: message,
        }
    }
}

impl fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ SenderID: {}, ReceiverID: {}, Type: {:?}, ClusterID: {}, SenderName: {} }}",
            self.send_id, self.recv_id, self.msg_type, self.cluster_id, self.sender
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    MemberAuth,
    LeaderAuth,
    Text,
    AkeOne,
    AkeTwo,
    XiRiCommitment,
    Key,
    LeadAkeOne,
    LeadAkeTwo,
    LeaderXiRiCommitment,
    QKDIDLeader,
    QKDIDMember,
    Ping,
    Pong,
    Err,
}

impl TryFrom<u8> for MessageType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MessageType::MemberAuth),
            1 => Ok(MessageType::LeaderAuth),
            2 => Ok(MessageType::Text),
            3 => Ok(MessageType::AkeOne),
            4 => Ok(MessageType::AkeTwo),
            5 => Ok(MessageType::XiRiCommitment),
            6 => Ok(MessageType::Key),
            7 => Ok(MessageType::LeadAkeOne),
            8 => Ok(MessageType::LeadAkeTwo),
            9 => Ok(MessageType::LeaderXiRiCommitment),
            10 => Ok(MessageType::QKDIDLeader),
            11 => Ok(MessageType::QKDIDMember),
            12 => Ok(MessageType::Ping),
            13 => Ok(MessageType::Pong),
            14 => Ok(MessageType::Err),
            _ => Err(()),
        }
    }
}

impl Serialize for MessageType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for MessageType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        MessageType::try_from(v).map_err(|_| serde::de::Error::custom("unknown message type"))
    }
}

pub trait ToJson {
    fn to_json(&self) -> String;
}

impl<T: Serialize> ToJson for T {
    fn to_json(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(mut s) => {
                s.push('\n');
                s
            }
            Err(_) => "".to_owned(),
        }
    }
}
