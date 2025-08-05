use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
};

use rand::Rng;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::{
    game::{Game, Move, Winner},
    handler::ServerMessage,
};

pub type ConnId = u64;

pub type RoomId = u64;

pub struct JoinRoomResult {
    pub opponent_id: i32,
    pub self_color: Color,
}

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
    blue: Option<ConnId>,
    green: Option<ConnId>,
}

impl Players {
    fn get_color(&self, id: ConnId) -> Option<Color> {
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

    fn contains(&self, id: ConnId) -> bool {
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

struct Session {
    uuid: i32,
    tx: mpsc::UnboundedSender<String>,
}

impl Session {
    fn send(&self, message: String) -> Result<(), mpsc::error::SendError<String>> {
        self.tx.send(message)
    }

    fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

#[derive(Serialize)]
pub struct MatchInfo {
    room: RoomId,
    game_history: Vec<Move>,
    player_blue: Option<i32>,
    player_green: Option<i32>,
    viewers: Vec<i32>,
}

struct MatchRoom {
    game: Game,
    players: Players,
    viewers: HashSet<ConnId>,
}

impl MatchRoom {
    fn new(blue: Option<ConnId>, green: Option<ConnId>) -> Self {
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

    fn is_player(&self, id: ConnId) -> bool {
        self.players.contains(id)
    }

    // fn is_audience(&self, id: ConnId) -> bool {
    //     self.viewers.contains(&id) && !self.is_player(id)
    // }

    // fn has_both_player(&self) -> bool {
    //     self.players.blue.is_some() && self.players.green.is_some()
    // }

    fn join_players(&mut self, id: ConnId, color: Option<Color>) -> bool {
        if self.is_player(id) || self.contains(id) {
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

    fn join_viewers(&mut self, id: ConnId) -> bool {
        self.viewers.insert(id)
    }

    fn contains(&self, id: ConnId) -> bool {
        self.viewers.contains(&id)
    }

    fn is_empty(&self) -> bool {
        self.players.blue.is_none() && self.players.green.is_none()
    }
}

enum Command {
    Connect {
        uuid: i32,
        conn_tx: mpsc::UnboundedSender<String>,
        res_tx: oneshot::Sender<ConnId>,
    },

    Disconnect {
        conn: ConnId,
    },

    Message {
        msg: String,
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<()>,
    },

    StartMatching {
        conn: ConnId,
        res_tx: oneshot::Sender<()>,
    },

    StopMatching {
        conn: ConnId,
        res_tx: oneshot::Sender<()>,
    },

    Move {
        mv: Move,
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<MoveResult>,
    },

    PlayerJoin {
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<Option<ServerMessage>>,
    },

    ViewerJoin {
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<Option<ServerMessage>>,
    },

    CreateMatchRoom {
        conn: ConnId,
        res_tx: oneshot::Sender<RoomId>,
    },
}

pub struct MatchServer {
    sessions: HashMap<ConnId, Session>,
    matches: HashMap<RoomId, MatchRoom>,
    waitings: HashSet<ConnId>,
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

    async fn get_match_info(&self, room_id: RoomId) -> Option<MatchInfo> {
        if let Some(room) = self.matches.get(&room_id)
            && !room.is_empty()
        {
            let game_history = room.game.history.clone();
            let player_blue = if let Some(conn) = room.players.blue {
                Some(self.sessions.get(&conn).unwrap().uuid)
            } else {
                None
            };
            let player_green = if let Some(conn) = room.players.green {
                Some(self.sessions.get(&conn).unwrap().uuid)
            } else {
                None
            };
            let viewers: HashSet<i32> = room
                .viewers
                .iter()
                .map(|conn| self.sessions.get(conn).unwrap().uuid)
                .collect();
            let info = MatchInfo {
                room: room_id,
                game_history,
                player_blue,
                player_green,
                viewers: viewers.into_iter().collect(),
            };
            Some(info)
        } else {
            None
        }
    }

    async fn send_message_in_room(&self, room: RoomId, from: ConnId, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            let msg = serde_json::to_string(&msg).unwrap();
            for conn_id in &room.viewers {
                if *conn_id != from {
                    if let Some(tx) = self.sessions.get(conn_id) {
                        // errors if client disconnected abruptly and hasn't been timed-out yet
                        let _ = tx.send(msg.clone());
                    }
                }
            }
        }
    }

    async fn broadcast_message(&self, room: ConnId, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            let msg = serde_json::to_string(msg).unwrap();
            for conn_id in &room.viewers {
                if let Some(tx) = self.sessions.get(conn_id) {
                    // errors if client disconnected abruptly and hasn't been timed-out yet
                    let _ = tx.send(msg.clone());
                }
            }
        }
    }

    // async fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
    //     if let Some(matc) = self
    //         .matches
    //         .iter()
    //         .find_map(|(matc, room)| room.contains(conn).then_some(matc))
    //     {
    //         self.send_message_in_room(*matc, conn, msg).await;
    //     }
    // }

    async fn connect(&mut self, uuid: i32, tx: mpsc::UnboundedSender<String>) -> ConnId {
        let mut rng = rand::rng();
        let id = loop {
            let result: ConnId = rng.random();
            if self.matches.contains_key(&result) {
                continue;
            } else {
                break result;
            }
        };
        self.sessions.insert(id, Session { uuid, tx });
        id
    }

    async fn start_matching(&mut self, conn: ConnId) {
        self.waitings.insert(conn);
    }

    async fn stop_matching(&mut self, conn: ConnId) {
        self.waitings.remove(&conn);
    }

    async fn _create_match_room(&mut self, blue: Option<ConnId>, green: Option<ConnId>) -> RoomId {
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

    async fn create_match_room(&mut self, conn: ConnId) -> RoomId {
        self.waitings.remove(&conn);
        let blue: bool = rand::rng().random();
        if blue {
            self._create_match_room(Some(conn), None).await
        } else {
            self._create_match_room(None, Some(conn)).await
        }
    }

    async fn join_players_match_room(
        &mut self,
        room_id: RoomId,
        conn: ConnId,
    ) -> Option<ServerMessage> {
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                let result = room.join_players(conn, None);
                if !result {
                    return None;
                }
                let msg =
                    ServerMessage::join_message(self.sessions.get(&conn).unwrap().uuid, room_id);
                self.send_message_in_room(room_id, conn, &msg).await;
                let msg = ServerMessage::match_message(
                    &self.get_match_info(room_id).await.unwrap(),
                    room_id,
                );
                self.send_message_in_room(room_id, conn, &msg).await;
                Some(msg)
            }
            None => None,
        }
    }

    async fn join_viewers_match_room(
        &mut self,
        room_id: RoomId,
        conn: ConnId,
    ) -> Option<ServerMessage> {
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                if room.join_viewers(conn) {
                    let info = self.get_match_info(room_id).await.unwrap();
                    let msg = ServerMessage::join_message(
                        self.sessions.get(&conn).unwrap().uuid,
                        room_id,
                    );
                    self.send_message_in_room(room_id, conn, &msg).await;
                    Some(ServerMessage::match_message(&info, room_id))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    async fn make_move(&mut self, mv: Move, room_id: RoomId, conn: ConnId) -> MoveResult {
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
                let color = room.players.get_color(conn);
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
            self.send_message_in_room(room_id, conn, &msg).await;
        }

        if let Some(winner) = result.winner {
            let msg = ServerMessage::end_message(room_id, winner);
            self.send_message_in_room(room_id, conn, &msg).await;
        }

        result
    }

    async fn disconnect(&mut self, conn: ConnId) {
        // let mut matches: Vec<RoomId> = Vec::new();
        self.waitings.remove(&conn);
        if let Some(session) = self.sessions.remove(&conn) {
            let matches: Vec<RoomId> = self
                .matches
                .iter()
                .filter_map(|(room_id, room)| {
                    if room.contains(conn) {
                        Some(*room_id)
                    } else {
                        None
                    }
                })
                .collect();
            // for (room_id, room) in &mut self.matches {
            //     if room.contains(conn) {
            //         matches.push(*room_id);
            //     }
            // }

            for room_id in matches {
                let msg = ServerMessage::leave_message(session.uuid, room_id);
                self.broadcast_message(room_id, &msg).await;
            }
        }
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

                        Command::Disconnect { conn } => {
                            self.disconnect(conn).await;
                        }

                        Command::Message { msg, room, conn, res_tx } => {
                            let msg = ServerMessage::chat_message(msg, room);
                            self.send_message_in_room(room, conn, &msg).await;
                            let _ = res_tx.send(());
                        }

                        Command::StartMatching { conn, res_tx } => {
                            self.start_matching(conn).await;
                            let _ = res_tx.send(());
                        }

                        Command::StopMatching { conn, res_tx } => {
                            self.stop_matching(conn).await;
                            let _ = res_tx.send(());
                        }

                        Command::PlayerJoin { room, conn, res_tx } => {
                            let result = self.join_players_match_room(room, conn).await;
                            let _ = res_tx.send(result);
                        }

                        Command::ViewerJoin { room, conn, res_tx } => {
                            let result = self.join_viewers_match_room(room, conn).await;
                            let _ = res_tx.send(result);
                        }

                        Command::Move {mv, room, conn, res_tx} => {
                            let result = self.make_move(mv, room, conn).await;
                            let _ = res_tx.send(result);
                        },

                        Command::CreateMatchRoom{conn, res_tx} => {
                            let result = self.create_match_room(conn).await;
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
            let mut players: VecDeque<ConnId> = self.waitings.drain().collect();

            while players.len() >= 2 {
                let player_blue = players.pop_front().unwrap();
                let player_green = players.pop_front().unwrap();
                let match_id = self
                    ._create_match_room(Some(player_blue), Some(player_green))
                    .await;
                let info = self.get_match_info(match_id).await.unwrap();
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
        // let mut dead_connections = Vec::new();
        // for (id, tx) in &self.sessions {
        //     if tx.is_closed() {
        //         dead_connections.push(*id);
        //     }
        // }

        let dead_connections: Vec<ConnId> = self
            .sessions
            .iter()
            .filter_map(|(id, tx)| if tx.is_closed() { Some(*id) } else { None })
            .collect();

        for conn_id in dead_connections {
            self.disconnect(conn_id).await;
        }
    }

    async fn check_matches(&mut self) {
        // let mut empty_rooms = Vec::new();

        // for (room_id, room) in &self.matches {
        //     if room.is_empty() {
        //         empty_rooms.push(*room_id);
        //     }
        // }

        let empty_rooms: Vec<RoomId> = self
            .matches
            .iter()
            .filter_map(|(id, room)| {
                if room.is_empty() {
                    Some(*id)
                } else {
                    None
                }
            })
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
    pub async fn connect(&self, uuid: i32, conn_tx: mpsc::UnboundedSender<String>) -> ConnId {
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

    pub async fn send_message(&self, room: RoomId, conn: ConnId, msg: impl Into<String>) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                room,
                conn,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub fn disconnect(&self, conn: ConnId) {
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }

    pub async fn start_matching(&self, conn: ConnId) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StartMatching { conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn stop_matching(&self, conn: ConnId) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StopMatching { conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn join_viewers(&self, room: RoomId, conn: ConnId) -> Option<ServerMessage> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin { room, conn, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn join_players(&self, room: RoomId, conn: ConnId) -> Option<ServerMessage> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PlayerJoin { room, conn, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn create_match_room(&self, conn: ConnId) -> RoomId {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::CreateMatchRoom { conn, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn make_move(&self, mv: Move, room: RoomId, conn: ConnId) -> MoveResult {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Move {
                mv,
                room,
                conn,
                res_tx,
            })
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
