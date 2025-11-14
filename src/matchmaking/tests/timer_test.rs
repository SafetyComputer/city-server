#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;

    use super::super::super::timer::*;

    #[test]
    fn test_countdown_flow() {
        let total_duration = Duration::from_millis(200);
        let mut timer = CountdownTimer::new(total_duration);

        // 初始状态
        assert_eq!(timer.remaining(), total_duration);
        assert!(timer.is_paused());

        // 第一次运行
        timer.start();
        sleep(Duration::from_millis(50));
        assert!(timer.remaining().as_millis().abs_diff(150) <= 10);

        // 第一次暂停
        timer.pause();
        sleep(Duration::from_millis(50)); // 暂停期间时间不变
        assert!(timer.remaining().as_millis().abs_diff(150) <= 10);

        // 第二次运行
        timer.start();
        sleep(Duration::from_millis(100));
        assert!(timer.remaining().as_millis().abs_diff(50) <= 10);
        assert!(!timer.is_expired());

        // 第二次暂停
        timer.pause();
        sleep(Duration::from_millis(50)); // 暂停期间时间不变
        assert!(timer.remaining().as_millis().abs_diff(50) <= 10);

        // 第三次运行
        timer.start();
        sleep(Duration::from_millis(60));
        assert!(timer.is_expired());
        assert_eq!(timer.remaining(), Duration::default());

        // 重置测试
        timer.reset();
        assert_eq!(timer.remaining(), total_duration);
        assert!(timer.is_paused());
    }

    #[test]
    fn test_multiple_starts_pauses() {
        let total_duration = Duration::from_millis(300);
        let mut timer = CountdownTimer::new(total_duration);

        timer.start();
        sleep(Duration::from_millis(100));
        timer.pause();

        // 暂停50ms
        sleep(Duration::from_millis(50));
        timer.start();
        sleep(Duration::from_millis(100));
        timer.pause();

        // 再暂停50ms
        sleep(Duration::from_millis(50));
        timer.start();
        sleep(Duration::from_millis(100));

        // 验证：总运行时间应为100+100+100=300ms
        assert!(timer.is_expired());
        assert_eq!(timer.remaining(), Duration::default());
    }
    #[test]
    fn test_timer_initial_state() {
        let total_duration = Duration::from_secs(60);
        let timer = CountdownTimer::new(total_duration);
        
        assert_eq!(timer.remaining(), total_duration);
        assert!(timer.is_paused());
        assert!(!timer.is_expired());
    }

    #[test]
    fn test_timer_expired_state() {
        let total_duration = Duration::from_millis(10);
        let mut timer = CountdownTimer::new(total_duration);
        
        timer.start();
        sleep(Duration::from_millis(20));
        
        assert!(timer.is_expired());
        assert_eq!(timer.remaining(), Duration::default());
        assert!(!timer.is_paused());
    }

    #[test]
    fn test_timer_multiple_pause_resume() {
        let total_duration = Duration::from_millis(200);
        let mut timer = CountdownTimer::new(total_duration);
        
        // 多次暂停和恢复不应该影响计时
        timer.start();
        sleep(Duration::from_millis(50));
        timer.pause();
        timer.pause(); // 重复暂停
        timer.start(); // 恢复
        timer.start(); // 重复开始
        sleep(Duration::from_millis(50));
        
        // 剩余时间应该在100ms左右
        assert!(timer.remaining().as_millis().abs_diff(100) <= 10);
    }

    #[test]
    fn test_timer_reset_after_start() {
        let total_duration = Duration::from_millis(200);
        let mut timer = CountdownTimer::new(total_duration);
        
        timer.start();
        sleep(Duration::from_millis(50));
        timer.reset();
        
        assert_eq!(timer.remaining(), total_duration);
        assert!(timer.is_paused());
        assert!(!timer.is_expired());
    }

    #[test]
    fn test_timer_precision() {
        let total_duration = Duration::from_millis(100);
        let mut timer = CountdownTimer::new(total_duration);
        
        timer.start();
        sleep(Duration::from_millis(25));
        
        let remaining = timer.remaining();
        // 允许5ms的误差
        assert!(remaining.as_millis().abs_diff(75) <= 5);
    }

    #[test]
    fn test_timer_zero_duration() {
        let total_duration = Duration::from_millis(0);
        let mut timer = CountdownTimer::new(total_duration);
        
        assert!(timer.is_expired());
        assert_eq!(timer.remaining(), Duration::default());
        
        timer.start();
        assert!(timer.is_expired());
        
        timer.pause();
        assert!(timer.is_expired());
    }
}
