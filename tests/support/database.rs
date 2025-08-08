use std::error::Error;

use city_server::data::models::{Match, User};
use diesel::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

// 嵌入数据库迁移
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

// 测试数据库连接池
pub struct TestDatabase {
    pub pool: Pool<ConnectionManager<PgConnection>>,
}

impl TestDatabase {
    // 初始化数据库连接池并运行迁移
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = Pool::builder().build(manager)?;

        let mut conn = pool.get()?;
        conn.revert_all_migrations(MIGRATIONS)?;
        conn.run_pending_migrations(MIGRATIONS)?;

        Ok(Self { pool })
    }

    // 创建用户夹具
    pub async fn create_user(
        &self,
        name: &str,
    ) -> Result<User, Box<dyn Error + Send + Sync + 'static>> {
        use city_server::data::schema::users::dsl::*;
        use diesel::{ExpressionMethods, QueryDsl};

        let new_user = User {
            id: None,
            username: name.to_string(),
            password: "test_password".to_string(),
            elo: 1000,
        };

        let pool = self.pool.clone();
        let user = tokio::task::spawn_blocking(
            move || -> Result<User, Box<dyn Error + Send + Sync + 'static>> {
                let mut conn = pool.get()?;
                diesel::insert_into(users)
                    .values(&new_user)
                    .execute(&mut conn)?;

                users.order(id.desc()).first(&mut conn).map_err(Into::into)
            },
        )
        .await??;

        Ok(user)
    }

    // 创建比赛夹具
    pub async fn create_match(
        &self,
        player1: i32,
        player2: i32,
    ) -> Result<Match, Box<dyn Error + Send + Sync + 'static>> {
        use city_server::data::schema::matches::dsl::*;
        use diesel::{ExpressionMethods, QueryDsl};

        let new_match = Match {
            id: None,
            player_blue: player1,
            player_green: player2,
            winner: "none".to_string(),
            history: "[]".to_string(),
        };

        let pool = self.pool.clone();
        let match_record = tokio::task::spawn_blocking(
            move || -> Result<Match, Box<dyn Error + Send + Sync + 'static>> {
                let mut conn = pool.get()?;
                diesel::insert_into(matches)
                    .values(&new_match)
                    .execute(&mut conn)?;

                matches
                    .order(id.desc())
                    .first(&mut conn)
                    .map_err(Into::into)
            },
        )
        .await??;

        Ok(match_record)
    }

    // 清理数据库（事务回滚）
    pub async fn cleanup(&self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(
            move || -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
                let mut conn = pool.get()?;
                conn.transaction(|conn| {
                    conn.revert_all_migrations(MIGRATIONS)?;
                    Ok(())
                })
            },
        )
        .await??;

        Ok(())
    }
}
