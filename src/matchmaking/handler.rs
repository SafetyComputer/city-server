use std::{
    fmt::Display,
    time::{Duration, Instant},
};

use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::interval};

use crate::{
    game::{Move, Winner},
    matchmaking::service::{MatchInfo, MatchServerHandle, RoomId, Uuid},
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
enum Command {
    SendMessage,
    Move,
}

#[derive(Serialize)]
pub struct ServerMessage {
    pub message_type: MessageType,
    pub room: Option<RoomId>,
    pub data: String,
}

#[derive(Serialize)]
pub enum MessageType {
    Chat,
    Move,
    Match,
    End,
    Join,
    Leave,
}

impl ServerMessage {
    pub fn match_message(info: &MatchInfo, room: RoomId) -> Self {
        ServerMessage {
            message_type: MessageType::Match,
            room: Some(room),
            data: serde_json::to_string(info).unwrap(),
        }
    }

    pub fn chat_message(msg: impl Into<String>, room: RoomId) -> Self {
        Self {
            message_type: MessageType::Chat,
            room: Some(room),
            data: msg.into(),
        }
    }

    pub fn join_message(uuid: Uuid, room: RoomId) -> Self {
        Self {
            message_type: MessageType::Join,
            room: Some(room),
            data: format!("{uuid}"),
        }
    }

    pub fn leave_message(uuid: Uuid, room: RoomId) -> Self {
        Self {
            message_type: MessageType::Leave,
            room: Some(room),
            data: format!("{uuid}"),
        }
    }

    pub fn end_message(room: RoomId, winner: Option<Winner>) -> Self {
        Self {
            message_type: MessageType::End,
            room: Some(room),
            data: serde_json::to_string(&winner).unwrap(),
        }
    }

    pub fn move_message(mv: &Move, room: RoomId) -> Self {
        Self {
            message_type: MessageType::Move,
            room: Some(room),
            data: serde_json::to_string(mv).unwrap(),
        }
    }
}

impl Display for ServerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

pub async fn match_ws(
    match_server: MatchServerHandle,
    uuid: Uuid,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
) {
    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    let conn_id = match_server.connect(uuid, conn_tx).await;

    let mut msg_stream = msg_stream
        .max_frame_size(128 * 1024)
        .aggregate_continuations()
        .max_continuation_size(2 * 1024 * 1024);

    let close_reason = loop {
        tokio::select! {
            Some(Ok(msg)) = msg_stream.next() => {
                match msg {
                    AggregatedMessage::Ping(bytes) => {
                        last_heartbeat = Instant::now();
                        session.pong(&bytes).await.unwrap();
                    }
                    AggregatedMessage::Pong(_) => {
                        last_heartbeat = Instant::now();
                    }
                    AggregatedMessage::Text(_text) => {
                    }
                    AggregatedMessage::Binary(_bin) => {
                    }
                    AggregatedMessage::Close(reason) => break reason,
                }
            }
            Some(chat_msg) = conn_rx.recv() => {
                 session.text(chat_msg).await.unwrap();
            }
            _ = interval.tick() => {
                if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    break None;
                }
                let _ = session.ping(b"").await;
            }
            else => {
                break None;
            }
        }
    };

    match_server.disconnect(conn_id, uuid);
    let _ = session.close(close_reason).await;
}