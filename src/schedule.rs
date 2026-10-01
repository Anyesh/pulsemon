use std::time::{Duration, Instant};

pub fn tick_due(now: Instant, last_tick: Instant, rate: Duration) -> bool {
    now >= last_tick + rate
}

/// How long the main loop may block waiting for input before something time-based
/// (the next tick or the `other` deadline, e.g. a status message expiring) needs it.
pub fn wait_for(
    now: Instant,
    last_tick: Instant,
    rate: Duration,
    other: Option<Instant>,
) -> Duration {
    let tick = last_tick + rate;
    let deadline = other.map_or(tick, |o| o.min(tick));
    deadline.saturating_duration_since(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn waits_until_next_tick() {
        let t0 = Instant::now();
        assert_eq!(wait_for(t0 + MS(100), t0, MS(1000), None), MS(900));
    }

    #[test]
    fn rate_change_applies_to_pending_tick() {
        let t0 = Instant::now();
        assert_eq!(wait_for(t0 + MS(100), t0, MS(250), None), MS(150));
        assert!(!tick_due(t0 + MS(100), t0, MS(250)));
        assert!(tick_due(t0 + MS(300), t0, MS(250)));
    }

    #[test]
    fn earlier_other_deadline_wins() {
        let t0 = Instant::now();
        assert_eq!(wait_for(t0, t0, MS(1000), Some(t0 + MS(40))), MS(40));
        assert_eq!(wait_for(t0, t0, MS(1000), Some(t0 + MS(4000))), MS(1000));
    }

    #[test]
    fn overdue_tick_does_not_block() {
        let t0 = Instant::now();
        assert_eq!(wait_for(t0 + MS(5000), t0, MS(1000), None), Duration::ZERO);
        assert!(tick_due(t0 + MS(1000), t0, MS(1000)));
    }

    #[test]
    fn steady_input_does_not_delay_tick() {
        let t0 = Instant::now();
        let mut now = t0;
        let mut ticks = 0;
        let mut last = t0;
        while now < t0 + MS(3000) {
            now += MS(10);
            if tick_due(now, last, MS(1000)) {
                ticks += 1;
                last = now;
            }
        }
        assert_eq!(ticks, 3);
    }
}
