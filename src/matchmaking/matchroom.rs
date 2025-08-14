use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use actix_web::web;
use diesel::{
    Connection, ExpressionMethods as _, RunQueryDsl,
    dsl::{insert_into, update},
    query_dsl::methods::FindDsl,
};

use crate::matchmaking::{Uuid, timer::CountdownTimer};

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

pub struct Players {
    pub blue: Option<Uuid>,
    pub green: Option<Uuid>,
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

pub struct MatchRoom {
    pub game: Game,
    pub players: Players,
    pub viewers: HashSet<Uuid>,
    pub ended: bool,
    pub winner: Option<Winner>,
    pub incomplete_since: Option<Instant>,

    blue_timer: CountdownTimer,
    green_timer: CountdownTimer,
}

impl MatchRoom {
    pub fn new(blue: Option<Uuid>, green: Option<Uuid>) -> Self {
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
            incomplete_since: Some(Instant::now()),
            blue_timer: CountdownTimer::new(Duration::from_secs(600)),
            green_timer: CountdownTimer::new(Duration::from_secs(600)),
        }
    }

    pub fn is_player(&self, id: Uuid) -> bool {
        self.players.contains(id)
    }

    pub fn join_players(&mut self, id: Uuid, color: Option<Color>) -> bool {
        let result = match color {
            Some(Color::Blue) => {
                if self.players.blue.is_none() {
                    self.players.blue = Some(id);
                    self.viewers.insert(id);
                    true
                } else {
                    self.players.blue.unwrap() == id
                }
            }
            Some(Color::Green) => {
                if self.players.green.is_none() {
                    self.players.green = Some(id);
                    self.viewers.insert(id);
                    true
                } else {
                    self.players.green.unwrap() == id
                }
            }
            None => {
                // 避免重复加入
                if self.players.get_color(id).is_some() {
                    return true;
                }

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
                self.is_player(id)
            }
        };

        self.update_incomplete_status();
        result
    }

    pub fn join_viewers(&mut self, id: Uuid) -> bool {
        self.viewers.insert(id)
    }

    pub fn remove(&mut self, id: Uuid) -> bool {
        let result = self.viewers.remove(&id);
        self.update_incomplete_status();
        result
    }

    pub fn contains(&self, id: Uuid) -> bool {
        self.viewers.contains(&id)
    }

    // fn is_empty(&self) -> bool {
    //     if let Some(blue) = self.players.blue
    //         && self.contains(blue)
    //     {
    //         return false;
    //     }
    //     if let Some(green) = self.players.green
    //         && self.contains(green)
    //     {
    //         return false;
    //     }
    //     true
    // }

    pub fn is_full(&self) -> bool {
        let blue_online = self.players.blue.is_some_and(|id| self.contains(id));
        let green_online = self.players.green.is_some_and(|id| self.contains(id));
        self.players.blue.is_some() && self.players.green.is_some() && blue_online && green_online
    }

    pub fn update_incomplete_status(&mut self) {
        if self.is_full() {
            self.incomplete_since = None;
        } else if self.incomplete_since.is_none()
            && (self.players.blue.is_some() || self.players.green.is_some())
        {
            self.incomplete_since = Some(Instant::now());
        }
    }

    pub fn should_close_due_to_timeout(&self) -> bool {
        if let Some(since) = self.incomplete_since {
            since.elapsed() > Duration::from_secs(30 * 60)
        } else {
            false
        }
    }

    pub fn save(self, db: web::Data<Dbpool>) -> Result<(), diesel::result::Error> {
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

    pub fn make_move(&mut self, mv: Move, uuid: Uuid) -> bool {
        if self.ended {
            return false;
        }

        let color = self.players.get_color(uuid);
        if color.is_none() {
            return false;
        }
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
                self.ended = true;
                let (winner, _) = self.game.game_result();
                self.winner = Some(winner);
            } else {
                match color {
                    Color::Blue => {
                        self.green_timer.start();
                        self.blue_timer.pause();
                    }
                    Color::Green => {
                        self.blue_timer.start();
                        self.green_timer.pause();
                    }
                }
            }
        }

        success
    }

    pub fn resign(&mut self, uuid: Uuid) -> bool {
        if self.ended {
            false
        } else {
            let color = self.players.get_color(uuid);
            match color {
                None => false,

                Some(Color::Blue) => {
                    if self.game.blue_turn {
                        self.ended = true;
                        self.winner = Some(Winner::Green);
                        true
                    } else {
                        false
                    }
                }

                Some(Color::Green) => {
                    if !self.game.blue_turn {
                        self.ended = true;
                        self.winner = Some(Winner::Blue);
                        true
                    } else {
                        false
                    }
                }
            }
        }
    }

    pub fn check_timout(&mut self) -> Option<Color> {
        if self.blue_timer.is_expired() {
            self.ended = true;
            self.winner = Some(Winner::Green);
            return Some(Color::Blue);
        }

        if self.green_timer.is_expired() {
            self.ended = true;
            self.winner = Some(Winner::Blue);
            return Some(Color::Green);
        }

        None
    }
}
