use std::time::{Duration, Instant};

pub(crate) struct Motion {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    curve: [f32; 4],
}

impl Motion {
    pub(crate) fn open(keyboard: bool, duration: u64) -> Self {
        Self {
            curve: [0.23, 1., 0.32, 1.],
            from: if keyboard { 1. } else { 0. },
            to: 1.,
            start: Instant::now(),
            duration: Duration::from_millis(if keyboard { 0 } else { duration }),
        }
    }

    pub(crate) fn feedback() -> Self {
        let mut motion = Self::open(true, 0);
        motion.curve = [0.2, 0., 0., 1.];
        motion.close(true, 0);
        motion
    }

    pub(crate) fn close(&mut self, keyboard: bool, duration: u64) {
        self.retarget(0., keyboard, duration);
    }

    pub(crate) fn reopen(&mut self, keyboard: bool, duration: u64) {
        self.retarget(1., keyboard, duration);
    }

    fn retarget(&mut self, to: f32, keyboard: bool, duration: u64) {
        self.from = self.value();
        self.to = to;
        self.start = Instant::now();
        self.duration = Duration::from_millis(if keyboard { 0 } else { duration });
    }

    pub(crate) fn value(&self) -> f32 {
        if !self.animating() {
            return self.to;
        }
        let delta = self.start.elapsed().as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * ease_out(delta, self.curve)
    }

    pub(crate) fn animating(&self) -> bool {
        self.start.elapsed() < self.duration
    }

    pub(crate) fn closing(&self) -> bool {
        self.to == 0.
    }

    pub(crate) fn gone(&self) -> bool {
        self.closing() && !self.animating()
    }
}

fn ease_out(delta: f32, curve: [f32; 4]) -> f32 {
    let (mut low, mut high) = (0., 1.);
    for _ in 0..16 {
        let t = (low + high) / 2.;
        let x =
            3. * (1. - t) * (1. - t) * t * curve[0] + 3. * (1. - t) * t * t * curve[2] + t * t * t;
        if x < delta {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) / 2.;
    3. * (1. - t) * (1. - t) * t * curve[1] + 3. * (1. - t) * t * t * curve[3] + t * t * t
}
