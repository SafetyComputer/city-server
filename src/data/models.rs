use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Queryable, Selectable, Identifiable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::data::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    #[diesel(deserialize_as = i32)]
    pub id: Option<i32>,
    pub username: String,
    pub password: String,
    pub elo: i32,
}

impl User {
    pub fn into_user_get(self) -> UserGet {
        UserGet {
            id: self.id.unwrap(),
            username: self.username,
            elo: self.elo,
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Serialize, Deserialize)]
#[diesel(table_name = crate::data::schema::users)]
pub struct UserGet {
    pub id: i32,
    pub username: String,
    pub elo: i32,
}
