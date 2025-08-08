// @generated automatically by Diesel CLI.

diesel::table! {
    matches (id) {
        id -> Int4,
        player_blue -> Int4,
        player_green -> Int4,
        #[max_length = 10]
        winner -> Varchar,
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
