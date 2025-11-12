use std::fmt;
use std::time;

use rand::Rng;

use super::*;

pub struct MinimaxPlayer {
    game: Game,
}

impl MinimaxPlayer {
    pub fn new(game: Game) -> Self {
        Self { game }
    }

    fn minimax_evaluate(
        &mut self,
        depth: i32,
        alpha: i32,
        beta: i32,
        nodes: &mut u64,
        cutoff: i32,
    ) -> i32 {
        *nodes += 1;

        if self.game.game_over() {
            return self.game.evaluate();
        }

        if depth == 0 {
            return self.game.territory_difference();
        }

        let moves = if depth == 1 {
            self.game.possible_moves()
        } else {
            self.evaluation_sorted_moves(cutoff)
        };

        let mut value = if self.game.blue_turn {
            i32::MIN
        } else {
            i32::MAX
        };
        for mv in moves {
            self.game.make_move(mv, false);
            let score = self.minimax_evaluate(depth - 1, alpha, beta, nodes, cutoff);
            self.game.undo_move();

            if self.game.blue_turn {
                value = value.max(score);
                if value >= beta {
                    break;
                }
            } else {
                value = value.min(score);
                if value <= alpha {
                    break;
                }
            }
        }
        value
    }

    fn minimax_evaluate_moves(&mut self, depth: i32, nodes: &mut u64) -> Vec<EvaluatedMove> {
        // evaluate all first‐level moves and return them sorted
        let mut scored: Vec<EvaluatedMove> = self
            .evaluation_sorted_moves(0)
            .into_iter()
            .map(|mv| {
                self.game.make_move(mv, false);
                let sc = self.minimax_evaluate(depth - 1, i32::MIN, i32::MAX, nodes, 0);
                self.game.undo_move();
                EvaluatedMove::new(mv, sc)
            })
            .collect();

        scored.sort();

        if self.game.blue_turn {
            scored.reverse(); // descending for max player
        }

        scored
    }

    pub fn iterative_deepening_minimax(&mut self, depth: i32) -> EvaluatedMove {
        // iterative deepening minimax with aspiration windows
        let start = time::Instant::now();
        let max_depth = match self.game.history.len().cmp(&6) {
            std::cmp::Ordering::Less => depth + 2, // if less than 6 moves, use 6
            _ => depth + 4,                        // otherwise, use history length + 2
        }; // Maximum depth to search
        let time_limit_secs = 3; // Time limit in seconds

        // Initial search at the base depth to get a starting value
        let evaluated_moves = self.minimax_evaluate_moves(depth, &mut 0u64);
        let mut best_move = evaluated_moves[0].mv;
        let mut best_score = evaluated_moves[0].ev;
        let mut current_depth = depth + 2;

        // Window size parameters
        let mut window_size = 1; // Initial window size

        // Main iterative deepening loop
        while current_depth <= max_depth && start.elapsed().as_secs() < time_limit_secs {
            // println!(
            //     "Searching at depth {} with window around {}",
            //     current_depth, best_score
            // );

            // Set aspiration window bounds
            let mut alpha = best_score - window_size;
            let mut beta = best_score + window_size;
            let mut retry = true;

            // Try search with current window, expand if needed
            while retry {
                retry = false;
                let mut nodes_evaluated = 0u64;

                // Score each first-level move with the current window
                let mut scored: Vec<EvaluatedMove> = self
                    .evaluation_sorted_moves(0)
                    .into_iter()
                    .map(|mv| {
                        self.game.make_move(mv, false);
                        let sc = self.minimax_evaluate(
                            current_depth - 1,
                            alpha,
                            beta,
                            &mut nodes_evaluated,
                            0,
                        );
                        self.game.undo_move();
                        EvaluatedMove::new(mv, sc)
                    })
                    .collect();

                // If score is outside window bounds, retry with wider window
                if !scored.is_empty() {
                    scored.sort();
                    if self.game.blue_turn {
                        scored.reverse();
                    }

                    let new_score = scored[0].ev;

                    // Check if result was outside the window
                    if new_score <= alpha {
                        // Failed low, retry with wider window
                        // println!("Failed low: {} <= {}, widening window", new_score, alpha);
                        window_size *= 2;
                        alpha = new_score - window_size;
                        retry = true;
                        continue;
                    } else if new_score >= beta {
                        // Failed high, retry with wider window
                        // println!("Failed high: {} >= {}, widening window", new_score, beta);
                        window_size *= 2;
                        beta = new_score + window_size;
                        retry = true;
                        continue;
                    } else {
                        // Search succeeded within window
                        best_score = new_score;

                        // Pick randomly among best-scoring moves
                        let best_moves: Vec<Move> = scored
                            .iter()
                            .filter(|em| em.ev == new_score)
                            .map(|em| em.mv)
                            .collect();

                        let mut rng = rand::rng();
                        best_move = best_moves[rng.random_range(0..best_moves.len())];

                        // Diagnostics
                        // println!("Top 5 moves:");
                        // for em in scored.iter().take(5) {
                        //     println!("  {:?}", em);
                        // }
                        // println!("Nodes evaluated: {}", nodes_evaluated);
                        // println!("Elapsed time: {:?}", start.elapsed());
                    }
                }
            }

            // Reset window size for next iteration
            window_size = 1;

            // Increase depth for next iteration
            current_depth += 2;
        }

        // println!("Final best move: {:?} with score {}", best_move, best_score);
        EvaluatedMove::new(best_move, best_score)
    }

    fn evaluate_move(&mut self, mv: Move) -> i32 {
        self.game.make_move(mv, false);
        let score = self.game.evaluate();
        self.game.undo_move();
        score
    }

    fn evaluation_sorted_moves(&mut self, cutoff: i32) -> Vec<Move> {
        // evaluate all possible moves and return them sorted by evaluation value
        let mut scored_moves: Vec<EvaluatedMove> = self
            .game
            .possible_moves()
            .into_iter()
            .map(|mv| {
                let score = self.evaluate_move(mv);
                EvaluatedMove::new(mv, score)
            })
            .collect();

        // sort moves by evaluation value
        scored_moves.sort();
        if self.game.blue_turn {
            scored_moves.reverse(); // descending for max player
        }

        if cutoff > 0 && scored_moves.len() > cutoff as usize {
            scored_moves.truncate(cutoff as usize);
        }

        scored_moves.into_iter().map(|em| em.mv).collect()
    }
}

pub struct EvaluatedMove {
    mv: Move,
    ev: i32,
}

impl EvaluatedMove {
    pub fn new(mv: Move, ev: i32) -> EvaluatedMove {
        EvaluatedMove { mv, ev }
    }
}

impl fmt::Debug for EvaluatedMove {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let sign = if self.ev > 0 { "+" } else { "" };
        write!(f, "{:?} ({}{})", self.mv, sign, self.ev)
    }
}

impl PartialEq for EvaluatedMove {
    // compare only by evaluation value
    fn eq(&self, other: &Self) -> bool {
        self.ev == other.ev
    }
}

impl Eq for EvaluatedMove {}

impl PartialOrd for EvaluatedMove {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvaluatedMove {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ev.cmp(&other.ev)
    }
}
