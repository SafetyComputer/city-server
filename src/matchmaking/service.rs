use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    time::Duration,
};

use actix_web::web;

use rand::Rng;
use tokio::sync::{mpsc, oneshot};

use crate::{
    data::Dbpool,
    game::{Move, Winner},
    matchmaking::matchroom::{Color, MatchInfo, MatchRoom, MatchRoomState},
};

use super::handler::ServerMessage;

pub type ConnId = u32;

pub type RoomId = u32;

pub type UserId = i32;

#[derive(Clone, Copy)]
pub enum BackgroundTask {
    MatchPlayers,
    CheckConnections,
    CheckMatches,
}

struct Sessions {
    inner: HashMap<ConnId, mpsc::UnboundedSender<String>>,
}

impl Sessions {
    fn send(&self, message: String) {
        for tx in self.inner.values() {
            let _ = tx.send(message.clone());
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn contains(&self, conn: ConnId) -> bool {
        self.inner.contains_key(&conn)
    }

    fn insert(
        &mut self,
        k: ConnId,
        v: mpsc::UnboundedSender<String>,
    ) -> Option<mpsc::UnboundedSender<String>> {
        self.inner.insert(k, v)
    }

    fn remove(&mut self, k: ConnId) -> Option<mpsc::UnboundedSender<String>> {
        self.inner.remove(&k)
    }

    fn remove_closed_sessions(&mut self) {
        let closed_sessions: Vec<ConnId> = self
            .inner
            .iter()
            .filter_map(|(conn_id, tx)| if tx.is_closed() { Some(*conn_id) } else { None })
            .collect();
        for conn_id in closed_sessions {
            self.inner.remove(&conn_id);
        }
    }
}

enum Command {
    Connect {
        uuid: UserId,
        conn_tx: mpsc::UnboundedSender<String>,
        res_tx: oneshot::Sender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
        uuid: UserId,
    },

    Message {
        msg: String,
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<()>,
    },

    StartMatching {
        uuid: UserId,
        res_tx: oneshot::Sender<()>,
    },

    StopMatching {
        uuid: UserId,
        res_tx: oneshot::Sender<()>,
    },

    Move {
        mv: Move,
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<Option<Duration>>,
    },

    Resign {
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<bool>,
    },

    PlayerJoin {
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    ViewerJoin {
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    CreateMatchRoom {
        uuid: UserId,
        res_tx: oneshot::Sender<RoomId>,
    },

    ListMatchRoom {
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },

    LeaveMatchRoom {
        room: RoomId,
        uuid: UserId,
        res_tx: oneshot::Sender<()>,
    },

    GetMatchRoomById {
        room: RoomId,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    Reconnect {
        uuid: UserId,
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },
}

pub struct MatchServer {
    sessions: HashMap<UserId, Sessions>,
    matches: HashMap<RoomId, MatchRoom>,
    waitings: HashSet<UserId>,
    db: web::Data<Dbpool>,
    cmd_rx: mpsc::UnboundedReceiver<Command>,
    task_rx: mpsc::UnboundedReceiver<BackgroundTask>,
}

impl MatchServer {
    pub fn new(db: web::Data<Dbpool>) -> (Self, MatchServerHandle) {
        let matches = HashMap::with_capacity(4);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                matches,
                waitings: HashSet::new(),
                db,
                cmd_rx,
                task_rx,
            },
            MatchServerHandle { cmd_tx, task_tx },
        )
    }

    fn contains(&self, uuid: UserId) -> bool {
        match self.sessions.get(&uuid) {
            Some(sessions) => !sessions.is_empty(),
            None => false,
        }
    }

    fn get_match_info(&self, room_id: RoomId) -> Option<MatchInfo> {
        if let Some(room) = self.matches.get(&room_id) {
            let info = room.get_info(room_id);
            Some(info)
        } else {
            None
        }
    }

    fn send_message_in_room(&self, room: RoomId, uuid: UserId, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            if !room.contains(uuid) {
                return;
            }
            let msg = serde_json::to_string(&msg).unwrap();
            for uuid in room.get_viewers() {
                if let Some(sessions) = self.sessions.get(uuid) {
                    sessions.send(msg.clone());
                }
            }
        }
    }

    fn broadcast_message(&self, room: RoomId, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            let msg = serde_json::to_string(msg).unwrap();
            for uuid in room.get_viewers() {
                if let Some(sessions) = self.sessions.get(uuid) {
                    sessions.send(msg.clone());
                }
            }
        }
    }

    fn connect(&mut self, uuid: UserId, tx: mpsc::UnboundedSender<String>) -> ConnId {
        let mut rng = rand::rng();
        let conn_id = loop {
            let result: ConnId = rng.random();
            if self
                .sessions
                .iter()
                .any(|(_, sessions)| sessions.contains(result))
            {
                continue;
            } else {
                break result;
            }
        };
        if let Some(sessions) = self.sessions.get_mut(&uuid) {
            sessions.insert(conn_id, tx);
        } else {
            self.sessions.insert(
                uuid,
                Sessions {
                    inner: HashMap::from([(conn_id, tx)]),
                },
            );
        }
        conn_id
    }

    fn start_matching(&mut self, uuid: UserId) {
        self.waitings.insert(uuid);
    }

    fn stop_matching(&mut self, uuid: UserId) {
        self.waitings.remove(&uuid);
    }

    fn rematch(&mut self, room_id: RoomId, uuid: UserId) -> bool {
        let room = self.matches.get_mut(&room_id);
        if let Some(room) = room {
            room.join_rematch(uuid)
        } else {
            false
        }
    }

    fn _create_match_room(&mut self, blue: Option<UserId>, green: Option<UserId>) -> RoomId {
        let mut rng = rand::rng();
        let match_id = loop {
            let result: RoomId = rng.random();
            if self.matches.contains_key(&result) {
                continue;
            } else {
                break result;
            }
        };
        self.matches.insert(match_id, MatchRoom::new(blue, green));
        match_id
    }

    fn create_match_room(&mut self, uuid: UserId) -> RoomId {
        self.waitings.remove(&uuid);
        let blue: bool = rand::rng().random();
        if blue {
            self._create_match_room(Some(uuid), None)
        } else {
            self._create_match_room(None, Some(uuid))
        }
    }

    fn list_match_room(&self) -> Vec<MatchInfo> {
        let result: Vec<MatchInfo> = self
            .matches
            .iter()
            .filter_map(|(room_id, _)| self.get_match_info(*room_id))
            .collect();
        result
    }

    fn leave_match_room(&mut self, room_id: RoomId, uuid: UserId) {
        if let Some(room) = self.matches.get_mut(&room_id) {
            room.remove(uuid);
            let msg = ServerMessage::leave_message(uuid, room_id);
            self.broadcast_message(room_id, &msg);
        }
    }

    fn join_players_match_room(
        &mut self,
        room_id: RoomId,
        uuid: UserId,
    ) -> Option<MatchInfo> {
        if !self.contains(uuid) {
            return None;
        }
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                if room.is_player(uuid) {
                    let info = self.get_match_info(room_id).unwrap();
                    return Some(info);
                };
                let result = room.join_players(uuid, None);
                if !result {
                    return None;
                }
                let msg = ServerMessage::join_message(uuid, room_id);
                self.broadcast_message(room_id, &msg);
                let info = self.get_match_info(room_id).unwrap();
                let msg = ServerMessage::match_message(&info, room_id);
                self.broadcast_message(room_id, &msg);

                Some(info)
            }
            None => None,
        }
    }

    fn join_viewers_match_room(
        &mut self,
        room_id: RoomId,
        uuid: UserId,
    ) -> Option<MatchInfo> {
        if !self.contains(uuid) {
            return None;
        }
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                if room.join_viewers(uuid) {
                    let info = self.get_match_info(room_id).unwrap();
                    let msg = ServerMessage::join_message(uuid, room_id);
                    self.broadcast_message(room_id, &msg);
                    Some(info)
                } else {
                    let info = self.get_match_info(room_id).unwrap();
                    Some(info)
                }
            }
            None => None,
        }
    }

    fn make_move(&mut self, mv: Move, room_id: RoomId, uuid: UserId) -> Option<Duration> {
        if !self.contains(uuid) {
            return None;
        }

        let room = self.matches.get_mut(&room_id);

        if room.is_none() {
            return None;
        }

        let room = room.unwrap();
        let result = room.make_move(mv, uuid);

        let room = self.matches.get(&room_id).unwrap();

        if result.is_some() {
            let msg = ServerMessage::move_message(&mv, room_id);
            self.send_message_in_room(room_id, uuid, &msg);
        }

        if let MatchRoomState::Ended(winner) = room.get_state() {
            let msg = ServerMessage::end_message(room_id, Some(*winner));
            self.send_message_in_room(room_id, uuid, &msg);
        }

        result
    }

    fn resign(&mut self, room_id: RoomId, uuid: UserId) -> bool {
        if !self.contains(uuid) {
            return false;
        }

        let room = self.matches.get_mut(&room_id);

        if room.is_none() {
            return false;
        }

        let room = room.unwrap();
        let result = room.resign(uuid);

        let room = self.matches.get(&room_id).unwrap();

        if result {
            let msg = ServerMessage::resign_message(uuid, room_id);
            self.send_message_in_room(room_id, uuid, &msg);
        }

        if let MatchRoomState::Ended(winner) = room.get_state() {
            let msg = ServerMessage::end_message(room_id, Some(*winner));
            self.send_message_in_room(room_id, uuid, &msg);
        }

        result
    }

    fn remove_user(&mut self, uuid: UserId) {
        self.waitings.remove(&uuid);
        self.sessions.remove(&uuid);
        let rooms: Vec<RoomId> = self
            .matches
            .iter_mut()
            .filter_map(|(room_id, room)| {
                if room.remove(uuid) {
                    Some(*room_id)
                } else {
                    None
                }
            })
            .collect();
        for room_id in rooms {
            let msg = ServerMessage::leave_message(uuid, room_id);
            self.broadcast_message(room_id, &msg);
        }
    }

    fn disconnect(&mut self, conn: ConnId, uuid: UserId) {
        let sessions = self.sessions.get_mut(&uuid).unwrap();
        sessions.remove(conn);
        if sessions.is_empty() {
            self.remove_user(uuid);
        }
    }

    fn reconnect(&mut self, uuid: UserId) -> Vec<MatchInfo> {
        let mut result = Vec::new();
        let rooms: Vec<RoomId> = self
            .matches
            .iter()
            .filter_map(|(room_id, room)| {
                if room.is_player(uuid) {
                    Some(*room_id)
                } else {
                    None
                }
            })
            .collect();
        for room_id in rooms {
            if let Some(info) = self.join_viewers_match_room(room_id, uuid) {
                result.push(info);
            }
        }
        result
    }

    pub async fn run(mut self) -> io::Result<()> {
        loop {
            tokio::select! {
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {

                        Command::Connect {
                            uuid,
                            conn_tx,
                            res_tx,
                        } => {
                            let result = self.connect(uuid, conn_tx);
                            let _ = res_tx.send(result);
                        }

                        Command::Disconnect { conn, uuid } => {
                            self.disconnect(conn, uuid);
                        }

                        Command::Message { msg, room, uuid, res_tx } => {
                            let msg = ServerMessage::chat_message(msg, room);
                            self.send_message_in_room(room, uuid, &msg);
                            let _ = res_tx.send(());
                        }

                        Command::StartMatching { uuid, res_tx } => {
                            self.start_matching(uuid);
                            let _ = res_tx.send(());
                        }

                        Command::StopMatching { uuid, res_tx } => {
                            self.stop_matching(uuid);
                            let _ = res_tx.send(());
                        }

                        Command::PlayerJoin { room, uuid, res_tx } => {
                            let result = self.join_players_match_room(room, uuid);
                            let _ = res_tx.send(result);
                        }

                        Command::ViewerJoin { room, uuid, res_tx } => {
                            let result = self.join_viewers_match_room(room, uuid);
                            let _ = res_tx.send(result);
                        }

                        Command::Move { mv, room, uuid, res_tx } => {
                            let result = self.make_move(mv, room, uuid);
                            let _ = res_tx.send(result);
                        },

                        Command::Resign { room, uuid, res_tx } => {
                            let result = self.resign(room, uuid);
                            let _ = res_tx.send(result);
                        },

                        Command::CreateMatchRoom{uuid, res_tx} => {
                            let result = self.create_match_room(uuid);
                            let _ = res_tx.send(result);
                        },

                        Command::ListMatchRoom{ res_tx } => {
                            let result = self.list_match_room();
                            let _ = res_tx.send(result);
                        },

                        Command::LeaveMatchRoom{ room, uuid, res_tx } => {
                            self.leave_match_room(room, uuid);
                            let _ = res_tx.send(());
                        },

                        Command::GetMatchRoomById{ room, res_tx } => {
                            let result = self.get_match_info(room);
                            let _ = res_tx.send(result);
                        },

                        Command::Reconnect{ uuid, res_tx } => {
                            let result = self.reconnect(uuid);
                            let _ = res_tx.send(result);
                        }
                    }
                }

                Some(task) = self.task_rx.recv() => {
                    match task {
                        BackgroundTask::MatchPlayers => self.try_match_players(),
                        BackgroundTask::CheckConnections => self.check_connections(),
                        BackgroundTask::CheckMatches => self.check_matches(),
                    }
                }
            }
        }
    }

    fn try_match_players(&mut self) {
        if self.waitings.len() >= 2 {
            let mut players: VecDeque<UserId> = self.waitings.drain().collect();

            while players.len() >= 2 {
                let player_blue = players.pop_front().unwrap();
                let player_green = players.pop_front().unwrap();
                let match_id = self
                    ._create_match_room(Some(player_blue), Some(player_green));
                let info = self.get_match_info(match_id).unwrap();
                let msg = ServerMessage::match_message(&info, match_id);
                self.broadcast_message(match_id, &msg);
            }

            while !players.is_empty() {
                let player = players.pop_front().unwrap();
                self.waitings.insert(player);
            }
        }
    }

    fn check_connections(&mut self) {
        let dead_users: Vec<UserId> = self
            .sessions
            .iter_mut()
            .filter_map(|(uuid, sessions)| {
                sessions.remove_closed_sessions();
                if sessions.is_empty() {
                    Some(*uuid)
                } else {
                    None
                }
            })
            .collect();

        for uuid in dead_users {
            self.remove_user(uuid);
        }
    }

    fn check_matches(&mut self) {
        let mut timout_rooms= Vec::new();
        let mut messages = Vec::new();
        for (room_id, room) in self.matches.iter_mut() {
            let msg = match room.check_self(&self.db) {
                MatchRoomState::Ended(winner) => {
                    ServerMessage::end_message(*room_id, Some(winner))
                }
                MatchRoomState::TimeOut => {
                    timout_rooms.push(*room_id);
                    ServerMessage::end_message(*room_id, None)
                }
                _ => { continue; }
            };
            messages.push((*room_id, msg));
        }

        for (room, msg) in messages {
            self.broadcast_message(room, &msg);
        }

        for room in timout_rooms {
            self.matches.remove(&room);
        }
    }
}

#[derive(Clone)]
pub struct MatchServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
    task_tx: mpsc::UnboundedSender<BackgroundTask>,
}

impl MatchServerHandle {
    pub async fn connect(&self, uuid: UserId, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        self.cmd_tx
            .send(Command::Connect {
                uuid,
                conn_tx,
                res_tx,
            })
            .unwrap();

        res_rx.await.unwrap()
    }

    pub async fn send_message(&self, room: RoomId, uuid: UserId, msg: impl Into<String>) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                room,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub fn disconnect(&self, conn: ConnId, uuid: UserId) {
        self.cmd_tx
            .send(Command::Disconnect { conn, uuid })
            .unwrap();
    }

    pub async fn start_matching(&self, uuid: UserId) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StartMatching { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn stop_matching(&self, uuid: UserId) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StopMatching { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn join_viewers(&self, room: RoomId, uuid: UserId) -> Option<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin { room, uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn join_players(&self, room: RoomId, uuid: UserId) -> Option<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PlayerJoin { room, uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn create_match_room(&self, uuid: UserId) -> RoomId {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::CreateMatchRoom { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn make_move(&self, mv: Move, room: RoomId, uuid: UserId) -> Option<Duration> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Move {
                mv,
                room,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn resign(&self, room: RoomId, uuid: UserId) -> bool {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Resign { room, uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn list_match_room(&self) -> Vec<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx.send(Command::ListMatchRoom { res_tx }).unwrap();
        res_rx.await.unwrap()
    }

    pub async fn leave_match_room(&self, room: RoomId, uuid: UserId) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::LeaveMatchRoom { room, uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn get_match_room_by_id(&self, room: RoomId) -> Option<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::GetMatchRoomById { room, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn reconnect(&self, uuid: UserId) -> Vec<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Reconnect { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub fn schedule_background_task(
        &self,
        task: BackgroundTask,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<BackgroundTask>> {
        self.task_tx.send(task)
    }
}
