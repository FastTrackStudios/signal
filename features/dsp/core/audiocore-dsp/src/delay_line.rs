//! Circular buffer delay line with fractional-sample interpolation.

// r[impl dsp.delay.circular-buffer]
pub struct DelayLine {
    buffer: Box<[f64]>,
    write_pos: usize,
}

impl DelayLine {
    pub fn new(max_length: usize) -> Self {
        Self {
            buffer: vec![0.0; max_length].into_boxed_slice(),
            write_pos: 0,
        }
    }

    pub fn write(&mut self, sample: f64) {
        self.buffer[self.write_pos] = sample;
        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }
    }

    // r[impl dsp.delay.integer-read]
    pub fn read(&self, delay_samples: usize) -> f64 {
        let len = self.buffer.len();
        let pos = (self.write_pos + len - delay_samples) % len;
        self.buffer[pos]
    }

    // r[impl dsp.delay.fractional-read]
    pub fn read_linear(&self, delay_samples: f64) -> f64 {
        let int_delay = delay_samples as usize;
        let frac = delay_samples - int_delay as f64;
        let a = self.read(int_delay);
        let b = self.read(int_delay + 1);
        a + frac * (b - a)
    }

    // r[impl dsp.delay.cubic-read]
    /// Catmull-Rom cubic interpolation for higher quality fractional reads.
    pub fn read_cubic(&self, delay_samples: f64) -> f64 {
        let int_delay = delay_samples as usize;
        let frac = delay_samples - int_delay as f64;

        let y0 = self.read(int_delay.saturating_sub(1).max(1));
        let y1 = self.read(int_delay);
        let y2 = self.read((int_delay + 1).min(self.buffer.len() - 1));
        let y3 = self.read((int_delay + 2).min(self.buffer.len() - 1));

        // Catmull-Rom spline
        let a0 = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
        let a1 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let a2 = -0.5 * y0 + 0.5 * y2;
        let a3 = y1;

        ((a0 * frac + a1) * frac + a2) * frac + a3
    }

    // r[impl dsp.delay.clear]
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
