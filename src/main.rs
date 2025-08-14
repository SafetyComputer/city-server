use std::{env, io};

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{SessionMiddleware, config::PersistentSession, storage::CookieSessionStore};
use actix_web::{App, HttpServer, cookie::Key};
use city_server::matchmaking::service::BackgroundTask;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use dotenvy::dotenv;
use futures_util::try_join;

use city_server::matchmaking::MatchServer;
use city_server::web;
use rustls::ServerConfig;

fn get_secret_key(key_raw: &String) -> Key {
    let key_chars = key_raw.as_bytes();
    let length = key_chars.len();
    let mut key: [u8; 64] = [0; 64];
    for i in 0..64 {
        key[i] = key_chars[i % length];
    }
    Key::from(&key)
}

fn get_tls_config(tls_path: String) -> ServerConfig {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();

    let mut certs_file =
        std::io::BufReader::new(std::fs::File::open(tls_path.clone() + "/cert.pem").unwrap());
    let mut key_file =
        std::io::BufReader::new(std::fs::File::open(tls_path.clone() + "/key.pem").unwrap());

    // load TLS certs and key
    // to create a self-signed temporary cert for testing:
    // `openssl req -x509 -newkey rsa:4096 -nodes -keyout key.pem -out cert.pem -days 365 -subj '/CN=localhost'`
    let tls_certs = rustls_pemfile::certs(&mut certs_file)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let tls_key = rustls_pemfile::pkcs8_private_keys(&mut key_file)
        .next()
        .unwrap()
        .unwrap();

    // set up TLS config options

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(tls_certs, rustls::pki_types::PrivateKeyDer::Pkcs8(tls_key))
        .unwrap()
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").unwrap();
    let pool = city_server::data::Dbpool::from(&database_url);
    let conn = &mut pool.get_connection();
    conn.run_pending_migrations(MIGRATIONS).unwrap();

    let key_raw = env::var("KEY").unwrap();
    let key = get_secret_key(&key_raw);

    let (match_server, server_tx) = MatchServer::new(actix_web::web::Data::new(pool.clone()));
    let background_tx = server_tx.clone();
    let match_server = tokio::task::spawn(match_server.run());
    let match_server_background = tokio::task::spawn(async move {
        let mut match_interval = tokio::time::interval(core::time::Duration::from_secs(1));
        let mut check_connection_interval =
            tokio::time::interval(core::time::Duration::from_secs(300));
        let mut check_matches_interval = tokio::time::interval(core::time::Duration::from_secs(30));
        let mut check_timer_interval =
            tokio::time::interval(core::time::Duration::from_millis(100));
        loop {
            let result = tokio::select! {
                _ = match_interval.tick() => {
                    background_tx.schedule_background_task(BackgroundTask::MatchPlayers)
                }

                _ = check_connection_interval.tick() => {
                    background_tx.schedule_background_task(BackgroundTask::CheckConnections)
                }

                _ = check_matches_interval.tick() => {
                    background_tx.schedule_background_task(BackgroundTask::CheckMatches)
                }

                _ = check_timer_interval.tick() => {
                    background_tx.schedule_background_task(BackgroundTask::CheckTimer)
                }
            };
            if result.is_err() {
                break;
            }
        }
        io::Result::<()>::Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "closed connection",
        ))
    });

    let tls_path = env::var("TLS_PATH").unwrap();
    let tls_config = get_tls_config(tls_path);

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
            .wrap(Cors::permissive())
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
    })
    .bind_rustls_0_23("0.0.0.0:8088", tls_config)?
    //.bind("0.0.0.0:8088")?
    .run();
    try_join!(
        http_server,
        async move { match_server.await.unwrap() },
        async move { match_server_background.await.unwrap() }
    )?;
    Ok(())
}
