use std::{env, io};

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{SessionMiddleware, config::PersistentSession, storage::CookieSessionStore};
use actix_web::{App, HttpServer, cookie::Key, web};
use dotenvy::dotenv;
use futures_util::try_join;

use city_server::matchmaking::{BACKGROUND_TASKS, MatchServer};

fn get_secret_key(key_raw: &String) -> Key {
    let key_chars = key_raw.as_bytes();
    let length = key_chars.len();
    let mut key: [u8; 64] = [0; 64];
    for i in 0..64 {
        key[i] = key_chars[i % length];
    }
    Key::from(&key)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").unwrap();
    let pool = city_server::Dbpool::from(&database_url);
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
    let http_server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(server_tx.clone()))
            .wrap(Cors::permissive())
            .wrap(IdentityMiddleware::default())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), key.clone())
                    .cookie_name("auth".to_owned())
                    .cookie_secure(false)
                    .session_lifecycle(
                        PersistentSession::default()
                            .session_ttl(actix_web::cookie::time::Duration::hours(3)),
                    )
                    .build(),
            )
            .service(city_server::login)
            .service(city_server::logout)
            .service(city_server::post_user)
            .service(city_server::get_user)
            .service(city_server::match_ws)
    })
    .bind("0.0.0.0:8088")?
    .run();
    try_join!(
        http_server,
        async move { match_server.await.unwrap() },
        async move { match_server_background.await.unwrap() }
    )?;
    Ok(())
}
