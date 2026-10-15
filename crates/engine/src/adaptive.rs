/// Experimental admission controller. Existing leases always drain normally.
/// Each decision averages two one-second samples; unsuccessful trials back off.
pub(crate) struct Controller {
    ceiling: usize,
    limit: usize,
    samples: Vec<f64>,
    baseline: Option<f64>,
    trial: Option<f64>,
    cooldown: u8,
    regressions: u8,
}
impl Controller {
    pub(crate) fn new(ceiling: usize) -> Self {
        Self {
            ceiling: ceiling.clamp(1, 4),
            limit: 1,
            samples: Vec::with_capacity(2),
            baseline: None,
            trial: None,
            cooldown: 0,
            regressions: 0,
        }
    }
    pub(crate) fn limit(&self) -> usize {
        self.limit
    }
    pub(crate) fn throttle(&mut self) {
        self.limit = 1;
        self.samples.clear();
        self.baseline = None;
        self.trial = None;
        self.cooldown = 15;
        self.regressions = 0;
    }
    pub(crate) fn sample(&mut self, rate: f64) {
        if !rate.is_finite() || rate < 0.0 {
            return;
        }
        self.samples.push(rate);
        if self.samples.len() < 2 {
            return;
        }
        let rate = (self.samples[0] + self.samples[1]) / 2.0;
        self.samples.clear();
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return;
        }
        if let Some(base) = self.trial.take() {
            if rate < base * 1.10 {
                self.limit = self.limit.saturating_sub(1).max(1);
                self.baseline = Some(base);
                self.cooldown = 5;
            } else {
                self.baseline = Some(rate);
            }
            return;
        }
        if self.baseline.is_some_and(|base| rate < base * 0.8) {
            self.regressions += 1;
            if self.regressions >= 2 {
                self.limit = self.limit.saturating_sub(1).max(1);
                self.baseline = None;
                self.cooldown = 3;
                self.regressions = 0;
            }
            return;
        }
        self.regressions = 0;
        self.baseline = Some(rate);
        if rate > 0.0 && self.limit < self.ceiling {
            self.trial = Some(rate);
            self.limit += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waits_for_multiple_samples_and_never_exceeds_ceiling() {
        let mut controller = Controller::new(4);
        controller.sample(100.0);
        assert_eq!(controller.limit(), 1);
        controller.sample(100.0);
        assert_eq!(controller.limit(), 2);
        for _ in 0..100 {
            controller.sample(controller.limit() as f64 * 100.0);
            assert!((1..=4).contains(&controller.limit()));
        }
        let mut one = Controller::new(1);
        for _ in 0..20 {
            one.sample(100.0);
            assert_eq!(one.limit(), 1);
        }
    }
    #[test]
    fn rejects_non_improving_trials_and_holds_after_throttle() {
        let mut controller = Controller::new(4);
        for _ in 0..2 {
            controller.sample(100.0);
        }
        assert_eq!(controller.limit(), 2);
        for _ in 0..2 {
            controller.sample(100.0);
        }
        assert_eq!(controller.limit(), 1);
        controller.throttle();
        for _ in 0..30 {
            controller.sample(1000.0);
            assert_eq!(controller.limit(), 1);
        }
        controller.sample(1000.0);
        controller.sample(1000.0);
        assert_eq!(controller.limit(), 2);
    }
    #[test]
    fn sustained_regression_reduces_admission_without_canceling_leases() {
        let mut controller = Controller::new(2);
        for rate in [100.0, 100.0, 200.0, 200.0, 200.0, 200.0] {
            controller.sample(rate);
        }
        assert_eq!(controller.limit(), 2);
        for _ in 0..4 {
            controller.sample(80.0);
        }
        assert_eq!(controller.limit(), 1);
    }
}
