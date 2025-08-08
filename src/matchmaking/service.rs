use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
};

use actix_web::web;
use diesel::{
    Connection, ExpressionMethods as _, RunQueryDsl,
    dsl::{insert_into, update},
    query_dsl::methods::FindDsl,
};
use rand::Rng;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::{
    data::{
        Dbpool,
        models::{Match, User},
    },
    game::logic::{Game, Move, Winner},
};

use super::handler::ServerMessage;

pub type ConnId = u64;

pub type RoomId = u64;

pub type Uuid = i32;

fn elo_update(winner_elo: i32, loser_elo: i32) -> (i32, i32) {
    let expected_winner =
        1_f32 / (1_f32 + 10_f32.powf((loser_elo as f32 - winner_elo as f32) / 400_f32));
    let expected_loser = 1_f32 - expected_winner;

    (
        (winner_elo as f32 + 32_f32 * (1_f32 - expected_winner)).round() as i32,
        (loser_elo as f32 + 32_f32 * (0_f32 - expected_loser)).round() as i32,
    )
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
        None
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
        false
    }
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

    // fn send_with_skip(&self, message: String, skip: ConnId) {
    //     for (conn_id, tx) in &self.inner {
    //         if *conn_id != skip {
    //             let _ = tx.send(message.clone());
    //         }
    //     }
    // }

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
    pub room: RoomId,
    pub game_history: Vec<Move>,
    pub player_blue: Option<Uuid>,
    pub player_green: Option<Uuid>,
    pub viewers: Vec<Uuid>,
}

struct MatchRoom {
    game: Game,
    players: Players,
    viewers: HashSet<Uuid>,
    ended: bool,
    winner: Option<Winner>,
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
            ended: false,
            winner: None,
        }
    }

    fn is_player(&self, id: Uuid) -> bool {
        self.players.contains(id)
    }

    fn join_players(&mut self, id: Uuid, color: Option<Color>) -> bool {
        if self.is_player(id) {
            return false;
        }
        match color {
            Some(Color::Blue) => {
                if self.players.blue.is_none() {
                    self.players.blue = Some(id);
                    self.viewers.insert(id);
                    true
                } else {
                    false
                }
            }
            Some(Color::Green) => {
                if self.players.green.is_none() {
                    self.players.green = Some(id);
                    self.viewers.insert(id);
                    true
                } else {
                    false
                }
            }
            None => {
                if self.players.blue.is_none() {
                    self.players.blue = Some(id);
                    self.viewers.insert(id);
                    return true;
                }
                if self.players.green.is_none() {
                    self.players.green = Some(id);
                    self.viewers.insert(id);
                    return true;
                }
                false
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
        if let Some(blue) = self.players.blue
            && self.contains(blue)
        {
            return false;
        }
        if let Some(green) = self.players.green
            && self.contains(green)
        {
            return false;
        }
        true
    }

    fn save(self, db: web::Data<Dbpool>) -> Result<(), diesel::result::Error> {
        use crate::data::schema::matches;
        use crate::data::schema::users;
        let conn = &mut db.get_connection();
        let blue_id = self.players.blue.unwrap();
        let green_id = self.players.green.unwrap();
        let player_blue: User = users::table.find(blue_id).first(conn)?;
        let player_green: User = users::table.find(green_id).first(conn)?;
        let result = Match {
            id: None,
            player_blue: blue_id,
            player_green: green_id,
            winner: self.winner.unwrap(),
            history: serde_json::to_string(&self.game.history).unwrap(),
        };
        conn.transaction(|conn| {
            match self.winner {
                Some(Winner::Blue) => {
                    let (winner_elo, loser_elo) = elo_update(player_blue.elo, player_green.elo);
                    update(users::table.find(blue_id))
                        .set(users::dsl::elo.eq(winner_elo))
                        .execute(conn)?;
                    update(users::table.find(green_id))
                        .set(users::dsl::elo.eq(loser_elo))
                        .execute(conn)?;
                }
                Some(Winner::Green) => {
                    let (winner_elo, loser_elo) = elo_update(player_green.elo, player_blue.elo);
                    update(users::table.find(blue_id))
                        .set(users::dsl::elo.eq(loser_elo))
                        .execute(conn)?;
                    update(users::table.find(green_id))
                        .set(users::dsl::elo.eq(winner_elo))
                        .execute(conn)?;
                }
                _ => {}
            }
            insert_into(matches::table).values(result).execute(conn)?;
            Ok(())
        })
    }

    fn make_move(&mut self, mv: Move, uuid: Uuid) -> bool {
        let success = {
            if self.game.game_over() {
                false
            } else {
                let color = self.players.get_color(uuid);
                match color {
                    None => false,

                    Some(Color::Blue) => {
                        if self.game.blue_turn {
                            self.game.make_move(mv, true)
                        } else {
                            false
                        }
                    }

                    Some(Color::Green) => {
                        if !self.game.blue_turn {
                            self.game.make_move(mv, true)
                        } else {
                            false
                        }
                    }
                }
            }
        };

        let game_over = self.game.game_over();

        if game_over {
            self.ended = true;
            let (winner, _) = self.game.game_result();
            self.winner = Some(winner);
        }

        success
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
        uuid: Uuid,
        res_tx: oneshot::Sender<bool>,
    },

    PlayerJoin {
        room: RoomId,
        uuid: Uuid,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    ViewerJoin {
        room: RoomId,
        uuid: Uuid,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    CreateMatchRoom {
        uuid: Uuid,
        res_tx: oneshot::Sender<RoomId>,
    },

    ListMatchRoom {
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },

    LeaveMatchRoom {
        room: RoomId,
        uuid: Uuid,
        res_tx: oneshot::Sender<()>,
    },

    GetMatchRoomById {
        room: RoomId,
        res_tx: oneshot::Sender<Option<MatchInfo>>,
    },

    Reconnect {
        uuid: Uuid,
        res_tx: oneshot::Sender<Vec<MatchInfo>>,
    },
}

pub struct MatchServer {
    sessions: HashMap<Uuid, Sessions>,
    matches: HashMap<RoomId, MatchRoom>,
    waitings: HashSet<Uuid>,
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

    fn contains(&self, uuid: Uuid) -> bool {
        match self.sessions.get(&uuid) {
            Some(sessions) => !sessions.is_empty(),
            None => false,
        }
    }

    fn get_match_info(&self, room_id: RoomId) -> Option<MatchInfo> {
        if let Some(room) = self.matches.get(&room_id)
            && !room.is_empty() && !room.ended
        {
            let game_history = room.game.history.clone();
            let info = MatchInfo {
                room: room_id,
                game_history,
                player_blue: room.players.blue,
                player_green: room.players.green,
                viewers: room.viewers.iter().copied().collect(),
            };
            Some(info)
        } else {
            None
        }
    }

    async fn send_message_in_room(&self, room: RoomId, uuid: Uuid, msg: &ServerMessage) {
        if let Some(room) = self.matches.get(&room) {
            if !room.contains(uuid) {
                return;
            }
            let msg = serde_json::to_string(&msg).unwrap();
            for uuid in &room.viewers {
                if let Some(sessions) = self.sessions.get(uuid) {
                    sessions.send(msg.clone());
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

    async fn leave_match_room(&mut self, room_id: RoomId, uuid: Uuid) {
        if let Some(room) = self.matches.get_mut(&room_id) {
            room.remove(uuid);
            let msg = ServerMessage::leave_message(uuid, room_id);
            self.broadcast_message(room_id, &msg).await;
        }
    }

    async fn join_players_match_room(&mut self, room_id: RoomId, uuid: Uuid) -> Option<MatchInfo> {
        if !self.contains(uuid) {
            return None;
        }
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                let result = room.join_players(uuid, None);
                if !result {
                    return None;
                }
                let msg = ServerMessage::join_message(uuid, room_id);
                self.broadcast_message(room_id, &msg).await;
                let info = self.get_match_info(room_id).unwrap();
                let msg = ServerMessage::match_message(&info, room_id);
                self.broadcast_message(room_id, &msg).await;

                Some(info)
            }
            None => None,
        }
    }

    async fn join_viewers_match_room(&mut self, room_id: RoomId, uuid: Uuid) -> Option<MatchInfo> {
        if !self.contains(uuid) {
            return None;
        }
        let room = self.matches.get_mut(&room_id);
        match room {
            Some(room) => {
                if room.join_viewers(uuid) {
                    let info = self.get_match_info(room_id).unwrap();
                    let msg = ServerMessage::join_message(uuid, room_id);
                    self.broadcast_message(room_id, &msg).await;
                    Some(info)
                } else {
                    let info = self.get_match_info(room_id).unwrap();
                    Some(info)
                }
            }
            None => None,
        }
    }

    async fn make_move(&mut self, mv: Move, room_id: RoomId, uuid: Uuid) -> bool {
        if !self.contains(uuid) {
            return false;
        }

        let room = self.matches.get_mut(&room_id);

        if room.is_none() {
            return false;
        }

        let room = room.unwrap();
        let result = room.make_move(mv, uuid);

        let room = self.matches.get(&room_id).unwrap();

        if result {
            let msg = ServerMessage::move_message(&mv, room_id);
            self.send_message_in_room(room_id, uuid, &msg).await;
        }

        if room.ended {
            let msg = ServerMessage::end_message(room_id, room.winner);
            self.send_message_in_room(room_id, uuid, &msg).await;
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

    async fn reconnect(&mut self, uuid: Uuid) -> Vec<MatchInfo> {
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
            if let Some(info) = self.join_viewers_match_room(room_id, uuid).await {
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
                            let result = self.connect(uuid, conn_tx).await;
                            let _ = res_tx.send(result);
                        }

                        Command::Disconnect { conn, uuid } => {
                            self.disconnect(conn, uuid).await;
                        }

                        Command::Message { msg, room, uuid, res_tx } => {
                            let msg = ServerMessage::chat_message(msg, room);
                            self.send_message_in_room(room, uuid, &msg).await;
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

                        Command::PlayerJoin { room, uuid, res_tx } => {
                            let result = self.join_players_match_room(room, uuid).await;
                            let _ = res_tx.send(result);
                        }

                        Command::ViewerJoin { room, uuid, res_tx } => {
                            let result = self.join_viewers_match_room(room, uuid).await;
                            let _ = res_tx.send(result);
                        }

                        Command::Move {mv, room, uuid, res_tx} => {
                            let result = self.make_move(mv, room, uuid).await;
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

                        Command::LeaveMatchRoom{ room, uuid, res_tx } => {
                            self.leave_match_room(room, uuid).await;
                            let _ = res_tx.send(());
                        },

                        Command::GetMatchRoomById{ room, res_tx } => {
                            let result = self.get_match_info(room);
                            let _ = res_tx.send(result);
                        },

                        Command::Reconnect{ uuid, res_tx } => {
                            let result = self.reconnect(uuid).await;
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
            let msg = ServerMessage::end_message(room_id, None);
            self.broadcast_message(room_id, &msg).await;
            self.matches.remove(&room_id);
        }

        let ended_rooms: Vec<RoomId> = self
            .matches
            .iter()
            .filter_map(|(id, room)| if room.ended { Some(*id) } else { None })
            .collect();

        for room_id in ended_rooms {
            let room = self.matches.remove(&room_id);
            if let Some(room) = room {
                let _ = room.save(self.db.clone());
            }
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

    pub async fn send_message(&self, room: RoomId, uuid: Uuid, msg: impl Into<String>) {
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

    pub async fn join_viewers(&self, room: RoomId, uuid: Uuid) -> Option<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::ViewerJoin { room, uuid, res_tx })
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn join_players(&self, room: RoomId, uuid: Uuid) -> Option<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::PlayerJoin { room, uuid, res_tx })
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

    pub async fn make_move(&self, mv: Move, room: RoomId, uuid: Uuid) -> bool {
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

    pub async fn list_match_room(&self) -> Vec<MatchInfo> {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx.send(Command::ListMatchRoom { res_tx }).unwrap();
        res_rx.await.unwrap()
    }

    pub async fn leave_match_room(&self, room: RoomId, uuid: Uuid) {
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

    pub async fn reconnect(&self, uuid: Uuid) -> Vec<MatchInfo> {
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
