use actix_web::web::Data;
use support::database::TestDatabase;
use tokio::sync::mpsc;

use city_server::{
    data::Dbpool,
    matchmaking::service::{BackgroundTask, MatchServer},
};

pub mod support;

#[tokio::test]
async fn test_player_matching() {
    // 创建测试数据库
    let test_db = TestDatabase::new().unwrap();
    let db_pool: Dbpool = Dbpool {
        pool: test_db.pool.clone(),
    };

    // 初始化匹配服务
    let (server, handle) = MatchServer::new(Data::new(db_pool));
    tokio::spawn(server.run());

    // 创建测试用户
    let user1 = test_db.create_user("player1").await.unwrap();
    let user2 = test_db.create_user("player2").await.unwrap();

    // 模拟两个玩家加入匹配队列
    handle.start_matching(user1.id.unwrap()).await;
    handle.start_matching(user2.id.unwrap()).await;

    // 触发匹配任务
    handle
        .schedule_background_task(BackgroundTask::MatchPlayers)
        .unwrap();

    // 验证是否创建了房间
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let rooms = handle.list_match_room().await;
    assert_eq!(rooms.len(), 1);
    let room = &rooms[0];
    assert!(room.player_blue.is_some());
    assert!(room.player_green.is_some());

    // 清理数据库
    test_db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_match_timeout() {
    // 创建测试数据库
    let test_db = TestDatabase::new().unwrap();
    let db_pool = test_db.pool.clone();

    // 初始化匹配服务
    let (server, handle) = MatchServer::new(Data::new(Dbpool { pool: db_pool }));
    tokio::spawn(server.run());

    // 创建测试用户
    let user = test_db.create_user("timeout_user").await.unwrap();

    // 玩家加入匹配队列
    handle.start_matching(user.id.unwrap()).await;

    // 模拟匹配超时
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 触发匹配检查
    handle
        .schedule_background_task(BackgroundTask::CheckMatches)
        .unwrap();

    // 验证玩家仍在等待队列
    let (tx, _rx) = mpsc::unbounded_channel();
    let _ = handle.connect(user.id.unwrap(), tx).await;
    handle
        .schedule_background_task(BackgroundTask::CheckConnections)
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(handle.list_match_room().await.is_empty());

    // 清理数据库
    test_db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_cancel_matching() {
    // 创建测试数据库
    let test_db = TestDatabase::new().unwrap();
    let db_pool: Dbpool = Dbpool {
        pool: test_db.pool.clone(),
    };

    // 初始化匹配服务
    let (server, handle) = MatchServer::new(Data::new(db_pool));
    tokio::spawn(server.run());

    // 创建测试用户
    let user = test_db.create_user("cancel_user").await.unwrap();

    // 玩家加入后取消匹配
    handle.start_matching(user.id.unwrap()).await;
    handle.stop_matching(user.id.unwrap()).await;

    // 触发匹配任务
    handle
        .schedule_background_task(BackgroundTask::MatchPlayers)
        .unwrap();

    // 验证没有创建房间
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(handle.list_match_room().await.is_empty());

    // 清理数据库
    test_db.cleanup().await.unwrap();
}
