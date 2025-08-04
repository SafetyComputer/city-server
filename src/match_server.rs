use std::{
    collections::{HashMap, HashSet},
    io,
};

use rand::Rng;
use tokio::sync::{mpsc, oneshot};

use crate::game::{Game, Move};

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
    blue: Option<i32>,
    green: Option<i32>,
}

impl Players {
    fn get_color(&self, id: i32) -> Option<Color> {
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

    fn contains(&self, id: i32) -> bool {
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

struct MatchRoom {
    game: Game,
    players: Players,
    viewers: HashSet<i32>,
}

impl MatchRoom {
    fn new(blue: Option<i32>, green: Option<i32>) -> Self {
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

    fn is_player(&self, id: i32) -> bool {
        self.players.contains(id)
    }

    fn is_audience(&self, id: i32) -> bool {
        self.viewers.contains(&id) && !self.is_player(id)
    }

    fn has_both_player(&self) -> bool {
        self.players.blue.is_some() && self.players.green.is_some()
    }

    fn player_join(&mut self, id: i32, color: Option<Color>) -> bool {
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

    fn viewer_join(&mut self, id: i32) -> bool {
        self.viewers.insert(id)
    }

    fn contains(&self, id: i32) -> bool {
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
        res_tx: oneshot::Sender<()>,
    },

    Disconnect {
        conn: i32,
        name: String,
    },

    Message {
        msg: String,
        conn: i32,
        res_tx: oneshot::Sender<()>,
    },

    StartMatching {
        conn: i32,
        res_tx: oneshot::Sender<()>,
    },

    StopMatching {
        conn: i32,
        res_tx: oneshot::Sender<()>,
    },

    Move {
        mv: Move,
        conn: i32,
        res_tx: oneshot::Sender<bool>,
    },

    PlayerJoin {
        room: i32,
        conn: i32,
        res_tx: oneshot::Sender<bool>,
    },

    ViewerJoin {
        room: i32,
        conn: i32,
        res_tx: oneshot::Sender<bool>,
    },

    CreateMatchRoom {
        conn: i32,
        res_tx: oneshot::Sender<i32>
    }
}

pub struct MatchServer {
    sessions: HashMap<i32, mpsc::UnboundedSender<String>>,
    matches: HashMap<i32, MatchRoom>,
    waitings: HashSet<i32>,
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

    async fn send_system_message(&self, matc: i32, from: i32, msg: impl Into<String>) {
        if let Some(room) = self.matches.get(&matc) {
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

    async fn send_message(&self, conn: i32, msg: impl Into<String>) {
        if let Some(matc) = self
            .matches
            .iter()
            .find_map(|(matc, room)| room.contains(conn).then_some(matc))
        {
            self.send_system_message(*matc, conn, msg).await;
        }
    }

    async fn connect(&mut self, uuid: i32, tx: mpsc::UnboundedSender<String>) {
        self.sessions.insert(uuid, tx);
    }

    async fn start_matching(&mut self, uuid: i32) {
        self.waitings.insert(uuid);
    }

    async fn stop_matching(&mut self, uuid: i32) {
        self.waitings.remove(&uuid);
    }

    async fn _create_match_room(&mut self, blue: Option<i32>, green: Option<i32>) -> i32 {
        let mut rng = rand::rng();
        let match_id = loop {
            let result: i32 = rng.random();
            if self.matches.contains_key(&result) {
                continue;
            } else {
                break result;
            }
        };
        self.matches.insert(match_id, MatchRoom::new(blue, green));
        match_id
    }

    async fn create_match_room(&mut self, uuid: i32) -> i32 {
        self.waitings.remove(&uuid);
        let mut rng = rand::rng();
        let blue: bool = rng.random();
        if blue {
            self._create_match_room(Some(uuid), None).await
        } else {
            self._create_match_room(None, Some(uuid)).await
        }
    }

    async fn player_join_match_room(&mut self, matc: i32, uuid: i32) -> bool {
        let room = self.matches.get_mut(&matc);
        match room {
            Some(room) => room.player_join(uuid, None),
            None => return false,
        }
    }

    async fn viewer_join_match_room(&mut self, matc: i32, uuid: i32) -> bool {
        let room = self.matches.get_mut(&matc);
        match room {
            Some(room) => room.viewer_join(uuid),
            None => return false,
        }
    }

    async fn disconnect(&mut self, conn_id: i32, name: String) {
        let mut matches: Vec<i32> = Vec::new();
        self.waitings.remove(&conn_id);
        if self.sessions.remove(&conn_id).is_some() {
            for (matc, room) in &mut self.matches {
                if room.contains(conn_id) {
                    matches.push(*matc);
                }
            }
        }

        for matc in matches {
            self.send_system_message(matc, conn_id, format!("{name} has left"))
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
                            self.connect(uuid, conn_tx).await;
                            let _ = res_tx.send(());
                        }

                        Command::Disconnect { conn, name } => {
                            self.disconnect(conn, name).await;
                        }

                        Command::Message { msg, conn, res_tx } => {
                            self.send_message(conn, msg).await;
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

                        Command::Move {mv, conn, res_tx} => {},

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
            let players: Vec<i32> = self.waitings.drain().take(2).collect(); // maybe bug, not sure
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
            self.send_system_message(room_id, -1, "match ended").await;
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
    pub async fn connect(&self, uuid: i32, conn_tx: mpsc::UnboundedSender<String>) {
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

    pub async fn send_message(&self, conn: i32, msg: impl Into<String>) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Message {
                msg: msg.into(),
                conn,
                res_tx,
            })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub fn disconnect(&self, conn: i32, name: String) {
        self.cmd_tx
            .send(Command::Disconnect { conn, name })
            .unwrap();
    }

    pub async fn start_matching(&self, conn: i32) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StartMatching { conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn stop_matching(&self, conn: i32) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::StopMatching { conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn viewer_join(&self, room: i32, conn: i32) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin { room, conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub async fn player_join(&self, room: i32, conn: i32) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PlayerJoin { room, conn, res_tx })
            .unwrap();
        res_rx.await.unwrap();
    }

    pub fn schedule_background_task(
        &self,
        task: BackgroundTask,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<BackgroundTask>> {
        self.task_tx.send(task)
    }
}
