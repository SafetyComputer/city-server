// @generated automatically by Diesel CLI.

diesel::table! {
    sessions (id) {
        id -> Int4,
        user_id -> Int4,
        #[max_length = 32]
        auth_id -> Bpchar,
        start_time -> Timestamptz,
        active -> Bool,
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

diesel::joinable!(sessions -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    users,
);
