#[cfg(test)]
mod tests {
    use std::env;

    use diesel::connection::Connection;
    use diesel::pg::PgConnection;
    use diesel::prelude::*;

    use crate::data::models::{Match, User};

    // 用户模型CRUD测试
    #[test]
    fn test_user_crud() {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap();
        let mut conn =
            PgConnection::establish(&database_url).expect("cant establish database connection");

        conn.test_transaction(|conn| -> Result<(), Box<dyn std::error::Error>> {
            // TODO: 实现用户创建/读取/更新/删除测试
            Ok(())
        });
    }

    // 比赛模型验证测试
    #[test]
    fn test_match_validation() {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap();
        let mut conn =
            PgConnection::establish(&database_url).expect("cant establish database connection");

        conn.test_transaction(|conn| -> Result<(), Box<dyn std::error::Error>> {
            // TODO: 实现比赛模型验证逻辑测试
            Ok(())
        });
    }
}
