//! Core audio engine: oscillator, ADSR, mixer, offline rendering.


pub mod wav;

/// Oscillator waveform type.
#[derive(Debug, Clone, Copy)]
pub enum Waveform {
    Sine,
    Square,
    Sawtooth,
    Triangle,
    Noise,
}

/// Simple oscillator.
pub struct Oscillator {
    sample_rate: f32,
    frequency: f32,
    phase: f32,
    waveform: Waveform,
}

impl Oscillator {
    pub fn new(sample_rate: f32, frequency: f32, waveform: Waveform) -> Self {
        Self {
            sample_rate,
            frequency,
            phase: 0.0,
            waveform,
        }
    }

    pub fn set_frequency(&mut self, freq: f32) {
        self.frequency = freq;
    }

    pub fn next_sample(&mut self) -> f32 {
        let sample = match self.waveform {
            Waveform::Sine => (2.0 * std::f32::consts::PI * self.phase).sin(),
            Waveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            },
            Waveform::Sawtooth => 2.0 * self.phase - 1.0,
            Waveform::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    -4.0 * self.phase + 3.0
                }
            },
            Waveform::Noise => rand::random::<f32>() * 2.0 - 1.0,
        };
        self.phase = (self.phase + self.frequency / self.sample_rate) % 1.0;
        sample
    }
}

/// ADSR envelope.
pub struct Adsr {
    sample_rate: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    state: EnvelopeState,
    /// Time spent in the current state (in seconds).
    state_time: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
    Finished,
}

impl Adsr {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            attack: 0.01,
            decay: 0.01,
            sustain: 0.8,
            release: 0.1,
            state: EnvelopeState::Idle,
            state_time: 0.0,
        }
    }

    pub fn set_param(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack = attack;
        self.decay = decay;
        self.sustain = sustain;
        self.release = release;
    }

    pub fn trigger_on(&mut self) {
        self.state = EnvelopeState::Attack;
        self.state_time = 0.0;
    }

    pub fn trigger_off(&mut self) {
        if self.state != EnvelopeState::Finished {
            self.state = EnvelopeState::Release;
            self.state_time = 0.0;
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;
        self.state_time += dt;

        let output = match self.state {
            EnvelopeState::Idle => 0.0,
            EnvelopeState::Attack => {
                if self.state_time >= self.attack {
                    self.state = EnvelopeState::Decay;
                    self.state_time = 0.0;
                    1.0
                } else {
                    self.state_time / self.attack
                }
            }
            EnvelopeState::Decay => {
                if self.state_time >= self.decay {
                    self.state = EnvelopeState::Sustain;
                    self.state_time = 0.0;
                    self.sustain
                } else {
                    1.0 + (self.sustain - 1.0) * (self.state_time / self.decay)
                }
            }
            EnvelopeState::Sustain => self.sustain,
            EnvelopeState::Release => {
                if self.state_time >= self.release {
                    self.state = EnvelopeState::Finished;
                    0.0
                } else {
                    self.sustain * (1.0 - self.state_time / self.release)
                }
            }
            EnvelopeState::Finished => 0.0,
        };
        output
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, EnvelopeState::Finished)
    }
}

pub trait AudioProcessor {
    fn process(&mut self, output: &mut [f32; 2]);
    fn is_active(&self) -> bool;
}

/// Audio mixer that mixes multiple channels into a stereo output.
pub struct Mixer {
    channels: Vec<Box<dyn AudioProcessor>>,
    master_volume: f32,
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            master_volume: 1.0,
        }
    }

    pub fn add_channel(&mut self, channel: Box<dyn AudioProcessor>) {
        self.channels.push(channel);
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    pub fn process(&mut self, output: &mut [f32; 2]) {
        output[0] = 0.0;
        output[1] = 0.0;
        for channel in &mut self.channels {
            if channel.is_active() {
                channel.process(output);
            }
        }
        output[0] *= self.master_volume;
        output[1] *= self.master_volume;
    }
}

/// Simple audio processor that combines an oscillator and an ADSR envelope.
pub struct Voice {
    oscillator: Oscillator,
    envelope: Adsr,
    active: bool,
    note_on: bool,
}

impl Voice {
    pub fn new(sample_rate: f32, frequency: f32, waveform: Waveform) -> Self {
        Self {
            oscillator: Oscillator::new(sample_rate, frequency, waveform),
            envelope: Adsr::new(sample_rate),
            active: false,
            note_on: false,
        }
    }

    pub fn note_on(&mut self) {
        self.envelope.trigger_on();
        self.note_on = true;
        self.active = true;
    }

    pub fn note_off(&mut self) {
        self.envelope.trigger_off();
        self.note_on = false;
    }

    pub fn set_frequency(&mut self, freq: f32) {
        self.oscillator.set_frequency(freq);
    }
}

impl AudioProcessor for Voice {
    fn process(&mut self, output: &mut [f32; 2]) {
        if !self.is_active() {
            return;
        }
        let sample = self.oscillator.next_sample() * self.envelope.next_sample();
        output[0] += sample;
        output[1] += sample;
        if !self.is_active() {
            self.active = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active && !self.envelope.is_finished()
    }
}

/// Offline renderer that renders audio to a buffer.
pub struct OfflineRenderer {
    sample_rate: u32,
    channels: u16,
    mixer: Mixer,
}

impl OfflineRenderer {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            mixer: Mixer::new(),
        }
    }

    pub fn add_voice(&mut self, voice: Box<dyn AudioProcessor>) {
        self.mixer.add_channel(voice);
    }

    pub fn render(&mut self, duration_sec: f32) -> Vec<f32> {
        let num_samples = (duration_sec * self.sample_rate as f32) as usize * self.channels as usize;
        let mut buffer = vec![0.0f32; num_samples];
        let mut frame = 0;
        while frame < (duration_sec * self.sample_rate as f32) as usize {
            let mut stereo = [0.0f32; 2];
            self.mixer.process(&mut stereo);
            for ch in 0..self.channels {
                let idx = frame * self.channels as usize + ch as usize;
                buffer[idx] = stereo[ch as usize];
            }
            frame += 1;
        }
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oscillator_sine() {
        let mut osc = Oscillator::new(44100.0, 440.0, Waveform::Sine);
        let sample = osc.next_sample();
        assert!((sample - 0.0).abs() < 1e-5); // phase starts at 0, sin(0) = 0
    }

    #[test]
    fn test_adsr_envelope() {
        let mut env = Adsr::new(44100.0);
        env.set_param(0.01, 0.01, 0.8, 0.1);
        env.trigger_on();
        // Attack phase
        let mut last_sample = 0.0;
        for _ in 0..((44100 as f32 * 0.01) as usize) {
            let sample = env.next_sample();
            last_sample = sample;
            assert!(sample >= 0.0 && sample <= 1.0);
        }
        assert!((last_sample - 1.0).abs() < 1e-3, "Attack peak should reach ~1.0");
        // Decay phase
        for _ in 0..((44100 as f32 * 0.01) as usize) {
            let sample = env.next_sample();
            assert!(sample >= 0.8 && sample <= 1.0);
        }
        // Sustain phase
        for _ in 0..((44100 as f32 * 0.1) as usize) {
            let sample = env.next_sample();
            assert!((sample - 0.8).abs() < 1e-5);
        }
        env.trigger_off();
        // Release phase
        for _ in 0..(((44100 as f32 * 0.1) as usize) + 1) {
            let sample = env.next_sample();
            assert!(sample >= 0.0 && sample <= 0.8);
        }
        assert!(env.is_finished());
    }

    #[test]
    fn test_offline_render_to_wav() {
        use crate::wav::{writer, WavSpec, SampleFormat};
        let mut renderer = OfflineRenderer::new(44100, 2);
        // Create a simple voice
        let mut voice = Voice::new(44100.0, 440.0, Waveform::Sine);
        voice.note_on();
        renderer.add_voice(Box::new(voice));

        // Render 1 second
        let buffer = renderer.render(1.0);
        // Write to WAV file using our own wav writer
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::F32,
        };
        writer("test_output.wav", &spec, &buffer).expect("Failed to write WAV file");
        // Check file exists
        assert!(std::path::Path::new("test_output.wav").exists());
        // Clean up
        let _ = std::fs::remove_file("test_output.wav");
    }
}
