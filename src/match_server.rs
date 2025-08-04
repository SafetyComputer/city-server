use std::{
    collections::{HashMap, HashSet},
    io,
};

use rand::Rng;
use tokio::sync::{mpsc, oneshot};

use crate::game::{Game, Move};

pub type ConnId = u64;

pub type RoomId = u64;

enum Color {
    Blue,
    Green,
}

pub enum BackgroundTask {
    MatchPlayers,
    CheckConnections,
    CheckMatches,
}
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

    fn is_audience(&self, id: ConnId) -> bool {
        self.viewers.contains(&id) && !self.is_player(id)
    }

    fn has_both_player(&self) -> bool {
        self.players.blue.is_some() && self.players.green.is_some()
    }

    fn player_join(&mut self, id: ConnId, color: Option<Color>) -> bool {
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

    fn viewer_join(&mut self, id: ConnId) -> bool {
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
        name: String,
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
        res_tx: oneshot::Sender<bool>,
    },

    PlayerJoin {
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<bool>,
    },

    ViewerJoin {
        room: RoomId,
        conn: ConnId,
        res_tx: oneshot::Sender<bool>,
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

    async fn send_message_in_room(&self, room: RoomId, from: ConnId, msg: impl Into<String>) {
        if let Some(room) = self.matches.get(&room) {
            let msg = msg.into();
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

    async fn broadcast_message(&self, room: ConnId, msg: impl Into<String>) {
        if let Some(room) = self.matches.get(&room) {
            let msg = msg.into();
            for conn_id in &room.viewers {
                if let Some(tx) = self.sessions.get(conn_id) {
                    // errors if client disconnected abruptly and hasn't been timed-out yet
                    let _ = tx.send(msg.clone());
                }
            }
        }
    }

    async fn send_message(&self, conn: ConnId, msg: impl Into<String>) {
        if let Some(matc) = self
            .matches
            .iter()
            .find_map(|(matc, room)| room.contains(conn).then_some(matc))
        {
            self.send_message_in_room(*matc, conn, msg).await;
        }
    }

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

    async fn player_join_match_room(&mut self, room: RoomId, conn: ConnId) -> bool {
        let room = self.matches.get_mut(&room);
        match room {
            Some(room) => room.player_join(conn, None),
            None => return false,
        }
    }

    async fn viewer_join_match_room(&mut self, room: RoomId, conn: ConnId) -> bool {
        let room = self.matches.get_mut(&room);
        match room {
            Some(room) => room.viewer_join(conn),
            None => return false,
        }
    }

    async fn make_move(&mut self, mv: Move, room: RoomId, conn: ConnId) -> bool {
        let room = self.matches.get_mut(&room);
        match room {
            Some(room) => {
                let color = room.players.get_color(conn);
                match color {
                    None => return false,
                    Some(Color::Blue) => {
                        if room.game.blue_turn {
                            room.game.make_move(mv, true)
                        } else {
                            false
                        }
                    }
                    Some(Color::Green) => {
                        if !room.game.blue_turn {
                            room.game.make_move(mv, true)
                        } else {
                            false
                        }
                    }
                }
            }
            None => return false,
        }
    }

    async fn disconnect(&mut self, conn: ConnId, name: String) {
        let mut matches: Vec<RoomId> = Vec::new();
        self.waitings.remove(&conn);
        if self.sessions.remove(&conn).is_some() {
            for (room_id, room) in &mut self.matches {
                if room.contains(conn) {
                    matches.push(*room_id);
                }
            }
        }

        for room_id in matches {
            self.send_message_in_room(room_id, conn, format!("{name} has left"))
                .await;
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

                        Command::Disconnect { conn, name } => {
                            self.disconnect(conn, name).await;
                        }

                        Command::Message { msg, room, conn, res_tx } => {
                            self.send_message_in_room(room, conn, msg).await;
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
                            let result = self.player_join_match_room(room, conn).await;
                            let _ = res_tx.send(result);
                        }

                        Command::ViewerJoin { room, conn, res_tx } => {
                            let result = self.viewer_join_match_room(room, conn).await;
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
            let players: Vec<ConnId> = self.waitings.drain().take(2).collect(); // maybe bug, not sure
            let match_id = self
                ._create_match_room(Some(players[0]), Some(players[1]))
                .await;

            // 通知玩家匹配成功
            if let Some(tx) = self.sessions.get(&players[0]) {
                let _ = tx.send(format!(
                    "matched with {}, match id {}",
                    players[1], match_id
                ));
            }
            if let Some(tx) = self.sessions.get(&players[1]) {
                let _ = tx.send(format!(
                    "matched with {}, match id {}",
                    players[0], match_id
                ));
            }
        }
    }

    async fn check_connections(&mut self) {
        let mut dead_connections = Vec::new();
        for (id, tx) in &self.sessions {
            if tx.is_closed() {
                dead_connections.push(*id);
            }
        }

        for id in dead_connections {
            self.disconnect(id, "anonymous".to_string()).await;
        }
    }

    async fn check_matches(&mut self) {
        let mut empty_rooms = Vec::new();

        for (room_id, room) in &self.matches {
            if room.is_empty() {
                empty_rooms.push(*room_id);
            }
        }

        for room_id in empty_rooms {
            self.broadcast_message(room_id, "match ended").await;
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

    pub fn disconnect(&self, conn: ConnId, name: String) {
        self.cmd_tx
            .send(Command::Disconnect { conn, name })
            .unwrap();
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

    pub async fn viewer_join(&self, room: RoomId, conn: ConnId) -> bool {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin { room, conn, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn player_join(&self, room: RoomId, conn: ConnId) -> bool {
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

    pub async fn make_move(&self, mv: Move, room: RoomId, conn: ConnId) -> bool {
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
