use std::time::{Duration, Instant};

use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::interval};

use crate::{game::Move, match_server::MatchServerHandle};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize, Serialize)]
pub struct Message {
    command: String,
    room: i32,
    data: String,
}

pub async fn match_ws(
    match_server: MatchServerHandle,
    //user: crate::models::User,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
) {
    //let uuid = user.id.unwrap();
    let uuid = 0;
    //let name: String = user.username;
    let name = "0".to_string();
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
    conn: i32,
    name: &String,
) {
    let msg = text.trim();
    if let Ok(msg) = serde_json::from_str::<Message>(msg) {
        match msg.command.as_str() {
            "Message" => {
                let user_msg = format!("{name}: {}", msg.data);

                match_server.send_message(msg.room, conn, user_msg).await;
                let _ = session.text("success".to_string()).await;
            }

            "StartMatching" => {
                match_server.start_matching(conn).await;
            }

            "StopMatching" => {
                match_server.stop_matching(conn).await;
            }

            "Move" => {
                let mv = Move::from_notation(msg.data.as_str());
                match mv {
                    Ok(mv) => {
                        let result = match_server.make_move(mv, msg.room, conn).await;
                        if result {
                            let _ = session.text("success".to_string()).await;
                        } else {
                            let _ = session.text("invalid move".to_string()).await;
                        }
                    }
                    Err(_) => {
                        let _ = session.text("invalid move notion".to_string()).await;
                    }
                }
            }

            "CreateMatchRoom" => {
                let room_id = match_server.create_match_room(conn).await;
                let _ = session.text(format!("created room with id {room_id}")).await;
            }
            
            "PlayerJoin" => {
                let result = match_server.player_join(msg.room, conn).await;
            }

            "ViewerJoin" => {
                let result = match_server.viewer_join(msg.room, conn).await;
            }
            _ => {
                let _ = session.text("no such command".to_string()).await;
            }
        }
    } else {
        let _ = session.text("invalid message".to_string()).await;
    }
}
