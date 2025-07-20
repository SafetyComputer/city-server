use actix_identity::Identity;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use chrono;
use diesel::{
    ExpressionMethods, PgConnection, RunQueryDsl, insert_into,
    QueryDsl,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    update,
};
use rand::{Rng, distr::Alphanumeric, rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

async fn identity_to_user(identity: Identity, db: web::Data<Dbpool>) -> Result<User, diesel::result::Error> {
    use schema::sessions;
    use schema::users;
    let conn = &mut db.get_connection();
    let current_session: Result<Session, diesel::result::Error> = sessions::dsl::sessions
        .filter(sessions::dsl::auth_id.eq(identity.id().unwrap()))
        .filter(sessions::dsl::active.eq(&true))
        .first(conn);
    match current_session {
        Ok(session) => Ok(users::dsl::users.find(session.user_id).first(conn).expect("db error")),
        Err(e) => Err(e)
    }
}

#[post("/login")]
async fn login(
    request: HttpRequest,
    user: Option<Identity>,
    db: web::Data<Dbpool>,
    user_info: web::Query<UserPost>,
) -> HttpResponse {
    match user {
        Some(identity) => {
            let result = identity_to_user(identity, db).await;
            println!("{}", result.unwrap().username);
            return HttpResponse::Unauthorized()
                .json(AjaxResult::<bool>::fail("already logged in".to_string()));
        }
        None => {
            use schema::sessions;
            use schema::users;
            let conn = &mut db.get_connection();
            let result: Vec<User> = users::dsl::users
                .filter(users::dsl::username.eq(&user_info.username))
                .filter(users::dsl::password.eq(&user_info.password))
                .load(conn)
                .expect("db error");
            if result.is_empty() {
                HttpResponse::Unauthorized()
                    .json(AjaxResult::<bool>::fail("login info error".to_string()))
            } else {
                for _ in 0..3 {
                    let mut hasher = Sha256::new();
                    hasher.update(user_info.username.clone());
                    let now = chrono::Local::now();
                    let formatted_time = now.format("%Y-%m-%d %H:%M:%S").to_string();
                    hasher.update(formatted_time);
                    let salt: Vec<u8> = rng().sample_iter(&Alphanumeric).take(10).collect();
                    hasher.update(salt);
                    let id_result: String =
                        hasher.finalize().iter().map(|s| char::from(*s)).collect();
                    let same_auth_sessions: Vec<Session> = sessions::dsl::sessions
                        .filter(sessions::dsl::auth_id.eq(&id_result))
                        .filter(sessions::dsl::active.eq(&true))
                        .load(conn)
                        .expect("db error");
                    if same_auth_sessions.is_empty() {
                        let new_session = Session {
                            id: None,
                            user_id: result[0].id.expect("db error"),
                            auth_id: id_result.clone(),
                            start_time: None,
                            active: true,
                        };
                        let result = insert_into(sessions::dsl::sessions)
                            .values(&new_session)
                            .execute(conn);
                        match result {
                            Ok(_) => {
                                Identity::login(&request.extensions(), id_result).unwrap();
                                return HttpResponse::Ok()
                                    .json(AjaxResult::<bool>::success_without_data());
                            }
                            Err(_) => {
                                return HttpResponse::InternalServerError().json(
                                    AjaxResult::<bool>::fail("server database error".to_string()),
                                );
                            }
                        }
                    } else {
                        continue;
                    }
                }
                HttpResponse::Unauthorized().json(AjaxResult::<bool>::fail(
                    "server failed to generate auth key, try again".to_string(),
                ))
            }
        }
    }
}

#[post("/logout")]
async fn logout(user: Option<Identity>, db: web::Data<Dbpool>) -> impl Responder {
    match user {
        Some(user) => {
            use schema::sessions::dsl::*;
            let conn = &mut db.get_connection();
            update(sessions)
                .filter(auth_id.eq(&user.id().unwrap()))
                .filter(active.eq(&true))
                .set(active.eq(&false))
                .execute(conn)
                .expect("db error");
            user.logout();
            HttpResponse::Ok().json(AjaxResult::<bool>::success_without_data())
        }
        None => HttpResponse::Unauthorized()
            .json(AjaxResult::<bool>::fail("haven't logged in".to_string())),
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
            Ok(_) => HttpResponse::Ok().json(AjaxResult::<bool>::success_without_data()),
            Err(_) => HttpResponse::InternalServerError().json(AjaxResult::<bool>::fail(
                "server database error".to_string(),
            )),
        }
    } else {
        HttpResponse::Forbidden().json(AjaxResult::<bool>::fail("user already exist".to_string()))
    }
}

#[derive(Deserialize, Serialize)]
pub struct AjaxResult<T> {
    msg: String,
    data: Option<Vec<T>>,
}

const MSG_SUCCESS: &str = "success";
impl<T> AjaxResult<T> {
    pub fn success(data_opt: Option<Vec<T>>) -> Self {
        Self {
            msg: MSG_SUCCESS.to_string(),
            data: data_opt,
        }
    }

    pub fn success_without_data() -> Self {
        Self::success(Option::None)
    }
    pub fn success_with_single(single: T) -> Self {
        Self {
            msg: MSG_SUCCESS.to_string(),
            data: Option::Some(vec![single]),
        }
    }

    pub fn fail(msg: String) -> Self {
        Self { msg, data: None }
    }
}
