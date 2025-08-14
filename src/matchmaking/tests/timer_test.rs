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
}
