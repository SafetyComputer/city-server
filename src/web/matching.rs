use actix_identity::Identity;
use actix_web::{HttpResponse, Responder, delete, post, web};

use crate::{data::Dbpool, matchmaking::MatchServerHandle, web::user::identity_to_user};

#[post("matching")]
async fn post_matching(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            handle.start_matching(user.id.unwrap()).await;
            HttpResponse::Ok().json("success")
        }
        Err(e) => e,
    }
}

#[delete("matching")]
async fn delete_matching(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            handle.stop_matching(user.id.unwrap()).await;
            HttpResponse::Ok().json("success")
        }
        Err(e) => e,
    }
}
