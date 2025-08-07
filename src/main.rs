use std::{env, io};

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_rt;
use actix_session::{SessionMiddleware, config::PersistentSession, storage::CookieSessionStore};
use actix_web::{App, HttpServer, cookie::Key, web};
use dotenvy::dotenv;
use futures_util::try_join;

use city_server::matchmaking::{BACKGROUND_TASKS, MatchServer};
use city_server::network;
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
    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(tls_certs, rustls::pki_types::PrivateKeyDer::Pkcs8(tls_key))
        .unwrap();

    tls_config
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").unwrap();
    let pool = city_server::data::Dbpool::from(&database_url);

    let key_raw = env::var("KEY").unwrap();
    let key = get_secret_key(&key_raw);

    let (match_server, server_tx) = MatchServer::new();
    let background_tx = server_tx.clone();
    let match_server = tokio::task::spawn(match_server.run());
    let match_server_background = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(core::time::Duration::from_secs(1));
        let mut next_task = 2;
        loop {
            interval.tick().await;
            let result = background_tx.schedule_background_task(BACKGROUND_TASKS[next_task]);
            match result {
                Ok(_) => next_task = (next_task + 1) % 3,
                Err(_) => break,
            }
        }
        return io::Result::<()>::Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "closed connection",
        ));
    });

    let tls_path = env::var("TLS_PATH").unwrap();
    let tls_config = get_tls_config(tls_path);

    let http_server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(server_tx.clone()))
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
            .service(network::login)
            .service(network::logout)
            .service(network::post_user)
            .service(network::get_user)
            .service(network::get_match_ws)
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
