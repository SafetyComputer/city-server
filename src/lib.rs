use actix_identity::Identity;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, insert_into,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use serde::Deserialize;

pub mod models;
pub mod schema;

use models::*;

#[derive(Clone)]
pub struct Dbpool {
    pub pool: Pool<ConnectionManager<PgConnection>>,
}

impl Dbpool {
    pub fn from(database_url: &str) -> Dbpool {
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = Pool::builder()
            .build(manager)
            .expect("unable to connect to database");
        Dbpool { pool }
    }
    pub fn get_connection(&self) -> PooledConnection<ConnectionManager<PgConnection>> {
        self.pool.get().expect("unable to connect to database")
    }
}

#[derive(Deserialize)]
struct UserPost {
    username: String,
    password: String,
    elo: Option<i32>,
}

#[derive(Deserialize)]
struct UserQuery {
    id: Option<i32>,
    username: Option<String>,
}

impl UserPost {
    fn to_user(self) -> User {
        User {
            id: None,
            username: self.username,
            password: self.password,
            elo: match self.elo {
                None => 1000,
                Some(s) => s,
            },
        }
    }
}

async fn idnetity_to_user(
    identity: Identity,
    db: web::Data<Dbpool>,
) -> Result<User, diesel::result::Error> {
    use schema::users::dsl::*;
    let conn = &mut db.get_connection();
    let result: Result<User, diesel::result::Error> = users
        .filter(username.eq(&identity.id().unwrap()))
        .first(conn);
    result
}

#[post("/login")]
async fn login(
    request: HttpRequest,
    identity: Option<Identity>,
    db: web::Data<Dbpool>,
    user_info: web::Query<UserPost>,
) -> HttpResponse {
    match identity {
        Some(_) => {
            return HttpResponse::Unauthorized().json("already logged in");
        }
        None => {
            use schema::users::dsl::*;
            let conn = &mut db.get_connection();
            let result: Result<User, _> = users
                .filter(username.eq(&user_info.username))
                .filter(password.eq(&user_info.password))
                .first(conn);
            match result {
                Err(_) => HttpResponse::Unauthorized().json("login info error"),
                Ok(_) => {
                    Identity::login(&request.extensions(), user_info.username.clone().into())
                        .unwrap();
                    HttpResponse::Ok().json("success")
                }
            }
        }
    }
}

#[post("/logout")]
async fn logout(identity: Option<Identity>) -> impl Responder {
    match identity {
        Some(identity) => {
            identity.logout();
            HttpResponse::Ok().json("success")
        }
        None => HttpResponse::Unauthorized().json("haven't logged in"),
    }
}

#[get("/user")]
async fn get_user(db: web::Data<Dbpool>, user_info: web::Query<UserQuery>) -> impl Responder {
    use schema::users::dsl;
    let conn = &mut db.get_connection();
    let mut query = dsl::users.select((dsl::id, dsl::username, dsl::elo)).into_boxed();
    if let Some(id) = user_info.id {
        query = query.filter(dsl::id.eq(id));
    }
    if let Some(username) = &user_info.username {
        query = query.filter(dsl::username.eq(username.clone()));
    }
    match query.load::<UserGet>(conn) {
        Ok(users) if !users.is_empty() => HttpResponse::Ok().json(users),
        Ok(_) => HttpResponse::NotFound().json("no such user"),
        Err(e) => {
            eprintln!("Database error: {:?}", e);
            HttpResponse::InternalServerError().json("server database error")
        }
    }
}

#[post("/user")]
async fn post_user(db: web::Data<Dbpool>, user_info: web::Query<UserPost>) -> impl Responder {
    use schema::users::dsl::*;
    let conn = &mut db.get_connection();
    let result: Vec<User> = users
        .filter(username.eq(&user_info.username))
        .load(conn)
        .expect("db error");
    if result.is_empty() {
        let new_user = user_info.into_inner().to_user();
        let result = insert_into(users).values(&new_user).execute(conn);
        match result {
            Ok(_) => HttpResponse::Ok().json("success"),
            Err(_) => HttpResponse::InternalServerError().json("server database error"),
        }
    } else {
        HttpResponse::Forbidden().json("user already exist")
    }
}
