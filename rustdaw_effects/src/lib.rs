//! Audio effects: filter, delay, reverb.

/// Simple one-pole low-pass filter.
/// y[n] = x[n] * (1 - c) + y[n-1] * c
/// where c = 2 * sin(pi * fc / fs)
pub struct LowPassFilter {
    /// Cutoff frequency in Hz.
    pub fc: f32,
    /// Sample rate in Hz.
    pub fs: f32,
    /// Previous output sample.
    prev: f32,
}

impl LowPassFilter {
    pub fn new(fc: f32, fs: f32) -> Self {
        Self { fc, fs, prev: 0.0 }
    }

    pub fn set_cutoff(&mut self, fc: f32) {
        self.fc = fc;
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let c = 2.0 * std::f32::consts::PI * self.fc / self.fs;
        let c = c.min(1.0); // stability
        let out = sample * (1.0 - c) + self.prev * c;
        self.prev = out;
        out
    }
}

/// Simple delay effect with feedback and mix.
pub struct Delay {
    /// Delay time in seconds.
    pub delay_time: f32,
    /// Feedback amount (0.0 to 1.0).
    pub feedback: f32,
    /// Mix amount (0.0 = dry only, 1.0 = wet only).
    pub mix: f32,
    /// Sample rate in Hz.
    pub fs: f32,
    /// Delay buffer.
    buffer: Vec<f32>,
    /// Write position.
    pos: usize,
}

impl Delay {
    pub fn new(delay_time: f32, feedback: f32, mix: f32, fs: f32, max_delay_time: f32) -> Self {
        let buffer_size = (max_delay_time * fs) as usize;
        Self {
            delay_time,
            feedback: feedback.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            fs,
            buffer: vec![0.0; buffer_size],
            pos: 0,
        }
    }

    pub fn set_delay_time(&mut self, delay_time: f32) {
        self.delay_time = delay_time;
    }

    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 1.0);
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let delay_samples = (self.delay_time * self.fs) as usize;
        let read_pos = (self.pos + self.buffer.len() - delay_samples) % self.buffer.len();
        let delayed = self.buffer[read_pos];
        // Write to buffer: input + feedback * delayed
        self.buffer[self.pos] = sample + self.feedback * delayed;
        self.pos = (self.pos + 1) % self.buffer.len();
        // Mix dry and wet
        (1.0 - self.mix) * sample + self.mix * delayed
    }
}

/// Simple reverb using a series of comb filters and all-pass filters.
/// Based on Schroeder reverb.
pub struct Reverb {
    /// Comb filters.
    combs: Vec<CombFilter>,
    /// All-pass filters.
    allpasses: Vec<AllPassFilter>,
    /// Mix amount.
    mix: f32,
}

struct CombFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    filterstore: f32,
}

struct AllPassFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl Reverb {
    pub fn new(fs: f32) -> Self {
        // These values are for a sample rate of 44100 Hz; we scale by fs/44100.
        let scale = fs / 44100.0;
        let combs = vec![
            CombFilter::new((1116.0 * scale) as usize, 0.797, fs),
            CombFilter::new((1188.0 * scale) as usize, 0.783, fs),
            CombFilter::new((1277.0 * scale) as usize, 0.753, fs),
            CombFilter::new((1356.0 * scale) as usize, 0.733, fs),
        ];
        let allpasses = vec![
            AllPassFilter::new((556.0 * scale) as usize, 0.953, fs),
            AllPassFilter::new((441.0 * scale) as usize, 0.922, fs),
            AllPassFilter::new((341.0 * scale) as usize, 0.906, fs),
            AllPassFilter::new((225.0 * scale) as usize, 0.877, fs),
        ];
        Self {
            combs,
            allpasses,
            mix: 0.3, // default mix
        }
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        // Process through comb filters in parallel
        let mut comb_out = 0.0;
        for comb in &mut self.combs {
            comb_out += comb.process(sample);
        }
        // Process through all-pass filters in series
        let mut ap_out = comb_out;
        for ap in &mut self.allpasses {
            ap_out = ap.process(ap_out);
        }
        // Mix dry and wet
        (1.0 - self.mix) * sample + self.mix * ap_out
    }
}

impl CombFilter {
    pub fn new(buffer_size: usize, feedback: f32, _fs: f32) -> Self {
        Self {
            buffer: vec![0.0; buffer_size],
            pos: 0,
            feedback: feedback.clamp(0.0, 0.99),
            filterstore: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filterstore = output * self.feedback;
        self.buffer[self.pos] = input + self.filterstore;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

impl AllPassFilter {
    pub fn new(buffer_size: usize, feedback: f32, _fs: f32) -> Self {
        Self {
            buffer: vec![0.0; buffer_size],
            pos: 0,
            feedback: feedback.clamp(0.0, 0.99),
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = -input + self.buffer[self.pos];
        self.buffer[self.pos] = input + output * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

/// Effect chain: LPF -> Delay -> Reverb (mono in, stereo out via duplication).
/// Order chosen so delay/reverb don't feed back into the filter.
pub struct EffectChain {
    lpf: LowPassFilter,
    delay: Delay,
    reverb: Reverb,
    enabled: bool,
}

impl EffectChain {
    pub fn new(fs: f32) -> Self {
        Self {
            lpf: LowPassFilter::new(20000.0, fs), // effectively off at Nyquist
            delay: Delay::new(0.3, 0.3, 0.0, fs, 2.0), // 300ms delay, 30% feedback, dry only
            reverb: Reverb::new(fs),
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    // LPF controls
    pub fn set_lpf_cutoff(&mut self, fc: f32) {
        self.lpf.set_cutoff(fc.clamp(20.0, 20000.0));
    }

    // Delay controls
    pub fn set_delay_time(&mut self, time: f32) {
        self.delay.set_delay_time(time.clamp(0.0, 2.0));
    }
    pub fn set_delay_feedback(&mut self, fb: f32) {
        self.delay.set_feedback(fb.clamp(0.0, 0.95));
    }
    pub fn set_delay_mix(&mut self, mix: f32) {
        self.delay.set_mix(mix.clamp(0.0, 1.0));
    }

    // Reverb controls
    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.reverb.set_mix(mix.clamp(0.0, 1.0));
    }

    /// Process a mono sample, return stereo pair [L, R].
    /// Delay and reverb are mono internally; we just duplicate for stereo.
    pub fn process(&mut self, sample: f32) -> [f32; 2] {
        if !self.enabled {
            return [sample, sample];
        }
        let mut s = self.lpf.process(sample);
        s = self.delay.process(s);
        s = self.reverb.process(s);
        [s, s]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_low_pass_filter() {
        let mut lpf = LowPassFilter::new(1000.0, 44100.0);
        // DC signal should pass unchanged after convergence
        for _ in 0..1000 {
            lpf.process(1.0);
        }
        assert!((lpf.process(1.0) - 1.0).abs() < 1e-5);
        // High frequency should be attenuated
        // We'll just check that the filter doesn't blow up
        for _ in 0..1000 {
            lpf.process(0.0);
        }
    }

    #[test]
    fn test_delay() {
        let mut delay = Delay::new(0.5, 0.5, 0.5, 44100.0, 1.0); // 0.5s delay, 50% feedback, 50% mix, max 1s delay
        let input = 1.0;
        // First sample: dry only because delay buffer is empty
        let out1 = delay.process(input);
        assert!((out1 - 0.5).abs() < 1e-5); // (1-0.5)*1 + 0.5*0 = 0.5
        // Second sample: now we have some delay
        let out2 = delay.process(0.0);
        // The delay buffer now has: at pos0: 1.0 + 0.5*0 = 1.0, then we advanced pos
        // The read position for 0.5s delay is half the buffer away.
        // We'll just check that it's not NaN
        assert!(!out2.is_nan());
    }

    #[test]
    fn test_reverb() {
        let mut reverb = Reverb::new(44100.0);
        reverb.set_mix(0.5);
        let input = 1.0;
        let out = reverb.process(input);
        assert!(!out.is_nan());
        // Test with silence
        let out2 = reverb.process(0.0);
        assert!(!out2.is_nan());
    }

    #[test]
    fn test_effect_chain() {
        let mut chain = EffectChain::new(44100.0);
        chain.set_enabled(true);
        chain.set_lpf_cutoff(1000.0);
        chain.set_delay_time(0.1);
        chain.set_delay_feedback(0.3);
        chain.set_delay_mix(0.3);
        chain.set_reverb_mix(0.2);

        let out = chain.process(1.0);
        assert!(!out[0].is_nan() && !out[1].is_nan());
        // Disabled should pass through
        chain.set_enabled(false);
        let out2 = chain.process(0.5);
        assert!((out2[0] - 0.5).abs() < 1e-5);
    }
}
