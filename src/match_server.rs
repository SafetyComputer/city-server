use std::{
    collections::{HashMap, HashSet},
    io,
};

use tokio::sync::{mpsc, oneshot};
use rand::Rng;

enum Color {
    Blue,
    Green,
}

pub enum BackgroundTask {
    MatchPlayers,
    CheckConnections,
}
struct Players {
    blue: i32,
    green: i32,
}

impl Players {
    fn get_color(&self, id: i32) -> Option<Color> {
        if self.blue == id {
            Some(Color::Blue)
        } else if self.green == id {
            Some(Color::Green)
        } else {
            None
        }
    }

    fn contains(&self, id: i32) -> bool {
        (self.blue == id) | (self.green == id)
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
    },

    Message {
        msg: String,
        conn: i32,
        res_tx: oneshot::Sender<()>,
    },

    StartMatching {
        conn: i32,
        res_tx: oneshot::Sender<()>,
    }
}

pub struct MatchServer {
    sessions: HashMap<i32, mpsc::UnboundedSender<String>>,
    matches: HashMap<i32, Players>,
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
            MatchServerHandle {
                cmd_tx,
                task_tx,
            },
        )
    }

    async fn send_system_message(&self, matc: i32, from: i32, msg: impl Into<String>) {
        if let Some(players) = self.matches.get(&matc) {
            let msg = msg.into();

            if let Some(color) = players.get_color(from) {
                let other_id: i32;
                match color {
                    Color::Blue => other_id = players.green,
                    Color::Green => other_id = players.blue,
                }

                if let Some(tx) = self.sessions.get(&other_id) {
                    let _ = tx.send(msg);
                }
            } else {
                if let Some(tx) = self.sessions.get(&players.blue) {
                    let _ = tx.send(msg.clone());
                }
                if let Some(tx) = self.sessions.get(&players.green) {
                    let _ = tx.send(msg);
                }
            }
        }
    }

    async fn send_message(&self, conn: i32, msg: impl Into<String>) {
        if let Some(matc) = self
            .matches
            .iter()
            .find_map(|(matc, players)| players.contains(conn).then_some(matc))
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

    async fn disconnect(&mut self, conn_id: i32) {
        let mut matches: Vec<i32> = Vec::with_capacity(1);
        self.waitings.remove(&conn_id);
        if self.sessions.remove(&conn_id).is_some() {
            for (matc, players) in &mut self.matches {
                if players.contains(conn_id) {
                    matches.push(*matc);
                }
            }
        }

        for matc in matches {
            self.send_system_message(matc, conn_id, "opponent has left")
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

                        Command::Disconnect { conn } => {
                            self.disconnect(conn).await;
                        }

                        Command::Message { msg, conn, res_tx } => {
                            self.send_message(conn, msg).await;
                            let _ = res_tx.send(());
                        }

                        Command::StartMatching { conn, res_tx } => {
                            self.start_matching(conn).await;
                            let _ = res_tx.send(());
                        }
                    }
                }
                
                Some(task) = self.task_rx.recv() => {
                    match task {
                        BackgroundTask::MatchPlayers => self.try_match_players().await,
                        BackgroundTask::CheckConnections => self.check_connections().await,
                    }
                }
            }
        }
    }
    async fn try_match_players(&mut self) {
        if self.waitings.len() >= 2 {
            let players: Vec<i32> = self.waitings.drain().take(2).collect();
            let mut rng = rand::rng();
            let match_id = loop {
                let result: i32 = rng.random();
                if self.matches.contains_key(&result) {
                    continue;
                } else {
                    break result;
                }
            };
            self.matches.insert(match_id, Players {
                blue: players[0],
                green: players[1],
            });
            
            // 通知玩家匹配成功
            if let Some(tx) = self.sessions.get(&players[0]) {
                let _ = tx.send(format!("MATCHED_WITH {}", players[1]));
            }
            if let Some(tx) = self.sessions.get(&players[1]) {
                let _ = tx.send(format!("MATCHED_WITH {}", players[0]));
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
            self.disconnect(id).await;
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

    pub fn disconnect(&self, conn: i32) {
        self.cmd_tx.send(Command::Disconnect { conn }).unwrap();
    }

    pub async fn start_matching(&self, conn: i32) {
        let (res_tx, res_rx) = oneshot::channel();
        self.cmd_tx.send(Command::StartMatching { conn, res_tx }).unwrap();
        res_rx.await.unwrap();
    }
    
    pub fn schedule_background_task(&self, task: BackgroundTask) -> Result<(), tokio::sync::mpsc::error::SendError<BackgroundTask>> {
        self.task_tx.send(task)
    }
}
