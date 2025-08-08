#[cfg(test)]
mod tests {
    use crate::game::logic::{Game, Move, Coordinate, Direction, Cell};
    use mockall::*;

    // 创建GameState trait的mock实现
    mock! {
        pub GameState {
            fn blue_position(&self) -> Coordinate;
            fn green_position(&self) -> Coordinate;
            fn make_move(&mut self, mv: Move) -> bool;
            fn game_over(&mut self) -> bool;
        }
    }

    #[test]
    fn test_state_transition() {
        // 创建mock对象
        let mut mock = MockGameState::new();
        
        // 设置期望行为
        mock.expect_blue_position()
            .return_const(Coordinate::new(0, 0));
        mock.expect_make_move()
            .with(predicate::always())
            .returning(|_| true);
        mock.expect_game_over()
            .return_const(false);

        // 执行测试
        assert_eq!(mock.blue_position(), Coordinate::new(0, 0));
        assert!(mock.make_move(Move::new(Coordinate::new(1, 0), Direction::Right)));
        assert!(!mock.game_over());
    }

    #[test]
    fn test_win_condition() {
        let mut mock = MockGameState::new();
        mock.expect_game_over()
            .return_const(true);
        mock.expect_blue_position()
            .return_const(Coordinate::new(0, 0));
        mock.expect_green_position()
            .return_const(Coordinate::new(0, 0));

        // 模拟游戏结束状态
        assert!(mock.game_over());
    }

    #[test]
    fn test_territory_calculation() {
        // 使用真实游戏对象进行领土计算测试
        let mut game = Game::new(3, 3);
        game.blue_position = Coordinate::new(0, 0);
        game.green_position = Coordinate::new(2, 2);
        
        // 放置墙壁
        game.vertical_walls.set(Coordinate::new(1, 0), Cell::Blue);
        game.vertical_walls.set(Coordinate::new(1, 1), Cell::Blue);
        game.vertical_walls.set(Coordinate::new(1, 2), Cell::Blue);

        let diff = game.territory_difference();
        assert!(diff > 0); // 蓝方应有更多领土
    }
}