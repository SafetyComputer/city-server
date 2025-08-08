mod match_ws;
mod matching;
mod room;
mod user;

pub use match_ws::get_match_ws;
pub use matching::{delete_matching, post_matching};
pub use room::{get_room, patch_room, post_room};
pub use user::{get_user, login, logout, post_user};
