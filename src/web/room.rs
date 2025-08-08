use actix_identity::Identity;
use actix_web::{
    HttpResponse, Responder, get, patch, post,
    web::{self, Json, Query},
};
use serde::Deserialize;

use crate::{
    data::Dbpool,
    matchmaking::{MatchServerHandle, RoomId},
    web::user::identity_to_user,
};

#[derive(Deserialize)]
struct RoomQuery {
    room_id: Option<RoomId>,
}

#[get("/room")]
async fn get_room(
    _: Identity,
    handle: web::Data<MatchServerHandle>,
    room_info: Query<RoomQuery>,
) -> impl Responder {
    if let Some(room_id) = room_info.room_id {
        let info = handle.get_match_room_by_id(room_id).await;
        if let Some(info) = info {
            return HttpResponse::Found().json(vec![info]);
        } else {
            return HttpResponse::NotFound().json("no such room");
        }
    }
    let matches = handle.list_match_room().await;
    HttpResponse::Found().json(matches)
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

#[patch("/room/join/{join_as}")]
async fn patch_room_join(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
    room_info: Json<RoomQuery>,
    path: web::Path<String>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    let join_as = path.into_inner();
    match user {
        Ok(user) => {
            let info = match join_as.as_str() {
                "player" => {
                    handle
                        .join_players(room_info.room_id.unwrap(), user.id.unwrap())
                        .await
                }
                "viewer" => {
                    handle
                        .join_players(room_info.room_id.unwrap(), user.id.unwrap())
                        .await
                }
                _ => None,
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

#[patch("/room/leave")]
async fn patch_room_leave(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
    room_info: Json<RoomQuery>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            handle
                .leave_match_room(room_info.room_id.unwrap(), user.id.unwrap())
                .await;
            HttpResponse::Ok().json("success")
        }
        Err(e) => e,
    }
}

#[get("/reconnect")]
async fn reconnect(
    identity: Identity,
    db: web::Data<Dbpool>,
    handle: web::Data<MatchServerHandle>,
) -> impl Responder {
    let user = identity_to_user(identity, db).await;
    match user {
        Ok(user) => {
            let result = handle.reconnect(user.id.unwrap()).await;
            HttpResponse::Ok().json(result)
        }
        Err(e) => e,
    }
}
