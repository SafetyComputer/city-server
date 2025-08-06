use actix_identity::Identity;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, Responder, get, post, web};
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, insert_into};
use serde::Deserialize;

use crate::data::{
    Dbpool,
    models::{User, UserGet},
};

#[derive(Deserialize)]
pub struct UserPost {
    pub username: String,
    pub password: String,
    pub elo: Option<i32>,
}

#[derive(Deserialize)]
pub struct UserQuery {
    pub id: Option<i32>,
    pub username: Option<String>,
}

impl UserPost {
    pub fn to_user(self) -> User {
        User {
            id: None,
            username: self.username,
            password: self.password,
            elo: self.elo.unwrap_or(1000),
        }
    }
}

// 辅助函数：从身份信息获取用户
pub async fn identity_to_user(
    identity: Identity,
    db: web::Data<Dbpool>,
) -> Result<User, diesel::result::Error> {
    use crate::data::schema::users::dsl::*;
    let conn = &mut db.get_connection();
    users
        .filter(username.eq(&identity.id().unwrap()))
        .first(conn)
}

// 登录处理
#[post("/login")]
pub async fn login(
    request: HttpRequest,
    identity: Option<Identity>,
    db: web::Data<Dbpool>,
    user_info: web::Json<UserPost>,
) -> HttpResponse {
    if identity.is_some() {
        return HttpResponse::Unauthorized().json("already logged in");
    }

    use crate::data::schema::users::dsl::*;
    let conn = &mut db.get_connection();
    match users
        .filter(username.eq(&user_info.username))
        .filter(password.eq(&user_info.password))
        .first::<User>(conn)
    {
        Ok(_) => {
            Identity::login(&request.extensions(), user_info.username.clone().into()).unwrap();
            HttpResponse::Ok().json("success")
        }
        Err(_) => HttpResponse::Unauthorized().json("login info error"),
    }
}

// 登出处理
#[post("/logout")]
pub async fn logout(identity: Option<Identity>) -> impl Responder {
    match identity {
        Some(identity) => {
            identity.logout();
            HttpResponse::Ok().json("success")
        }
        None => HttpResponse::Unauthorized().json("haven't logged in"),
    }
}

// 获取用户信息
#[get("/user")]
pub async fn get_user(db: web::Data<Dbpool>, user_info: web::Query<UserQuery>) -> impl Responder {
    use crate::data::schema::users::dsl;
    let conn = &mut db.get_connection();
    let mut query = dsl::users
        .select((dsl::id, dsl::username, dsl::elo))
        .into_boxed();

    if let Some(id) = user_info.id {
        query = query.filter(dsl::id.eq(id));
    }
    if let Some(username) = &user_info.username {
        query = query.filter(dsl::username.eq(username));
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

// 创建用户
#[post("/user")]
pub async fn post_user(db: web::Data<Dbpool>, user_info: web::Json<UserPost>) -> impl Responder {
    use crate::data::schema::users::dsl::*;
    let conn = &mut db.get_connection();
    let existing_users: Vec<User> = users
        .filter(username.eq(&user_info.username))
        .load(conn)
        .expect("db error");

    if existing_users.is_empty() {
        let new_user = user_info.into_inner().to_user();
        match insert_into(users).values(&new_user).execute(conn) {
            Ok(_) => HttpResponse::Ok().json("success"),
            Err(_) => HttpResponse::InternalServerError().json("server database error"),
        }
    } else {
        HttpResponse::Forbidden().json("user already exist")
    }
}
