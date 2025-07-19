use serde::{Deserialize, Serialize};
use actix_web::{post, web, Responder, HttpResponse};
use actix_session::Session;
use diesel::{insert_into, query_dsl::methods::FilterDsl, r2d2::{ConnectionManager, Pool, PooledConnection}, update, ExpressionMethods, PgConnection, RunQueryDsl};

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
        let pool = Pool::builder().build(manager).expect("unable to connect to database");
        Dbpool {
            pool
        }
    }
    pub fn get_connection(&self) -> PooledConnection<ConnectionManager<PgConnection>> {
        self.pool.get().expect("unable to connect to database")
    }
}

#[derive(Deserialize)]
struct UserPost {
   username: String,
   password: String,
   elo: Option<i32>
}

impl UserPost {
    fn to_user(self) -> User {
        User {
            id: None,
            username: self.username,
            password: self.password,
            elo: match self.elo {
                None => 1000,
                Some(s) => s
            }
        }
    }
}

const SESSION_USER_KEY: &str = "user_info";
#[post("/login")]
async fn login(session: Session, db: web::Data<Dbpool>, login_info: web::Query<UserPost>) -> impl Responder {
    use schema::users::dsl::*;
    match session.get::<String>(SESSION_USER_KEY) {
        Ok(Some(user_info)) if user_info == login_info.username => {
            println!("already logged in");
            HttpResponse::Ok().json(AjaxResult::<bool>::success_without_data())
        }
        _ => {
            println!("login now");
            let conn = &mut db.get_connection();
            let result: Vec<User> = users.filter(username.eq(&login_info.username)).filter(password.eq(&login_info.password)).load(conn).expect("db error");
            if !result.is_empty() {
                session.insert::<String>(SESSION_USER_KEY, login_info.username.clone()).unwrap();
                HttpResponse::Ok().json(AjaxResult::<bool>::success_without_data())
            } else {
                HttpResponse::Forbidden().json(AjaxResult::<bool>::fail("password must match username".to_string()))
            }
        }
    }
}



#[derive(Deserialize)]
#[derive(Serialize)]
pub struct AjaxResult<T> {
    msg: String,
    data: Option<Vec<T>>,
}

const MSG_SUCCESS: &str = "success";
impl<T> AjaxResult<T> {

    pub fn success(data_opt: Option<Vec<T>>) -> Self{
         Self {
             msg: MSG_SUCCESS.to_string(),
             data: data_opt
         }
    }

    pub fn success_without_data() -> Self {
        Self::success(Option::None)
    }
    pub fn success_with_single(single: T) -> Self{
        Self {
            msg:  MSG_SUCCESS.to_string(),
            data: Option::Some(vec![single])
        }
    }

    pub fn fail(msg: String) -> Self {
        Self {
            msg,
            data: None
        }
    }

}