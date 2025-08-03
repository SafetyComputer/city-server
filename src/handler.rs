use std::time::{Duration, Instant};

use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use tokio::{sync::mpsc, time::interval};

use crate::match_server::MatchServerHandle;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn match_ws(
    match_server: MatchServerHandle,
    user: crate::models::User,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
) {
    let uuid = user.id.unwrap();
    let mut name: String = user.username;
    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    match_server.connect(uuid, conn_tx).await;

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
                        process_text_msg(&match_server, &mut session, &text, uuid, &mut name)
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

    match_server.disconnect(uuid);

    let _ = session.close(close_reason).await;
}

async fn process_text_msg(
    match_server: &MatchServerHandle,
    session: &mut actix_ws::Session,
    text: &str,
    conn: i32,
    name: &mut String,
) {
    let msg = text.trim();
    if msg.starts_with('/') {
    } else {
        let msg = format!("{name}: {msg}");

        match_server.send_message(conn, msg).await;
        let _ = session.text("success".to_string()).await;
    }
}
