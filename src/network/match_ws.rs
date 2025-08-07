use actix_identity::Identity;
use actix_web::{get, web, HttpRequest, HttpResponse, Responder};

use super::user::identity_to_user;
use crate::data::Dbpool;
use crate::matchmaking;
use crate::matchmaking::service::MatchServerHandle;

#[get("/ws")]
async fn get_match_ws(
    req: HttpRequest,
    stream: web::Payload,
    match_server: web::Data<MatchServerHandle>,
    db: web::Data<Dbpool>,
    identity: Identity,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;
    let user = identity_to_user(identity, db).await.unwrap();
    tokio::task::spawn_local(matchmaking::handler::match_ws(
        (**match_server).clone(),
        user,
        session,
        msg_stream,
    ));

    Ok(res)
}

// #[get("room")]
// async fn get_room(
//     req: HttpRequest,
//     match_server: web::Data<MatchServerHandle>,
//     _: Identity
// ) -> impl Responder {

// }