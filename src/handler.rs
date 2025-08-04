use std::time::{Duration, Instant};

use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::interval};

use crate::{
    game::Move,
    match_server::{Color, ConnId, MatchServerHandle, RoomId},
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct UserMessage {
    command: Command,
    room: Option<RoomId>,
    data: Option<String>,
}

#[derive(Deserialize)]
enum Command {
    SendMessage,
    StartMatching,
    StopMatching,
    Move,
    CreateMatchRoom,
    JoinPlayers,
    JoinViewers,
}

#[derive(Serialize)]
pub struct ServerMessage {
    pub message_type: MessageType,
    pub data: String,
}

#[derive(Serialize)]
pub enum MessageType {
    Success,
    Error,
    System,
    Chat,
    Move,
    Match,
}

impl ServerMessage {
    pub fn match_message(opponent_id: i32, match_id: RoomId, self_color: Color) -> Self {
        match self_color {
            Color::Blue => ServerMessage {
                message_type: MessageType::Match,
                data: format!(
                    "start match with {}, match id {}, self color is blue",
                    opponent_id, match_id
                ),
            },
            Color::Green => ServerMessage {
                message_type: MessageType::Match,
                data: format!(
                    "start match with {}, match id {}, self color is green",
                    opponent_id, match_id
                ),
            },
        }
    }

    pub fn success_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: MessageType::Success,
            data: msg.into(),
        }
    }

    pub fn error_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: MessageType::Error,
            data: msg.into(),
        }
    }

    pub fn chat_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: MessageType::Chat,
            data: msg.into(),
        }
    }

    pub fn system_message(msg: impl Into<String>) -> Self {
        Self {
            message_type: MessageType::System,
            data: msg.into(),
        }
    }

    pub fn move_message(mv: &Move) -> Self {
        Self {
            message_type: MessageType::Move,
            data: serde_json::to_string(mv).unwrap(),
        }
    }

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

pub async fn match_ws(
    match_server: MatchServerHandle,
    user: crate::models::User,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
) {
    let uuid = user.id.unwrap();
    //let uuid = 0;
    let name: String = user.username;
    //let name = "0".to_string();
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
                        process_text_msg(&match_server, &mut session, &text, conn_id, &name)
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

    match_server.disconnect(conn_id, name);

    let _ = session.close(close_reason).await;
}

async fn process_text_msg(
    match_server: &MatchServerHandle,
    session: &mut actix_ws::Session,
    text: &str,
    conn: ConnId,
    name: &String,
) {
    let msg = text.trim();
    if let Ok(msg) = serde_json::from_str::<UserMessage>(msg) {
        match msg.command {
            Command::SendMessage => {
                let user_msg = format!("{name}: {}", msg.data.unwrap());

                match_server
                    .send_message(msg.room.unwrap(), conn, user_msg)
                    .await;
                let msg = ServerMessage::success_message("successfully sent message");
                let _ = session.text(msg.to_string()).await;
            }

            Command::StartMatching => {
                match_server.start_matching(conn).await;
                let msg = ServerMessage::success_message("successfully started matching");
                let _ = session.text(msg.to_string()).await;
            }

            Command::StopMatching => {
                match_server.stop_matching(conn).await;
                let msg = ServerMessage::success_message("successfully stopped matching");
                let _ = session.text(msg.to_string()).await;
            }

            Command::Move => {
                let mv = serde_json::from_str(msg.data.unwrap().as_str());
                let msg = match mv {
                    Ok(mv) => {
                        let result = match_server.make_move(mv, msg.room.unwrap(), conn).await;
                        if result {
                            ServerMessage::success_message("successfully made move")
                        } else {
                            ServerMessage::error_message("illegal move")
                        }
                    }
                    Err(_) => ServerMessage::error_message("invalid move notion"),
                };
                let _ = session.text(msg.to_string()).await;
            }

            Command::CreateMatchRoom => {
                let room_id = match_server.create_match_room(conn).await;
                let msg = ServerMessage::success_message(format!("created room with id {room_id}"));
                let _ = session.text(msg.to_string()).await;
            }

            Command::JoinPlayers => {
                let room_id = msg.room.unwrap();
                let result = match_server.join_players(room_id, conn).await;
                let msg = match result {
                    None => ServerMessage::error_message(format!(
                        "unable to join room {} as player",
                        room_id
                    )),
                    Some(result) => {
                        ServerMessage::match_message(result.opponent_id, room_id, result.self_color)
                    }
                };
                let _ = session.text(msg.to_string()).await;
            }

            Command::JoinViewers => {
                let result = match_server.join_viewers(msg.room.unwrap(), conn).await;
                if result {
                    let msg = ServerMessage::success_message(format!(
                        "successfully joined room {} as viewer",
                        msg.room.unwrap()
                    ));
                    let _ = session.text(msg.to_string()).await;
                }
            }
        }
    } else {
        let msg = ServerMessage {
            message_type: MessageType::Error,
            data: "invalid message".to_string(),
        };
        let _ = session.text(msg.to_string()).await;
    }
}
