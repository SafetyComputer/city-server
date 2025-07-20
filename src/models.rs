use diesel::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Queryable, Selectable, Identifiable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    #[diesel(deserialize_as = i32)]
    pub id: Option<i32>,
    pub username: String,
    pub password: String,
    pub elo: i32
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::sessions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Session {
    #[diesel(deserialize_as = i32)]
    pub id: Option<i32>,
    pub user_id: i32,
    pub auth_id: String,
    #[diesel(deserialize_as = chrono::NaiveDateTime)]
    pub start_time: Option<chrono::NaiveDateTime>,
    pub active: bool
}