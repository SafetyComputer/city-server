use std::time::{Duration, Instant};

/// 可暂停的倒计时器
///
/// 时间计算原理:
/// 1. 总运行时间 = 所有运行时段的总和
/// 2. 暂停期间不计入运行时间
/// 3. accumulated字段记录历史运行时间总和
/// 4. 当前运行时段 = 当前时间 - 最后一次启动时间
pub struct CountdownTimer {
    total_duration: Duration,
    start_time: Option<Instant>, // 当前运行的开始时间
    accumulated: Duration,       // 历史运行时间总和
    is_paused: bool,
}

impl CountdownTimer {
    /// 创建新倒计时器
    ///
    /// # 参数
    /// - `duration`: 倒计时的总时长
    pub fn new(duration: Duration) -> Self {
        Self {
            total_duration: duration,
            start_time: None,
            accumulated: Duration::default(),
            is_paused: true, // 初始为暂停状态
        }
    }

    /// 启动/继续倒计时
    pub fn start(&mut self) {
        if self.is_paused {
            // 记录当前时间作为运行开始点
            self.start_time = Some(Instant::now());
            self.is_paused = false;
        }
    }

    /// 暂停倒计时
    pub fn pause(&mut self) {
        if !self.is_paused {
            if let Some(start) = self.start_time {
                // 累加当前运行时段
                self.accumulated += start.elapsed();
            }
            self.start_time = None;
            self.is_paused = true;
        }
    }

    /// 重置倒计时（保持原有时长）
    pub fn reset(&mut self) {
        self.start_time = None;
        self.accumulated = Duration::default();
        self.is_paused = true;
    }

    /// 获取剩余时间
    pub fn remaining(&self) -> Duration {
        let elapsed = self.calculate_elapsed();
        if elapsed >= self.total_duration {
            Duration::default()
        } else {
            self.total_duration - elapsed
        }
    }

    /// 检查是否已超时
    pub fn is_expired(&self) -> bool {
        self.remaining() == Duration::default()
    }

    /// 检查是否处于暂停状态
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// 计算总运行时间（排除暂停时间）
    fn calculate_elapsed(&self) -> Duration {
        let mut elapsed = self.accumulated;

        // 加上当前运行时段（如果正在运行）
        if let Some(start) = self.start_time {
            elapsed += start.elapsed();
        }

        elapsed
    }
}
