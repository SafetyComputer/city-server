use std::time::{Duration, Instant};

use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::interval};

use crate::{
    data::models::User,
    game::{Move, Winner},
    matchmaking::service::{ConnId, MatchInfo, MatchServerHandle, RoomId, Uuid},
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
type CommandId = u64;

#[derive(Deserialize)]
struct UserMessage {
    command: Command,
    command_id: CommandId,
    room: Option<RoomId>,
    data: Option<String>,
}

#[derive(Deserialize)]
enum Command {
    SendMessage,
    StartMatching,
    StopMatching,
    Move,
}

#[derive(Serialize)]
pub struct ServerMessage {
    pub message_type: MessageType,
    pub room: Option<RoomId>,
    pub data: String,
}

#[derive(Serialize)]
struct CommandReturn {
    command_id: CommandId,
    data: String,
}

#[derive(Serialize)]
pub enum MessageType {
    Success,
    Error,
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

    pub fn success_message(msg: impl Into<String>, command_id: CommandId) -> Self {
        let ret = CommandReturn {
            command_id,
            data: msg.into(),
        };
        Self {
            message_type: MessageType::Success,
            room: None,
            data: serde_json::to_string(&ret).unwrap(),
        }
    }

    pub fn error_message(msg: impl Into<String>, command_id: CommandId) -> Self {
        let ret = CommandReturn {
            command_id,
            data: msg.into(),
        };
        Self {
            message_type: MessageType::Error,
            room: None,
            data: serde_json::to_string(&ret).unwrap(),
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
            data: format!("{}", uuid),
        }
    }

    pub fn leave_message(uuid: Uuid, room: RoomId) -> Self {
        Self {
            message_type: MessageType::Leave,
            room: Some(room),
            data: format!("{}", uuid),
        }
    }

    pub fn end_message(room: RoomId, winner: Winner) -> Self {
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

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

pub async fn match_ws(
    match_server: MatchServerHandle,
    user: User,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
) {
    let uuid = user.id.unwrap();
    let name: String = user.username;
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
                    AggregatedMessage::Text(text) => {
                        process_text_msg(&match_server, &mut session, &text, conn_id, uuid, &name)
                            .await;
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

async fn process_text_msg(
    match_server: &MatchServerHandle,
    session: &mut actix_ws::Session,
    text: &str,
    conn: ConnId,
    uuid: Uuid,
    name: &String,
) {
    let msg = text.trim();
    if let Ok(msg) = serde_json::from_str::<UserMessage>(msg) {
        match msg.command {
            Command::SendMessage => {
                let user_msg = format!("{name}: {}", msg.data.unwrap());
                match_server
                    .send_message(msg.room.unwrap(), conn, uuid, user_msg)
                    .await;
                let msg =
                    ServerMessage::success_message("successfully sent message", msg.command_id);
                let _ = session.text(msg.to_string()).await;
            }

            Command::StartMatching => {
                match_server.start_matching(uuid).await;
                let msg =
                    ServerMessage::success_message("successfully started matching", msg.command_id);
                let _ = session.text(msg.to_string()).await;
            }

            Command::StopMatching => {
                match_server.stop_matching(uuid).await;
                let msg =
                    ServerMessage::success_message("successfully stopped matching", msg.command_id);
                let _ = session.text(msg.to_string()).await;
            }

            Command::Move => {
                let mv = serde_json::from_str(msg.data.unwrap().as_str());
                match mv {
                    Ok(mv) => {
                        let result = match_server
                            .make_move(mv, msg.room.unwrap(), conn, uuid)
                            .await;
                        let msg = if result.success {
                            ServerMessage::success_message("successfully made move", msg.command_id)
                        } else {
                            ServerMessage::error_message("illegal move", msg.command_id)
                        };
                        let _ = session.text(msg.to_string());
                        if let Some(winner) = result.winner {
                            let _ = session.text(
                                ServerMessage::end_message(msg.room.unwrap(), winner).to_string(),
                            );
                        }
                    }

                    Err(_) => {
                        let _ = session
                            .text(
                                ServerMessage::error_message("invalid move notion", msg.command_id)
                                    .to_string(),
                            )
                            .await;
                    }
                }
            }
        }
    } else {
        let msg = ServerMessage::error_message("invalid message".to_string(), 0);
        let _ = session.text(msg.to_string()).await;
    }
}
