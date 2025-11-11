use std::time::{Duration, Instant};

pub struct CountdownTimer {
    total_duration: Duration,
    start_time: Option<Instant>,
    accumulated: Duration,
    is_paused: bool,
}

impl CountdownTimer {
    pub fn new(duration: Duration) -> Self {
        Self {
            total_duration: duration,
            start_time: None,
            accumulated: Duration::default(),
            is_paused: true,
        }
    }

    pub fn start(&mut self) {
        if self.is_paused {
            self.start_time = Some(Instant::now());
            self.is_paused = false;
        }
    }

    pub fn pause(&mut self) {
        if !self.is_paused {
            if let Some(start) = self.start_time {
                self.accumulated += start.elapsed();
            }
            self.start_time = None;
            self.is_paused = true;
        }
    }

    pub fn reset(&mut self) {
        self.start_time = None;
        self.accumulated = Duration::default();
        self.is_paused = true;
    }

    pub fn remaining(&self) -> Duration {
        let elapsed = self.calculate_elapsed();
        if elapsed >= self.total_duration {
            Duration::default()
        } else {
            self.total_duration - elapsed
        }
    }

    pub fn is_expired(&self) -> bool {
        self.remaining() == Duration::default()
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    fn calculate_elapsed(&self) -> Duration {
        let mut elapsed = self.accumulated;

        if let Some(start) = self.start_time {
            elapsed += start.elapsed();
        }

        elapsed
    }
}
