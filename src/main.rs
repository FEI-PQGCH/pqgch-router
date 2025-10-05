mod message;
mod routing;
mod session;

use message::*;
use routing::*;
use session::*;
use socket2::SockRef;
use std::{io, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream, tcp::OwnedWriteHalf},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender, unbounded_channel},
        oneshot,
    },
};
use tracing::{info, warn};

type Tx = UnboundedSender<String>;
type Rx = UnboundedReceiver<String>;

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::init();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    tokio::spawn(SessionState::session_actor(cmd_rx));

    let addr = "0.0.0.0:9000";
    let listener = TcpListener::bind(addr).await?;
    info!("listening on {addr}");

    loop {
        let (stream, addr) = listener.accept().await?;

        let sock_ref = SockRef::from(&stream);
        let mut keep_alive = socket2::TcpKeepalive::new();
        keep_alive = keep_alive.with_time(Duration::from_secs(20));
        keep_alive = keep_alive.with_interval(Duration::from_secs(20));
        sock_ref.set_tcp_keepalive(&keep_alive)?;

        let cmd_tx_clone = cmd_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, cmd_tx_clone).await {
                warn!("connection {addr} error: {e}");
            }
        });
    }
}

async fn send_auth_error(net_writer: &mut OwnedWriteHalf) -> io::Result<()> {
    net_writer
        .write_all(
            ChatMessage::err("invalid auth".to_owned())
                .to_json()
                .as_bytes(),
        )
        .await?;
    Ok(())
}

async fn handle_conn(stream: TcpStream, cmd_tx: UnboundedSender<SessionCommand>) -> io::Result<()> {
    let peer = stream.peer_addr()?;
    info!("new connection from {peer}");

    let (net_reader, mut net_writer) = stream.into_split();
    let mut lines = BufReader::new(net_reader).lines();

    let auth_line = match lines.next_line().await? {
        Some(l) => l,
        None => {
            warn!("no auth message, dropping connection");
            send_auth_error(&mut net_writer).await?;
            return Ok(());
        }
    };

    let auth_msg: ChatMessage = match serde_json::from_str(&auth_line) {
        Ok(m) => m,
        Err(e) => {
            warn!("invalid auth message, dropping connection {e}");
            send_auth_error(&mut net_writer).await?;
            return Ok(());
        }
    };

    let client_key: ClientKey = {
        match auth_msg.msg_type {
            MessageType::MemberAuth => ClientKey {
                cluster_id: auth_msg.cluster_id,
                user_id: auth_msg.send_id,
                is_leader: false,
            },
            MessageType::LeaderAuth => ClientKey {
                cluster_id: auth_msg.cluster_id,
                user_id: auth_msg.send_id,
                is_leader: true,
            },
            _ => {
                warn!(
                    "expected auth message, received type ID: {:?}",
                    auth_msg.msg_type
                );
                send_auth_error(&mut net_writer).await?;
                return Ok(());
            }
        }
    };

    let (tx, rx) = unbounded_channel::<String>();
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();

    _ = cmd_tx.send(SessionCommand::Join {
        key: client_key,
        tx,
        stop_tx,
    });

    info!("{client_key} logged in successfully");

    let sender_task = |mut rx: Rx, mut writer: OwnedWriteHalf| async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = writer.write_all(message.as_bytes()).await {
                info!("failed to write message: {e}");
                break;
            }
        }
    };
    tokio::spawn(sender_task(rx, net_writer));

    loop {
        tokio::select! {
            Ok(line) = lines.next_line() => {
                match line {
                    Some(line) => {
                        match serde_json::from_str::<ChatMessage>(&line) {
                            Ok(app_msg) => {
                                info!("received {app_msg}");

                                if let MessageType::Ping = app_msg.msg_type {
                                    _ = cmd_tx.send(SessionCommand::Ping {key: client_key});
                                    continue;
                                }

                                _ = cmd_tx.send(SessionCommand::SendMessage {msg: app_msg, sender_key: client_key});
                            }
                            Err(e) => warn!("invalid message from client: {e}"),
                        }
                    }
                    None => break,
                }
            }
            _ = &mut stop_rx => return Ok(()),
        }
    }
    _ = cmd_tx.send(SessionCommand::Leave { key: client_key });

    Ok(())
}
