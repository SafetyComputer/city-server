use std::{collections::HashSet, time::Duration};

use diesel::{
    Connection, ExpressionMethods as _, RunQueryDsl,
    dsl::{insert_into, update},
    query_dsl::methods::FindDsl,
};
use serde::Serialize;

use crate::matchmaking::{RoomId, UserId, timer::CountdownTimer};

use crate::{
    data::{
        Dbpool,
        models::{Match, User},
    },
    game::logic::{Game, Move, Winner},
};

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

#[derive(Clone, Copy, Serialize)]
pub enum PlayerState {
    Matching,
    Ready(UserId),
    Left(UserId),
    WaitingForRematch(UserId),
}

impl PlayerState {
    pub fn get_uuid(&self) -> Option<UserId> {
        match self {
            Self::Matching => None,
            Self::Ready(id) | Self::Left(id) | Self::WaitingForRematch(id) => Some(*id),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }
}

pub struct Players {
    pub blue: PlayerState,
    pub green: PlayerState,
}

impl Players {
    fn get_color(&self, id: UserId) -> Option<Color> {
        if let Some(blue_id) = self.blue.get_uuid()
            && blue_id == id
        {
            return Some(Color::Blue);
        }
        if let Some(green_id) = self.green.get_uuid()
            && green_id == id
        {
            return Some(Color::Green);
        }
        None
    }

    fn contains(&self, id: UserId) -> bool {
        if let Some(blue_id) = self.blue.get_uuid()
            && blue_id == id
        {
            return true;
        }
        if let Some(green_id) = self.green.get_uuid()
            && green_id == id
        {
            return true;
        }
        false
    }

    fn rematch(&mut self) -> bool {
        if let PlayerState::WaitingForRematch(blue_id) = self.blue
            && let PlayerState::WaitingForRematch(green_id) = self.green
        {
            self.blue = PlayerState::Ready(blue_id);
            self.green = PlayerState::Ready(green_id);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy)]
pub enum MatchRoomState {
    Matching,
    Ready,
    PlayerLeft(CountdownTimer),
    WaitingForRematch(CountdownTimer),
    Ended(Winner),
    TimeOut,
}

pub struct MatchRoom {
    game: Game,
    players: Players,
    viewers: HashSet<UserId>,
    state: MatchRoomState,
    blue_timer: CountdownTimer,
    green_timer: CountdownTimer,
}

impl MatchRoom {
    pub fn new(blue: Option<UserId>, green: Option<UserId>) -> Self {
        let mut viewers = HashSet::with_capacity(2);

        let blue_state = if let Some(blue_id) = blue {
            viewers.insert(blue_id);
            PlayerState::Ready(blue_id)
        } else {
            PlayerState::Matching
        };
        let green_state = if let Some(green_id) = green {
            viewers.insert(green_id);
            PlayerState::Ready(green_id)
        } else {
            PlayerState::Matching
        };
        Self {
            game: Game::new(7, 7),
            players: Players {
                blue: blue_state,
                green: green_state,
            },
            viewers,
            state: if blue.is_none() || green.is_none() {
                MatchRoomState::Matching
            } else {
                MatchRoomState::Ready
            },
            blue_timer: CountdownTimer::new(Duration::from_secs(600)),
            green_timer: CountdownTimer::new(Duration::from_secs(600)),
        }
    }

    pub fn get_info(&self, id: RoomId) -> MatchInfo {
        MatchInfo {
            room: id,
            game_history: self.game.history.clone(),
            player_blue: self.players.blue,
            player_green: self.players.green,
            viewers: self.viewers.iter().copied().collect(),
        }
    }

    pub fn get_viewers<'a>(&'a self) -> std::collections::hash_set::Iter<'a, UserId> {
        self.viewers.iter()
    }

    pub fn get_state(&self) -> &MatchRoomState {
        &self.state
    }

    pub fn is_player(&self, id: UserId) -> bool {
        self.players.contains(id)
    }

    pub fn join_players(&mut self, id: UserId, color: Option<Color>) -> bool {
        let result = match color {
            Some(Color::Blue) => match self.players.blue {
                PlayerState::Matching => {
                    self.players.blue = PlayerState::Ready(id);
                    true
                }
                PlayerState::Left(blue_id) => {
                    if blue_id == id {
                        self.players.blue = PlayerState::Ready(id);
                        true
                    } else {
                        false
                    }
                }
                PlayerState::Ready(blue_id) | PlayerState::WaitingForRematch(blue_id) => {
                    blue_id == id
                }
            },
            Some(Color::Green) => match self.players.green {
                PlayerState::Matching => {
                    self.players.green = PlayerState::Ready(id);
                    true
                }
                PlayerState::Left(green_id) => {
                    if green_id == id {
                        self.players.green = PlayerState::Ready(id);
                        true
                    } else {
                        false
                    }
                }
                PlayerState::Ready(green_id) | PlayerState::WaitingForRematch(green_id) => {
                    green_id == id
                }
            },
            None => {
                // 避免重复加入
                if self.is_player(id) {
                    true
                } else if let PlayerState::Matching = self.players.blue {
                    self.players.blue = PlayerState::Ready(id);
                    self.viewers.insert(id);
                    true
                } else if let PlayerState::Matching = self.players.green {
                    self.players.green = PlayerState::Ready(id);
                    self.viewers.insert(id);
                    true
                } else {
                    false
                }
            }
        };
        if self.players.blue.is_ready() && self.players.green.is_ready() {
            self.state = MatchRoomState::Ready;
        }

        result
    }

    pub fn join_viewers(&mut self, id: UserId) -> bool {
        self.viewers.insert(id)
    }

    pub fn join_rematch(&mut self, id: UserId) -> bool {
        if let MatchRoomState::WaitingForRematch(_) = self.state
            && let Some(color) = self.players.get_color(id)
        {
            match color {
                Color::Blue => self.players.blue = PlayerState::WaitingForRematch(id),
                Color::Green => self.players.green = PlayerState::WaitingForRematch(id),
            }
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: UserId) -> bool {
        if let Some(color) = self.players.get_color(id) {
            match color {
                Color::Blue => self.players.blue = PlayerState::Left(id),
                Color::Green => self.players.green = PlayerState::Left(id),
            }
            let mut timer = CountdownTimer::new(Duration::from_secs(60 * 5));
            timer.start();
            self.state = MatchRoomState::PlayerLeft(timer);
        }

        self.viewers.remove(&id)
    }

    pub fn contains(&self, id: UserId) -> bool {
        self.viewers.contains(&id)
    }

    pub fn save(&self, db: &Dbpool) -> Result<(), diesel::result::Error> {
        use crate::data::schema::matches;
        use crate::data::schema::users;
        let winner = if let MatchRoomState::Ended(winner) = self.state {
            winner
        } else {
            return Ok(());
        };
        let conn = &mut db.get_connection();
        let blue_id = self.players.blue.get_uuid().unwrap();
        let green_id = self.players.green.get_uuid().unwrap();
        let player_blue: User = users::table.find(blue_id).first(conn)?;
        let player_green: User = users::table.find(green_id).first(conn)?;
        let result = Match {
            id: None,
            player_blue: blue_id,
            player_green: green_id,
            winner,
            history: serde_json::to_string(&self.game.history).unwrap(),
        };
        conn.transaction(|conn| {
            match winner {
                Winner::Blue => {
                    let (winner_elo, loser_elo) = elo_update(player_blue.elo, player_green.elo);
                    update(users::table.find(blue_id))
                        .set(users::dsl::elo.eq(winner_elo))
                        .execute(conn)?;
                    update(users::table.find(green_id))
                        .set(users::dsl::elo.eq(loser_elo))
                        .execute(conn)?;
                }
                Winner::Green => {
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

    pub fn make_move(&mut self, mv: Move, uuid: UserId) -> Option<Duration> {
        if let MatchRoomState::Ended(_) = self.state {
            return None;
        }

        let color = self.players.get_color(uuid);
        color?;
        let color = color.unwrap();

        let success = {
            match color {
                Color::Blue => {
                    if self.game.blue_turn {
                        self.game.make_move(mv, true)
                    } else {
                        false
                    }
                }

                Color::Green => {
                    if !self.game.blue_turn {
                        self.game.make_move(mv, true)
                    } else {
                        false
                    }
                }
            }
        };

        if success {
            if self.game.game_over() {
                let (winner, _) = self.game.game_result();
                self.state = MatchRoomState::Ended(winner);
            }
            match color {
                Color::Blue => {
                    self.green_timer.start();
                    self.blue_timer.pause();
                    return Some(self.blue_timer.remaining());
                }
                Color::Green => {
                    self.blue_timer.start();
                    self.green_timer.pause();
                    return Some(self.green_timer.remaining());
                }
            }
        }

        None
    }

    pub fn resign(&mut self, uuid: UserId) -> bool {
        if let MatchRoomState::Ended(_) = self.state {
            false
        } else {
            let color = self.players.get_color(uuid);
            match color {
                None => false,

                Some(Color::Blue) => {
                    if self.game.blue_turn {
                        self.state = MatchRoomState::Ended(Winner::Green);
                        true
                    } else {
                        false
                    }
                }

                Some(Color::Green) => {
                    if !self.game.blue_turn {
                        self.state = MatchRoomState::Ended(Winner::Blue);
                        true
                    } else {
                        false
                    }
                }
            }
        }
    }

    pub fn check_player_timout(&mut self) -> Option<Color> {
        if self.blue_timer.is_expired() {
            self.state = MatchRoomState::Ended(Winner::Green);
            return Some(Color::Blue);
        }

        if self.green_timer.is_expired() {
            self.state = MatchRoomState::Ended(Winner::Blue);
            return Some(Color::Green);
        }

        None
    }

    pub fn check_timout(&mut self) -> bool {
        let result = match self.state {
            MatchRoomState::WaitingForRematch(ref timer) => timer.is_expired(),
            MatchRoomState::PlayerLeft(ref timer) => timer.is_expired(),
            MatchRoomState::TimeOut => true,
            _ => false,
        };
        if result {
            self.state = MatchRoomState::TimeOut;
        }
        result
    }

    pub fn check_self(&mut self, db: &Dbpool) -> MatchRoomState {
        match self.state {
            MatchRoomState::Matching => {}
            MatchRoomState::Ready => {
                self.check_player_timout();
            }
            MatchRoomState::Ended(_) => {
                let _ = self.save(db);
                let mut timer = CountdownTimer::new(Duration::from_secs(60 * 3));
                timer.start();
                self.state = MatchRoomState::WaitingForRematch(timer)
            }
            MatchRoomState::PlayerLeft(_) => {
                self.check_timout();
            }
            MatchRoomState::WaitingForRematch(_) => {
                if self.players.rematch() {
                    self.state = MatchRoomState::Ready;
                } else {
                    self.check_timout();
                }
            }
            MatchRoomState::TimeOut => {}
        };
        self.state
    }
}

#[derive(Serialize)]
pub struct MatchInfo {
    pub room: RoomId,
    pub game_history: Vec<Move>,
    pub player_blue: PlayerState,
    pub player_green: PlayerState,
    pub viewers: Vec<UserId>,
}
