use std::env;

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{SessionMiddleware, config::PersistentSession, storage::CookieSessionStore};
use actix_web::{App, HttpServer, cookie::Key};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use dotenvy::dotenv;
use futures_util::try_join;

use city_server::matchmaking::MatchServer;
use city_server::web;

fn get_secret_key(key_raw: &String) -> Key {
    let key_chars = key_raw.as_bytes();
    let length = key_chars.len();
    let mut key: [u8; 64] = [0; 64];
    for i in 0..64 {
        key[i] = key_chars[i % length];
    }
    Key::from(&key)
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").unwrap();
    let pool = city_server::data::Dbpool::from(&database_url);
    let conn = &mut pool.get_connection();
    conn.run_pending_migrations(MIGRATIONS).unwrap();

    let key_raw = env::var("KEY").unwrap();
    let key = get_secret_key(&key_raw);

    let (match_server, server_tx) = MatchServer::new(actix_web::web::Data::new(pool.clone()));
    let match_server = tokio::task::spawn(match_server.run());

    let http_server = HttpServer::new(move || {
        App::new()
            .app_data(actix_web::web::Data::new(pool.clone()))
            .app_data(actix_web::web::Data::new(server_tx.clone()))
            .wrap(IdentityMiddleware::default())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), key.clone())
                    .cookie_name("auth".to_owned())
                    .cookie_secure(true)
                    .cookie_same_site(actix_web::cookie::SameSite::None)
                    .session_lifecycle(
                        PersistentSession::default()
                            .session_ttl(actix_web::cookie::time::Duration::days(30)),
                    )
                    .build(),
            )
            .wrap(
                Cors::default()
                    .allowed_origin_fn(|_origin, _req_head| true)
                    .allow_any_method()
                    .allow_any_header()
                    .expose_headers(vec![actix_web::http::header::SET_COOKIE])
                    .supports_credentials()
                    .max_age(3600),
            )
            .service(web::login)
            .service(web::logout)
            .service(web::post_user)
            .service(web::get_user)
            .service(web::get_user_self)
            .service(web::get_match_ws)
            .service(web::get_room)
            .service(web::post_room)
            .service(web::post_room_move)
            .service(web::post_room_chat)
            .service(web::post_room_resign)
            .service(web::patch_room_join)
            .service(web::patch_room_leave)
            .service(web::reconnect)
            .service(web::post_matching)
            .service(web::delete_matching)
            .service(web::get_rematch)
    })
    .bind("0.0.0.0:8088")?
    .run();
    try_join!(http_server, async move { match_server.await.unwrap() },)?;
    Ok(())
}
