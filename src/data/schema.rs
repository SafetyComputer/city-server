// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "winner"))]
    pub struct Winner;
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::Winner;

    matches (id) {
        id -> Int4,
        player_blue -> Int4,
        player_green -> Int4,
        winner -> Winner,
        #[max_length = 10000]
        history -> Varchar,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 20]
        username -> Varchar,
        #[max_length = 20]
        password -> Varchar,
        elo -> Int4,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    matches,
    users,
);
