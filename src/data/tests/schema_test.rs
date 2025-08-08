#[cfg(test)]
mod tests {
    use std::env;

    use diesel::connection::Connection;
    use diesel::pg::PgConnection;
    use diesel::prelude::*;
    use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // 数据库模式迁移测试
    #[test]
    fn test_migrations() {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap();
        let mut conn =
            PgConnection::establish(&database_url).expect("cant establish database connection");

        conn.test_transaction(|conn| -> Result<(), Box<dyn std::error::Error>> {
            // 运行所有迁移
            conn.run_pending_migrations(MIGRATIONS)
                .expect("failed to execute migrations");

            // TODO: 添加迁移后验证逻辑
            Ok(())
        });
    }
}
