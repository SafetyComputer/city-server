#[cfg(test)]
mod tests {

    use rand::Rng;

    use super::super::super::matchroom::*;

    fn create_test_user_id() -> i32 {
        let mut rng = rand::rng();
        rng.random()
    }

    fn create_both_test_user_id() -> (i32, i32) {
        loop {
            let blue_id = create_test_user_id();
            let green_id = create_test_user_id();
            if blue_id != green_id {
                break (blue_id as i32, green_id as i32);
            }
        }
    }

    fn create_test_room_id() -> u32 {
        let mut rng = rand::rng();
        rng.random()
    }

    #[test]
    fn test_matchroom_creation() {
        let (blue_id, green_id) = create_both_test_user_id();

        let room = MatchRoom::new(Some(blue_id), Some(green_id));

        assert!(matches!(*room.get_state(), MatchRoomState::Ready));

        assert!(room.contains(blue_id));
        assert!(room.contains(green_id));
    }

    #[test]
    fn test_matchroom_creation_partial() {
        let blue_id = create_test_user_id();

        let room = MatchRoom::new(Some(blue_id), None);

        let info = room.get_info(create_test_room_id());

        assert!(matches!(info.player_green, PlayerState::Matching));
        assert!(matches!(info.player_blue, PlayerState::Ready(id) if id == blue_id));

        assert!(matches!(*room.get_state(), MatchRoomState::Matching));
    }

    #[test]
    fn test_player_join_with_color() {
        let (blue_id, green_id) = create_both_test_user_id();
        let mut room = MatchRoom::new(None, None);

        // 玩家加入蓝色位置
        assert!(room.join_players(blue_id, Some(Color::Blue)));
        let info = room.get_info(create_test_room_id());
        assert!(matches!(info.player_blue, PlayerState::Ready(id) if id == blue_id));

        // 玩家加入绿色位置
        assert!(room.join_players(green_id, Some(Color::Green)));
        let info = room.get_info(create_test_room_id());
        assert!(matches!(info.player_green, PlayerState::Ready(id) if id == green_id));

        // 状态应该变为Ready
        assert!(matches!(*room.get_state(), MatchRoomState::Ready));
    }

    #[test]
    fn test_viewer_join() {
        let blue_id = create_test_user_id();
        let green_id = create_test_user_id();
        let viewer_id = create_test_user_id();
        let mut room = MatchRoom::new(Some(blue_id), Some(green_id));

        // 观众加入
        assert!(room.join_viewers(viewer_id));
        assert!(room.contains(viewer_id));

        // 重复加入应该返回false
        assert!(!room.join_viewers(viewer_id));
    }

    #[test]
    fn test_player_contains() {
        let blue_id = create_test_user_id();
        let green_id = create_test_user_id();
        let unknown_id = create_test_user_id();
        let room = MatchRoom::new(Some(blue_id), Some(green_id));

        assert!(room.is_player(blue_id));
        assert!(room.is_player(green_id));
        assert!(!room.is_player(unknown_id));
    }

    #[test]
    fn test_match_info_creation() {
        let blue_id = create_test_user_id();
        let green_id = create_test_user_id();
        let room = MatchRoom::new(Some(blue_id), Some(green_id));

        let room_id = create_test_room_id();
        let info = room.get_info(room_id);

        assert_eq!(info.room, room_id);
        assert_eq!(info.player_blue.get_uuid(), Some(blue_id));
        assert_eq!(info.player_green.get_uuid(), Some(green_id));
        assert!(info.viewers.contains(&blue_id));
        assert!(info.viewers.contains(&green_id));
    }

    #[test]
    fn test_player_state_methods() {
        let player_id = create_test_user_id();

        // 测试Ready状态
        let ready_state = PlayerState::Ready(player_id);
        assert_eq!(ready_state.get_uuid(), Some(player_id));
        assert!(ready_state.is_ready());

        // 测试Left状态
        let left_state = PlayerState::Left(player_id);
        assert_eq!(left_state.get_uuid(), Some(player_id));
        assert!(!left_state.is_ready());

        // 测试WaitingForRematch状态
        let rematch_state = PlayerState::WaitingForRematch(player_id);
        assert_eq!(rematch_state.get_uuid(), Some(player_id));
        assert!(!rematch_state.is_ready());

        // 测试Matching状态
        let matching_state = PlayerState::Matching;
        assert_eq!(matching_state.get_uuid(), None);
        assert!(!matching_state.is_ready());
    }
}
