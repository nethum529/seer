use std::time::Instant;

const FREQUENCY: f32 = 40.;

pub(super) struct Reveal {
    from: f32,
    to: f32,
    velocity: f32,
    start: Instant,
}

impl Reveal {
    pub(super) fn new() -> Self {
        Self {
            from: 0.,
            to: 0.,
            velocity: 0.,
            start: Instant::now(),
        }
    }

    pub(super) fn retarget(&mut self, shown: bool, instant: bool) {
        let now = Instant::now();
        let (value, velocity) = self.sample(now);
        self.to = if shown { 1. } else { 0. };
        self.from = if instant { self.to } else { value };
        self.velocity = if instant { 0. } else { velocity };
        self.start = now;
    }

    pub(super) fn value(&self) -> f32 {
        self.sample(Instant::now()).0
    }

    pub(super) fn animating(&self) -> bool {
        let (value, velocity) = self.sample(Instant::now());
        value != self.to || velocity != 0.
    }

    pub(super) fn closing(&self) -> bool {
        self.to == 0.
    }

    fn sample(&self, now: Instant) -> (f32, f32) {
        let elapsed = now.duration_since(self.start).as_secs_f32();
        let displacement = self.from - self.to;
        let coefficient = self.velocity + FREQUENCY * displacement;
        let decay = (-FREQUENCY * elapsed).exp();
        let remaining = (displacement + coefficient * elapsed) * decay;
        let velocity = (self.velocity - FREQUENCY * coefficient * elapsed) * decay;
        if remaining.abs() < 0.001 && velocity.abs() < 0.01 {
            (self.to, 0.)
        } else {
            (self.to + remaining, velocity)
        }
    }
}
