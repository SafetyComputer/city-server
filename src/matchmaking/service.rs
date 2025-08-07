use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
};

use rand::Rng;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::game::logic::{Game, Move, Winner};

use super::handler::ServerMessage;

pub type ConnId = u64;

pub type RoomId = u64;

pub type Uuid = i32;

#[derive(Clone, Copy)]
pub enum Color {
    Blue,
    Green,
}

#[derive(Clone, Copy)]
pub enum BackgroundTask {
    MatchPlayers,
    CheckConnections,
    CheckMatches,
}

pub const BACKGROUND_TASKS: [BackgroundTask; 3] = [
    BackgroundTask::MatchPlayers,
    BackgroundTask::CheckConnections,
    BackgroundTask::CheckConnections,
];

struct Players {
    blue: Option<Uuid>,
    green: Option<Uuid>,
}

impl Players {
    fn get_color(&self, id: Uuid) -> Option<Color> {
        match self.blue {
            None => {}
            Some(blue_id) => {
                if blue_id == id {
                    return Some(Color::Blue);
                }
            }
        }
        match self.green {
            None => {}
            Some(green_id) => {
                if green_id == id {
                    return Some(Color::Green);
                }
            }
        }
        return None;
    }

    fn contains(&self, id: Uuid) -> bool {
        match self.blue {
            None => {}
            Some(blue_id) => {
                if blue_id == id {
                    return true;
                }
            }
        }
        match self.green {
            None => {}
            Some(green_id) => {
                if green_id == id {
                    return true;
                }
            }
        }
        return false;
    }
}

pub struct MoveResult {
    pub success: bool,
    pub winner: Option<Winner>,
}

struct Sessions {
    inner: HashMap<ConnId, mpsc::UnboundedSender<String>>,
}

impl Sessions {
    fn send(&self, message: String) {
        for (_, tx) in &self.inner {
            let _ = tx.send(message.clone());
        }
    }

    fn send_with_skip(&self, message: String, skip: ConnId) {
        for (conn_id, tx) in &self.inner {
            if *conn_id != skip {
                let _ = tx.send(message.clone());
            }
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

#[derive(Serialize)]
pub struct MatchInfo {
    room: RoomId,
    game_history: Vec<Move>,
    player_blue: Option<Uuid>,
    player_green: Option<Uuid>,
    viewers: Vec<Uuid>,
}

struct MatchRoom {
    game: Game,
    players: Players,
    viewers: HashSet<Uuid>,
}

impl MatchRoom {
    fn new(blue: Option<Uuid>, green: Option<Uuid>) -> Self {
        let mut viewers = HashSet::with_capacity(2);
        if let Some(blue_id) = blue {
            viewers.insert(blue_id);
        }
        if let Some(green_id) = green {
            viewers.insert(green_id);
        }
        Self {
            game: Game::new(7, 7),
            players: Players { blue, green },
            viewers,
        }
    }

    fn is_player(&self, id: Uuid) -> bool {
        self.players.contains(id)
    }

    // fn is_audience(&self, id: ConnId) -> bool {
    //     self.viewers.contains(&id) && !self.is_player(id)
    // }

    // fn has_both_player(&self) -> bool {
    //     self.players.blue.is_some() && self.players.green.is_some()
    // }

    fn join_players(&mut self, id: Uuid, color: Option<Color>) -> bool {
        if self.is_player(id) {
            return false;
        }
        match color {
            Some(Color::Blue) => {
                if let None = self.players.blue {
                    self.players.blue = Some(id);
                    self.viewers.insert(id);
                    return true;
                } else {
                    return false;
                }
            }
            Some(Color::Green) => {
                if let None = self.players.green {
                    self.players.green = Some(id);
                    self.viewers.insert(id);
                    return true;
                } else {
                    return false;
                }
            }
            None => {
                if let None = self.players.blue {
                    self.players.blue = Some(id);
                    self.viewers.insert(id);
                    return true;
                }
                if let None = self.players.green {
                    self.players.green = Some(id);
                    self.viewers.insert(id);
                    return true;
                }
                return false;
            }
        }
    }

    fn join_viewers(&mut self, id: Uuid) -> bool {
        self.viewers.insert(id)
    }

    fn remove(&mut self, id: Uuid) -> bool {
        self.viewers.remove(&id)
    }

    fn contains(&self, id: Uuid) -> bool {
        self.viewers.contains(&id)
    }

    fn is_empty(&self) -> bool {
        self.players.blue.is_none() && self.players.green.is_none()
    }
}

enum Command {
    Connect {
        uuid: Uuid,
        conn_tx: mpsc::UnboundedSender<String>,
        res_tx: oneshot::Sender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
        uuid: Uuid,
    },

    Message {
        msg: String,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        res_tx: oneshot::Sender<()>,
    },

    StartMatching {
        uuid: Uuid,
        res_tx: oneshot::Sender<()>,
    },

    StopMatching {
        uuid: Uuid,
        res_tx: oneshot::Sender<()>,
    },

    Move {
        mv: Move,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        res_tx: oneshot::Sender<MoveResult>,
    },

    PlayerJoin {
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        res_tx: oneshot::Sender<Option<ServerMessage>>,
    },

    ViewerJoin {
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        res_tx: oneshot::Sender<Option<ServerMessage>>,
    },

    CreateMatchRoom {
        uuid: Uuid,
        res_tx: oneshot::Sender<RoomId>,
    },

    ListMatchRoom {
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },

    GetMatchRoom {
        room: RoomId,
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },

    Reconnect {
        conn: ConnId,
        uuid: Uuid,
        res_tx: oneshot::Sender<Vec<ServerMessage>>,
    },
}

pub struct MatchServer {
    sessions: HashMap<Uuid, Sessions>,
    matches: HashMap<RoomId, MatchRoom>,
    waitings: HashSet<Uuid>,
    cmd_rx: mpsc::UnboundedReceiver<Command>,
    task_rx: mpsc::UnboundedReceiver<BackgroundTask>,
}

impl MatchServer {
    pub fn new() -> (Self, MatchServerHandle) {
        let matches = HashMap::with_capacity(4);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        (
            Self {
                sessions: HashMap::new(),
                matches,
                waitings: HashSet::new(),
                cmd_rx,
                task_rx,
            },
            MatchServerHandle { cmd_tx, task_tx },
        )
    }

    fn get_match_info(&self, room_id: RoomId) -> Option<MatchInfo> {
        if let Some(room) = self.matches.get(&room_id)
            && !room.is_empty()
        {
            let game_history = room.game.history.clone();
            let info = MatchInfo {
                room: room_id,
                game_history,
                player_blue: room.players.blue,
                player_green: room.players.green,
                viewers: room.viewers.iter().map(|id| *id).collect(),
            };
            Some(info)
        } else {
            None
        }
    }

    async fn send_message_in_room(
        &self,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        msg: &ServerMessage,
    ) {
        if let Some(room) = self.matches.get(&room) {
            if !room.contains(uuid) {
                return;
            }
            let msg = serde_json::to_string(&msg).unwrap();
            for uuid in &room.viewers {
                if let Some(sessions) = self.sessions.get(uuid) {
                    sessions.send_with_skip(msg.clone(), conn);
                }
            }
        }
    }

    async fn broadcast_message(&self, room: RoomId, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            let msg = serde_json::to_string(msg).unwrap();
            for uuid in &room.viewers {
                if let Some(sessions) = self.sessions.get(uuid) {
                    sessions.send(msg.clone());
                }
            }
        }
    }

    async fn connect(&mut self, uuid: Uuid, tx: mpsc::UnboundedSender<String>) -> ConnId {
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

    async fn start_matching(&mut self, uuid: Uuid) {
        self.waitings.insert(uuid);
    }

    async fn stop_matching(&mut self, uuid: Uuid) {
        self.waitings.remove(&uuid);
    }

    async fn _create_match_room(&mut self, blue: Option<Uuid>, green: Option<Uuid>) -> RoomId {
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

    async fn create_match_room(&mut self, uuid: Uuid) -> RoomId {
        self.waitings.remove(&uuid);
        let blue: bool = rand::rng().random();
        if blue {
            self._create_match_room(Some(uuid), None).await
        } else {
            self._create_match_room(None, Some(uuid)).await
        }
    }

    async fn list_match_room(&self) -> Vec<MatchInfo> {
        let result: Vec<MatchInfo> = self
            .matches
            .iter()
            .filter_map(|(room_id, _)| self.get_match_info(*room_id))
            .collect();
        result
    }

    async fn join_players_match_room(
        &mut self,
        room_id: RoomId,
        conn: ConnId,
        uuid: Uuid,
    ) -> Option<ServerMessage> {
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                let result = room.join_players(uuid, None);
                if !result {
                    return None;
                }
                let msg = ServerMessage::join_message(uuid, room_id);
                self.send_message_in_room(room_id, conn, uuid, &msg).await;
                let msg =
                    ServerMessage::match_message(&self.get_match_info(room_id).unwrap(), room_id);
                self.send_message_in_room(room_id, conn, uuid, &msg).await;
                Some(msg)
            }
            None => None,
        }
    }

    async fn join_viewers_match_room(
        &mut self,
        room_id: RoomId,
        conn: ConnId,
        uuid: Uuid,
    ) -> Option<ServerMessage> {
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                if room.join_viewers(uuid) {
                    let info = self.get_match_info(room_id).unwrap();
                    let msg = ServerMessage::join_message(uuid, room_id);
                    self.send_message_in_room(room_id, conn, uuid, &msg).await;
                    Some(ServerMessage::match_message(&info, room_id))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    async fn make_move(
        &mut self,
        mv: Move,
        room_id: RoomId,
        conn: ConnId,
        uuid: Uuid,
    ) -> MoveResult {
        let room = self.matches.get_mut(&room_id);

        if room.is_none() {
            return MoveResult {
                success: false,
                winner: None,
            };
        }

        let room = room.unwrap();
        let success = {
            if room.game.game_over() {
                false
            } else {
                let color = room.players.get_color(uuid);
                match color {
                    None => false,

                    Some(Color::Blue) => {
                        if room.game.blue_turn {
                            let result = room.game.make_move(mv, true);
                            result
                        } else {
                            false
                        }
                    }

                    Some(Color::Green) => {
                        if !room.game.blue_turn {
                            let result = room.game.make_move(mv, true);
                            result
                        } else {
                            false
                        }
                    }
                }
            }
        };

        let game_over = room.game.game_over();
        let result = if game_over {
            let (winner, _) = room.game.game_result();
            MoveResult {
                success,
                winner: Some(winner),
            }
        } else {
            MoveResult {
                success,
                winner: None,
            }
        };

        if success {
            let msg = ServerMessage::move_message(&mv, room_id);
            self.send_message_in_room(room_id, conn, uuid, &msg).await;
        }

        if let Some(winner) = result.winner {
            let msg = ServerMessage::end_message(room_id, winner);
            self.send_message_in_room(room_id, conn, uuid, &msg).await;
        }

        result
    }

    async fn remove_user(&mut self, uuid: Uuid) {
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
            self.broadcast_message(room_id, &msg).await;
        }
    }

    async fn disconnect(&mut self, conn: ConnId, uuid: Uuid) {
        let sessions = self.sessions.get_mut(&uuid).unwrap();
        sessions.remove(conn);
        if sessions.is_empty() {
            self.remove_user(uuid).await;
        }
    }

    async fn reconnect(&mut self, conn: ConnId, uuid: Uuid) -> Vec<ServerMessage> {
        let mut result = Vec::new();
        let rooms: Vec<RoomId> = self
            .matches
            .iter()
            .filter_map(|(room_id, room)| {
                if !room.contains(uuid) && room.is_player(uuid) {
                    Some(*room_id)
                } else {
                    None
                }
            })
            .collect();
        for room_id in rooms {
            if let Some(msg) = self.join_viewers_match_room(room_id, conn, uuid).await {
                result.push(msg);
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
                            let result = self.connect(uuid, conn_tx).await;
                            let _ = res_tx.send(result);
                        }

                        Command::Disconnect { conn, uuid } => {
                            self.disconnect(conn, uuid).await;
                        }

                        Command::Message { msg, room, conn, uuid, res_tx } => {
                            let msg = ServerMessage::chat_message(msg, room);
                            self.send_message_in_room(room, conn, uuid, &msg).await;
                            let _ = res_tx.send(());
                        }

                        Command::StartMatching { uuid, res_tx } => {
                            self.start_matching(uuid).await;
                            let _ = res_tx.send(());
                        }

                        Command::StopMatching { uuid, res_tx } => {
                            self.stop_matching(uuid).await;
                            let _ = res_tx.send(());
                        }

                        Command::PlayerJoin { room, conn, uuid, res_tx } => {
                            let result = self.join_players_match_room(room, conn, uuid).await;
                            let _ = res_tx.send(result);
                        }

                        Command::ViewerJoin { room, conn, uuid, res_tx } => {
                            let result = self.join_viewers_match_room(room, conn, uuid).await;
                            let _ = res_tx.send(result);
                        }

                        Command::Move {mv, room, conn, uuid, res_tx} => {
                            let result = self.make_move(mv, room, conn, uuid).await;
                            let _ = res_tx.send(result);
                        },

                        Command::CreateMatchRoom{uuid, res_tx} => {
                            let result = self.create_match_room(uuid).await;
                            let _ = res_tx.send(result);
                        },

                        Command::ListMatchRoom{ res_tx } => {
                            let result = self.list_match_room().await;
                            let _ = res_tx.send(result);
                        },

                        Command::GetMatchRoom{ room, res_tx } => {},

                        Command::Reconnect{ conn, uuid, res_tx } => {
                            let result = self.reconnect(conn, uuid).await;
                            let _ = res_tx.send(result);
                        }
                    }
                }

                Some(task) = self.task_rx.recv() => {
                    match task {
                        BackgroundTask::MatchPlayers => self.try_match_players().await,
                        BackgroundTask::CheckConnections => self.check_connections().await,
                        BackgroundTask::CheckMatches => self.check_matches().await
                    }
                }
            }
        }
    }

    async fn try_match_players(&mut self) {
        if self.waitings.len() >= 2 {
            let mut players: VecDeque<Uuid> = self.waitings.drain().collect();

            while players.len() >= 2 {
                let player_blue = players.pop_front().unwrap();
                let player_green = players.pop_front().unwrap();
                let match_id = self
                    ._create_match_room(Some(player_blue), Some(player_green))
                    .await;
                let info = self.get_match_info(match_id).unwrap();
                let msg = ServerMessage::match_message(&info, match_id);
                self.broadcast_message(match_id, &msg).await;
            }

            while !players.is_empty() {
                let player = players.pop_front().unwrap();
                self.waitings.insert(player);
            }
        }
    }

    async fn check_connections(&mut self) {
        let dead_users: Vec<Uuid> = self
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
            self.remove_user(uuid).await;
        }
    }

    async fn check_matches(&mut self) {
        let empty_rooms: Vec<RoomId> = self
            .matches
            .iter()
            .filter_map(|(id, room)| if room.is_empty() { Some(*id) } else { None })
            .collect();

        for room_id in empty_rooms {
            let msg = ServerMessage::end_message(room_id, Winner::Draw);
            self.broadcast_message(room_id, &msg).await;
            self.matches.remove(&room_id);
        }
    }
}

#[derive(Clone)]
pub struct MatchServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
    task_tx: mpsc::UnboundedSender<BackgroundTask>,
}

impl MatchServerHandle {
    pub async fn connect(&self, uuid: Uuid, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
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

    pub async fn send_message(
        &self,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
        msg: impl Into<String>,
    ) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                room,
                conn,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub fn disconnect(&self, conn: ConnId, uuid: Uuid) {
        self.cmd_tx
            .send(Command::Disconnect { conn, uuid })
            .unwrap();
    }

    pub async fn start_matching(&self, uuid: Uuid) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StartMatching { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn stop_matching(&self, uuid: Uuid) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StopMatching { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn join_viewers(
        &self,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
    ) -> Option<ServerMessage> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin {
                room,
                conn,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn join_players(
        &self,
        room: RoomId,
        conn: ConnId,
        uuid: Uuid,
    ) -> Option<ServerMessage> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PlayerJoin {
                room,
                conn,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn create_match_room(&self, uuid: Uuid) -> RoomId {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::CreateMatchRoom { uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn make_move(&self, mv: Move, room: RoomId, conn: ConnId, uuid: Uuid) -> MoveResult {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Move {
                mv,
                room,
                conn,
                uuid,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn list_match_room(&self) -> Vec<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ListMatchRoom { res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn reconnect(&self, conn: ConnId, uuid: Uuid) -> Vec<ServerMessage> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Reconnect { conn, uuid, res_tx })
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
