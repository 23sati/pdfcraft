//! Coarse wheel acceleration for the document viewport.
//!
//! Some platforms report a clicky wheel as one line event per notch. Applied
//! verbatim each notch moves a fixed distance, so a fast spin feels slow. This
//! keeps a small constant-space state machine over arrival times and returns a
//! bounded multiplier: 1.0 for isolated notches, growing while notches arrive
//! quickly in the same direction.

use egui::Vec2;

/// Idle time after which a burst is forgotten, in seconds.
const RESET_GAP_SECS: f64 = 0.3;
/// Notch spacing at or below which input counts as a continuous spin, in seconds.
const FAST_GAP_SECS: f64 = 0.08;
/// Multiplier gained per fast consecutive notch.
const BOOST_PER_TICK: f32 = 0.4;
/// Upper bound for the multiplier. Reachable: 1.0 + 13 * 0.4 = 6.2, capped.
const MAX_BOOST: f32 = 6.0;
/// Ticks that affect the output.
const MAX_TICKS: u32 = 13;
/// Extra ticks retained beyond the ceiling. They absorb a few slow events
/// before the visible multiplier starts to fall.
const HYSTERESIS_TICKS: u32 = 4;
/// Maximum scroll applied from one frame of coarse input, in points per axis.
/// Each event is clamped first; this caps the sum.
pub(crate) const MAX_FRAME_POINTS: f32 = 8192.0;
/// Per-event clamp, in native units (lines or pages). Keeps one hostile event
/// bounded before points conversion.
pub(crate) const MAX_EVENT_UNITS: f32 = 10.0;

/// Arrival-rate state for line and page wheel events. Constant space: no
/// per-event allocation regardless of input rate.
#[derive(Debug, Default)]
pub(crate) struct WheelBoost {
    last: f64,
    burst: u32,
    dir: i8,
}

impl WheelBoost {
    /// Returns the multiplier for one bucket of motion carrying `motion`.
    ///
    /// `motion` is signed in native units; only its sign matters. Zero means
    /// no motion. Non-finite time returns 1.0 and leaves state unchanged;
    /// backwards time resets the burst and returns 1.0.
    pub(crate) fn multiplier(&mut self, now: f64, motion: f32) -> f32 {
        if !now.is_finite() || !motion.is_finite() || motion == 0.0 {
            return 1.0;
        }
        // Exact: signum of finite non-zero is always exactly -1.0 or 1.0.
        let dir = if motion > 0.0 { 1 } else { -1 };
        if !self.last.is_finite() {
            self.last = now;
            self.burst = 0;
            self.dir = dir;
            return 1.0;
        }
        let idle = now - self.last;
        if !idle.is_finite() || idle < 0.0 {
            self.last = now;
            self.burst = 0;
            self.dir = dir;
            return 1.0;
        }
        if idle > RESET_GAP_SECS || dir != self.dir {
            self.burst = 0;
        } else if idle <= FAST_GAP_SECS {
            self.burst = self.burst.saturating_add(1).min(MAX_TICKS + HYSTERESIS_TICKS);
        } else {
            // Same direction but unhurried. Decay so an old burst does not stick.
            self.burst = self.burst.saturating_sub(1);
        }
        self.last = now;
        self.dir = dir;
        let ticks = self.burst.min(MAX_TICKS);
        // Ticks are at most 13, exactly representable, so the result is finite.
        (1.0 + ticks as f32 * BOOST_PER_TICK).min(MAX_BOOST)
    }

    /// Drops burst state. Call after a programmatic scroll jump.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Caps one frame of accumulated coarse scroll, per axis. A non-finite axis
/// maps to zero; the other axis is preserved.
pub(crate) fn cap_frame(delta: Vec2) -> Vec2 {
    Vec2::new(
        if delta.x.is_finite() { delta.x.clamp(-MAX_FRAME_POINTS, MAX_FRAME_POINTS) } else { 0.0 },
        if delta.y.is_finite() { delta.y.clamp(-MAX_FRAME_POINTS, MAX_FRAME_POINTS) } else { 0.0 },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_and_slow_notches_stay_unboosted() {
        let mut b = WheelBoost::default();
        assert_eq!(b.multiplier(1.0, -1.0), 1.0);
        assert_eq!(b.multiplier(1.5, -1.0), 1.0);
        assert_eq!(b.multiplier(2.0, -1.0), 1.0);
    }

    #[test]
    fn fast_spin_accelerates_then_caps() {
        let mut b = WheelBoost::default();
        assert_eq!(b.multiplier(1.0, -1.0), 1.0);
        let mut last = 1.0;
        for (i, t) in (1..=20).map(|i| 1.0 + f64::from(i) * 0.02).enumerate() {
            last = b.multiplier(t, -1.0);
            if i < 2 {
                assert!(last >= 1.0, "early burst must not jump, got {last}");
            }
        }
        assert!(last > 2.0, "a sustained fast spin must accelerate, got {last}");
        assert!(last <= MAX_BOOST, "boost must be capped, got {last}");
        assert_eq!(last, MAX_BOOST, "a long spin should reach the ceiling");
        for i in 1..=20 {
            let m = b.multiplier(1.5 + f64::from(i) * 0.02, -1.0);
            assert!(m <= MAX_BOOST, "boost must stay capped, got {m}");
        }
    }

    #[test]
    fn reversal_and_idle_reset_the_burst() {
        let mut b = WheelBoost::default();
        b.multiplier(1.0, -1.0);
        for i in 1..=6 {
            b.multiplier(1.0 + f64::from(i) * 0.02, -1.0);
        }
        let boosted = b.multiplier(1.0 + 7.0 * 0.02, -1.0);
        assert!(boosted > 1.0, "precondition: burst is boosted, got {boosted}");
        assert_eq!(b.multiplier(1.0 + 8.0 * 0.02, 1.0), 1.0);
        for i in 9..=14 {
            b.multiplier(1.0 + f64::from(i) * 0.02, 1.0);
        }
        assert_eq!(b.multiplier(5.0, 1.0), 1.0);
    }

    #[test]
    fn invalid_input_never_boosts_or_corrupts() {
        let mut b = WheelBoost::default();
        for bad_now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(b.multiplier(bad_now, -1.0), 1.0);
        }
        for bad_motion in [0.0, -0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(b.multiplier(1.0, bad_motion), 1.0);
        }
        assert_eq!(b.multiplier(2.0, -1.0), 1.0);
        b.reset();
        assert_eq!(b.multiplier(3.0, -1.0), 1.0);
    }

    #[test]
    fn same_timestamp_events_count_as_a_burst() {
        let mut b = WheelBoost::default();
        assert_eq!(b.multiplier(1.0, -1.0), 1.0);
        for _ in 0..6 {
            b.multiplier(1.0, -1.0);
        }
        assert!(b.multiplier(1.0, -1.0) > 1.0);
    }

    #[test]
    fn fractional_notches_share_the_burst() {
        let mut b = WheelBoost::default();
        b.multiplier(1.0, -0.25);
        for i in 1..=8 {
            b.multiplier(1.0 + f64::from(i) * 0.02, -0.25);
        }
        assert!(b.multiplier(1.0 + 9.0 * 0.02, -0.25) > 1.0);
    }

    #[test]
    fn sleep_wake_resets_the_burst() {
        let mut b = WheelBoost::default();
        b.multiplier(1.0, -1.0);
        for i in 1..=6 {
            b.multiplier(1.0 + f64::from(i) * 0.02, -1.0);
        }
        assert_eq!(b.multiplier(3600.0, -1.0), 1.0);
    }

    #[test]
    fn backwards_time_resets_without_corrupting() {
        let mut b = WheelBoost::default();
        b.multiplier(1.0, -1.0);
        b.multiplier(1.02, -1.0);
        assert_eq!(b.multiplier(0.5, -1.0), 1.0);
        assert_eq!(b.multiplier(0.52, -1.0), 1.4);
    }

    #[test]
    fn unhurried_spin_decays() {
        let mut b = WheelBoost::default();
        b.multiplier(1.0, -1.0);
        for i in 1..=8 {
            b.multiplier(1.0 + f64::from(i) * 0.02, -1.0);
        }
        let peak = b.multiplier(1.0 + 9.0 * 0.02, -1.0);
        assert!(peak > 1.0);
        // Same direction but slower than the fast gap: each event sheds one tick.
        let mut prev = peak;
        for i in 10..=13 {
            let m = b.multiplier(1.0 + f64::from(i) * 0.15, -1.0);
            assert!(m <= prev, "decay must not grow: {m} > {prev}");
            prev = m;
        }
    }

    #[test]
    fn hysteresis_holds_the_ceiling_briefly() {
        let mut b = WheelBoost::default();
        let mut t = 1.0;
        b.multiplier(t, -1.0);
        for _ in 0..20 {
            t += 0.02;
            b.multiplier(t, -1.0);
        }
        assert_eq!(b.multiplier(t + 0.02, -1.0), MAX_BOOST);
        // A couple of unhurried events consume hysteresis before output falls.
        t += 0.15;
        assert_eq!(b.multiplier(t, -1.0), MAX_BOOST);
        t += 0.15;
        assert_eq!(b.multiplier(t, -1.0), MAX_BOOST);
    }

    #[test]
    fn frame_cap_bounds_each_axis() {
        use egui::vec2;
        assert_eq!(cap_frame(vec2(40.0, -40.0)), vec2(40.0, -40.0));
        assert_eq!(cap_frame(vec2(1e9, -1e9)), vec2(MAX_FRAME_POINTS, -MAX_FRAME_POINTS));
        assert_eq!(cap_frame(vec2(f32::NAN, 40.0)), vec2(0.0, 40.0));
        assert_eq!(cap_frame(vec2(40.0, f32::INFINITY)), vec2(40.0, 0.0));
    }
}
