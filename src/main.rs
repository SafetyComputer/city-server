use actix_session::{SessionMiddleware, storage::CookieSessionStore};
use actix_web::{App, HttpServer, cookie::Key, web};
use dotenvy::dotenv;
use std::env;
fn get_secret_key() -> Key {
    dotenv().ok();
    let key_raw = env::var("KEY").unwrap();
    let length = key_raw.len();
    let key_chars = key_raw.as_bytes();
    let mut key: [u8; 64] = [0; 64];
    for i in 0..64 {
        key[i] = key_chars[i % length];
    }
    Key::from(&key)
}
#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").unwrap();
    let pool = city_server::Dbpool::from(&database_url);
    let key = get_secret_key();
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .wrap(SessionMiddleware::new(
                CookieSessionStore::default(),
                key.clone(),
            ))
            .service(city_server::login)
            .service(city_server::post_user)
    })
    .bind("127.0.0.1:8088")?
    .run()
    .await
}
