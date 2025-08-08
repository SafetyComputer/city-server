use actix_identity::Identity;
use actix_web::{
    HttpResponse, Responder, get, patch, post,
    web::{self, Json},
};
use serde::Deserialize;

use crate::{
    data::Dbpool,
    matchmaking::{MatchServerHandle, RoomId},
    web::user::identity_to_user,
};

#[derive(Deserialize)]
enum JoinAs {
    Player,
    Viewer,
}

#[derive(Deserialize)]
struct JoinInfo {
    room_id: RoomId,
    join_as: JoinAs,
}

#[get("/room")]
async fn get_room(_: Identity, handle: web::Data<MatchServerHandle>) -> impl Responder {
    let matches = handle.list_match_room().await;
    HttpResponse::Ok().json(matches)
}

#[post("/room")]
async fn post_room(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            let room_id = handle.create_match_room(user.id.unwrap()).await;
            HttpResponse::Ok().json(room_id)
        }
        Err(e) => e,
    }
}

#[patch("/room")]
async fn patch_room(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
    join_info: Json<JoinInfo>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            let info = match join_info.join_as {
                JoinAs::Player => {
                    handle
                        .join_players(join_info.room_id, user.id.unwrap())
                        .await
                }
                JoinAs::Viewer => {
                    handle
                        .join_viewers(join_info.room_id, user.id.unwrap())
                        .await
                }
            };

            if let Some(info) = info {
                HttpResponse::Ok().json(info)
            } else {
                HttpResponse::BadRequest().json("failed to join room")
            }
        }
        Err(e) => e,
    }
}
