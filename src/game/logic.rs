use std::cmp::PartialEq;
use std::collections::VecDeque;
use std::fmt;
use std::ops::Add;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub enum Cell {
    Empty = 0,
    Blue = 1,
    Green = 2,
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        matches!(self, Cell::Empty)
    }
}

impl fmt::Debug for Cell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cell::Empty => write!(f, " "),
            Cell::Blue | Cell::Green => write!(f, "■"),
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Board<T> {
    board_matrix: Vec<Vec<T>>,
}

impl<T: Copy> Board<T> {
    pub fn new(width: i32, height: i32, default: T) -> Board<T> {
        Board {
            board_matrix: vec![vec![default; width as usize]; height as usize],
        }
    }

    pub fn get(&self, coordinate: Coordinate) -> &T {
        unsafe {
            self.board_matrix
                .get_unchecked(coordinate.y as usize)
                .get_unchecked(coordinate.x as usize)
        }
    }

    pub fn set(&mut self, coordinate: Coordinate, value: T) {
        self.board_matrix[coordinate.y as usize][coordinate.x as usize] = value;
    }
}

impl Board<bool> {
    fn total(&self) -> usize {
        self.board_matrix.iter().flatten().filter(|&&x| x).count()
    }
    fn clean(&mut self) {
        for i in self.board_matrix.iter_mut() {
            i.fill(false);
        }
    }
}

impl Board<i32> {
    fn clean(&mut self) {
        for i in self.board_matrix.iter_mut() {
            i.fill(-1);
        }
    }
}

// impl Board<Cell> {
//     fn to_bool(&self) -> Vec<Vec<bool>> {
//         self.board_matrix
//             .iter()
//             .map(|row| row.iter().map(|&cell| !cell.is_empty()).collect())
//             .collect()
//     }
// }

#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}

pub const DIRECTION_VALUES: [Direction; 4] = [
    Direction::Up,
    Direction::Down,
    Direction::Left,
    Direction::Right,
];

impl Direction {
    pub fn relative_position(self) -> Coordinate {
        match self {
            Direction::Up => Coordinate::new(0, -1),
            Direction::Down => Coordinate::new(0, 1),
            Direction::Left => Coordinate::new(-1, 0),
            Direction::Right => Coordinate::new(1, 0),
        }
    }
}

// implement debug for Direction
impl fmt::Debug for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Up => write!(f, "U"),
            Direction::Down => write!(f, "D"),
            Direction::Left => write!(f, "L"),
            Direction::Right => write!(f, "R"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Coordinate {
    pub x: i32,
    pub y: i32,
}

impl Coordinate {
    pub fn to_tuple(self) -> (i32, i32) {
        (self.x, self.y)
    }
}

impl Coordinate {
    pub fn new(x: i32, y: i32) -> Coordinate {
        Coordinate { x, y }
    }

    pub fn inside(self, width: i32, height: i32) -> bool {
        self.x >= 0 && self.x < width && self.y >= 0 && self.y < height
    }

    pub fn move_to(self, direction: Direction) -> Coordinate {
        self + direction.relative_position()
    }

    // pub fn get_x(&self) -> i32 {
    //     self.x
    // }

    // pub fn get_y(&self) -> i32 {
    //     self.y
    // }
}

impl PartialEq for Coordinate {
    fn eq(&self, other: &Coordinate) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl Add<Coordinate> for Coordinate {
    type Output = Coordinate;

    fn add(self, other: Coordinate) -> Coordinate {
        Coordinate::new(self.x + other.x, self.y + other.y)
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub destination: Coordinate,
    pub place_wall: Direction,
}

impl Move {
    pub fn from_notation(notation: &str) -> Result<Move, &'static str> {
        let x = notation.chars().nth(0).ok_or("Invalid Notation")?;
        let y = notation.chars().nth(1).ok_or("Invalid Notation")?;
        let destination = Coordinate::new(x as i32 - 'a' as i32, y as i32 - '1' as i32);
        let place_wall = match notation.chars().nth(2).ok_or("Invalid Notation")? {
            'U' => Direction::Up,
            'D' => Direction::Down,
            'L' => Direction::Left,
            'R' => Direction::Right,
            _ => return Err("Invalid Notation"),
        };

        Ok(Move::new(destination, place_wall))
    }

    pub fn to_flat(&self) -> ((i32, i32), i32) {
        // destination, wall_direction UDLR -> 0123
        let wall_direction = match self.place_wall {
            Direction::Up => 0,
            Direction::Down => 1,
            Direction::Left => 2,
            Direction::Right => 3,
        };
        (self.destination.to_tuple(), wall_direction)
    }
}

impl Move {
    pub fn new(destination: Coordinate, place_wall: Direction) -> Move {
        Move {
            destination,
            place_wall,
        }
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{:?}",
            (b'a' + self.destination.x as u8) as char,
            (b'1' + self.destination.y as u8) as char,
            self.place_wall
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Score {
    blue: i32,
    green: i32,
}

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum Winner {
    Blue,
    Green,
    Draw,
}

#[derive(Clone)]
pub struct Game {
    pub width: i32,
    pub height: i32,

    pub blue_position: Coordinate,
    pub green_position: Coordinate,

    pub horizontal_walls: Board<Cell>, // 0 for no wall, 1 for blue wall, 2 for green wall
    pub vertical_walls: Board<Cell>,

    pub blue_turn: bool, // true for blue, false for green

    pub history: Vec<Move>, // history of moves
    blue_reachable_cache: Board<bool>,
    green_reachable_cache: Board<bool>,
    blue_steps_cache: Board<i32>,
    green_steps_cache: Board<i32>,
}

impl Game {
    pub fn new(width: i32, height: i32) -> Game {
        Game {
            width,
            height,
            blue_position: Coordinate::new(0, 0), // the top-left corner
            green_position: Coordinate::new(width - 1, height - 1), // bottom-right corner
            horizontal_walls: Board::new(width, height - 1, Cell::Empty),
            vertical_walls: Board::new(width - 1, height, Cell::Empty),
            blue_turn: true,
            history: Vec::new(),
            blue_reachable_cache: Board::new(width, height, false),
            green_reachable_cache: Board::new(width, height, false),
            blue_steps_cache: Board::new(width, height, -1),
            green_steps_cache: Board::new(width, height, -1),
        }
    }

    fn reachable_with_cache(&mut self, start: Coordinate, step: i32, ignore_other_player: bool) {
        let reachable = if start == self.blue_position {
            self.blue_reachable_cache.clean();
            &mut self.blue_reachable_cache
        } else {
            self.green_reachable_cache.clean();
            &mut self.green_reachable_cache
        };
        let mut queue = VecDeque::new();
        queue.push_back((start, 0));
        reachable.set(start, true);

        // determine the "other" player so we don't move over them
        let other_player = if start == self.blue_position {
            self.green_position
        } else {
            self.blue_position
        };

        while !queue.is_empty() {
            let (current, current_step) = queue.pop_front().unwrap();
            if current_step == step {
                continue;
            }

            for direction in DIRECTION_VALUES {
                let next = current.move_to(direction);
                if !next.inside(self.width, self.height) {
                    continue;
                }
                // don't move over the other player's pawn
                if next == other_player && !ignore_other_player {
                    continue;
                }
                if *reachable.get(next) {
                    continue;
                }
                // check walls
                if (direction == Direction::Right && self.vertical_walls.get(current).is_empty())
                    || (direction == Direction::Left && self.vertical_walls.get(next).is_empty())
                    || (direction == Direction::Down
                        && self.horizontal_walls.get(current).is_empty())
                    || (direction == Direction::Up && self.horizontal_walls.get(next).is_empty())
                {
                    reachable.set(next, true);
                    queue.push_back((next, current_step + 1));
                }
            }
        }
    }

    fn steps_with_cache(&mut self, start: Coordinate) {
        let dist = if start == self.blue_position {
            self.blue_steps_cache.clean();
            &mut self.blue_steps_cache
        } else {
            self.green_steps_cache.clean();
            &mut self.green_steps_cache
        };
        let mut queue: VecDeque<(Coordinate, i32)> = VecDeque::new();

        // determine the "other" player so we don't move over them
        let other_player = if start == self.blue_position {
            self.green_position
        } else {
            self.blue_position
        };

        dist.set(start, 0);
        queue.push_back((start, 0));

        while !queue.is_empty() {
            let (current, d) = queue.remove(0).unwrap();
            for dir in DIRECTION_VALUES {
                let next = current.move_to(dir);
                if !next.inside(self.width, self.height) {
                    continue;
                }
                // don't move over the other player's pawn
                if next == other_player {
                    continue;
                }
                // already visited?
                if *dist.get(next) != -1 {
                    continue;
                }
                // check walls
                let can_move = match dir {
                    Direction::Right => self.vertical_walls.get(current).is_empty(),
                    Direction::Left => self.vertical_walls.get(next).is_empty(),
                    Direction::Down => self.horizontal_walls.get(current).is_empty(),
                    Direction::Up => self.horizontal_walls.get(next).is_empty(),
                };
                if can_move {
                    dist.set(next, d + 1);
                    queue.push_back((next, d + 1));
                }
            }
        }
    }

    pub fn possible_moves(&mut self) -> Vec<Move> {
        let mut moves = Vec::new();
        let start = if self.blue_turn {
            self.blue_position
        } else {
            self.green_position
        };
        self.reachable_with_cache(start, 3, false);
        let reachable = if self.blue_turn {
            &mut self.blue_reachable_cache
        } else {
            &mut self.green_reachable_cache
        };

        // get all possible moves
        // the wall placement is possible if the wall is not placed and the edge is not on the border

        for y in 0..self.height {
            for x in 0..self.width {
                if !reachable.get(Coordinate::new(x, y)) {
                    continue;
                }

                let current = Coordinate::new(x, y);
                for direction in DIRECTION_VALUES {
                    let next = current.move_to(direction);
                    if !next.inside(self.width, self.height) {
                        continue;
                    }

                    if direction == Direction::Right && self.vertical_walls.get(current).is_empty()
                        || direction == Direction::Left && self.vertical_walls.get(next).is_empty()
                        || direction == Direction::Down
                            && self.horizontal_walls.get(current).is_empty()
                        || direction == Direction::Up && self.horizontal_walls.get(next).is_empty()
                    {
                        moves.push(Move {
                            destination: current,
                            place_wall: direction,
                        });
                    }
                }
            }
        }

        moves
    }

    pub fn make_move(&mut self, mv: Move, safe: bool) -> bool {
        // make the move
        // if mv is in possible_moves, then make the move and place

        // return true if the move is made, false otherwise

        if safe && !self.possible_moves().contains(&mv) {
            return false;
        }

        if self.blue_turn {
            self.blue_position = mv.destination;
        } else {
            self.green_position = mv.destination;
        }

        let cell = if self.blue_turn {
            Cell::Blue
        } else {
            Cell::Green
        };

        match mv.place_wall {
            Direction::Up => self
                .horizontal_walls
                .set(mv.destination.move_to(Direction::Up), cell),
            Direction::Down => self.horizontal_walls.set(mv.destination, cell),
            Direction::Left => self
                .vertical_walls
                .set(mv.destination.move_to(Direction::Left), cell),
            Direction::Right => self.vertical_walls.set(mv.destination, cell),
        }

        self.blue_turn = !self.blue_turn;
        self.history.push(mv);
        true
    }

    pub fn undo_move(&mut self) {
        let last_move = self.history.pop().expect("No moves to undo");

        let last_position = match self.history.len() {
            0 => Coordinate { x: 0, y: 0 }, // if no moves left, reset to start
            1 => Coordinate {
                x: self.width - 1,
                y: self.height - 1,
            }, // if only one move left, reset to green position
            _ => {
                let second_last_move = self.history[self.history.len() - 2];
                second_last_move.destination
            }
        };

        // remove the wall
        match last_move.place_wall {
            Direction::Up => self
                .horizontal_walls
                .set(last_move.destination.move_to(Direction::Up), Cell::Empty),
            Direction::Down => self
                .horizontal_walls
                .set(last_move.destination, Cell::Empty),
            Direction::Left => self
                .vertical_walls
                .set(last_move.destination.move_to(Direction::Left), Cell::Empty),
            Direction::Right => self.vertical_walls.set(last_move.destination, Cell::Empty),
        }

        // reset the position
        if self.blue_turn {
            self.green_position = last_position;
        } else {
            self.blue_position = last_position;
        }

        self.blue_turn = !self.blue_turn;
    }

    pub fn territory_difference(&mut self) -> i32 {
        // if it takes less steps for one player to reach a cell, the the cell is counted as the player's territory
        // always return blue territory - green territory
        self.steps_with_cache(self.blue_position);
        self.steps_with_cache(self.green_position);
        let blue_dist = &mut self.blue_steps_cache;
        let green_dist = &mut self.green_steps_cache;

        let mut blue_territory = 0;
        let mut green_territory = 0;

        for y in 0..self.height {
            for x in 0..self.width {
                let pos = Coordinate::new(x, y);
                let bd = *blue_dist.get(pos);
                let gd = *green_dist.get(pos);

                // if blue reaches faster (or green can't reach), count for blue
                if bd >= 0 && (gd < 0 || bd < gd) {
                    blue_territory += 1;
                }
                // if green reaches faster (or blue can't reach), count for green
                else if gd >= 0 && (bd < 0 || gd < bd) {
                    green_territory += 1;
                }
            }
        }

        blue_territory - green_territory
    }

    pub fn evaluate(&mut self) -> i32 {
        if self.game_over() {
            self.reachable_with_cache(self.blue_position, self.height * self.height, true);
            self.reachable_with_cache(self.green_position, self.height * self.height, true);
            let blue_score = self.blue_reachable_cache.total() as i32;
            let green_score = self.green_reachable_cache.total() as i32;

            if blue_score > green_score {
                100
            } else if green_score > blue_score {
                -100
            } else {
                0
            }
        } else {
            self.territory_difference()
        }
    }

    pub fn game_over(&mut self) -> bool {
        //     the game is over when the green player can't reach the blue player
        self.reachable_with_cache(self.blue_position, self.height * self.height, true);
        let blue_reachable = &mut self.blue_reachable_cache;

        if !blue_reachable.get(self.green_position) {
            return true;
        }
        false
    }

    pub fn game_result(&mut self) -> (Winner, Score) {
        // the score is the area of the player can reach
        self.reachable_with_cache(self.blue_position, self.height * self.height, true);
        self.reachable_with_cache(self.green_position, self.height * self.height, true);
        let blue_reachable = &mut self.blue_reachable_cache;
        let green_reachable = &mut self.green_reachable_cache;

        let blue_score = blue_reachable.total() as i32;
        let green_score = green_reachable.total() as i32;

        let score = Score {
            blue: blue_score,
            green: green_score,
        };

        match blue_score.cmp(&green_score) {
            std::cmp::Ordering::Greater => (Winner::Blue, score),
            std::cmp::Ordering::Less => (Winner::Green, score),
            std::cmp::Ordering::Equal => (Winner::Draw, score),
        }
    }
}
