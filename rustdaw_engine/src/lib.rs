//! Core audio engine: oscillators, ADSR, mixer, offline rendering.

use ringbuf::RingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Oscillator waveform types.
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
                if (2.0 * std::f32::consts::PI * self.phase).sin() >= 0.0 { 1.0 } else { -1.0 }
            }
            Waveform::Sawtooth => 2.0 * self.phase - 1.0,
            Waveform::Triangle => {
                let s = (2.0 * std::f32::consts::PI * self.phase).sin();
                2.0 * s.asin() / std::f32::consts::PI
            }
            Waveform::Noise => rand::random::<f32>() * 2.0 - 1.0,
        };
        self.phase += self.frequency / self.sample_rate;
        if self.phase > 1.0 {
            self.phase -= 1.0;
        }
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

#[derive(Debug, Clone, Copy)]
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

    pub fn set_params(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
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

/// Audio mixer that mixes multiple channels into a stereo output.
pub struct Mixer {
    channels: Vec<Box<dyn AudioProcessor>>,
    master_volume: f32,
}

trait AudioProcessor {
    fn process(&mut self, output: &mut [f32; 2]);
    fn is_active(&self) -> bool;
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
    adsr: Adsr,
    active: bool,
    note_on: bool,
}

impl Voice {
    pub fn new(sample_rate: f32, frequency: f32, waveform: Waveform) -> Self {
        Self {
            oscillator: Oscillator::new(sample_rate, frequency, waveform),
            adsr: Adsr::new(sample_rate),
            active: false,
            note_on: false,
        }
    }

    pub fn note_on(&mut self) {
        self.adsr.trigger_on();
        self.note_on = true;
        self.active = true;
    }

    pub fn note_off(&mut self) {
        self.adsr.trigger_off();
        self.note_on = false;
    }

    pub fn is_active(&self) -> bool {
        self.active && !self.adsr.is_finished()
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
        let sample = self.oscillator.next_sample() * self.adsr.next_sample();
        output[0] += sample;
        output[1] += sample;
        if !self.is_active() {
            self.active = false;
        }
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

    pub fn render(&mut self, duration_seconds: f32) -> Vec<f32> {
        let num_samples = (duration_seconds * self.sample_rate as f32) as usize;
        let mut buffer = vec![0.0f32; num_samples * self.channels as usize];
        let mut frame = 0;
        while frame < num_samples {
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
    use hound::{WavSpec, WavWriter};

    #[test]
    fn test_oscillator_sine() {
        let mut osc = Oscillator::new(44100.0, 440.0, Waveform::Sine);
        let sample = osc.next_sample();
        assert!((sample - 0.0).abs() < 1e-5); // phase starts at 0, sin(0) = 0
    }

    #[test]
    fn test_adsr() {
        let mut adsr = Adsr::new(44100.0);
        adsr.set_params(0.01, 0.01, 0.8, 0.1);
        adsr.trigger_on();
        // Attack phase
        for _ in 0..(44100 * 0.01) as usize {
            adsr.next_sample();
        }
        assert!((adsr.next_sample() - 1.0).abs() < 1e-5);
        // Decay phase
        for _ in 0..(44100 * 0.01) as usize {
            adsr.next_sample();
        }
        assert!((adsr.next_sample() - 0.8).abs() < 1e-5);
        // Release
        adsr.trigger_off();
        for _ in 0..(44100 * 0.1) as usize {
            adsr.next_sample();
        }
        assert!(adsr.next_sample() < 0.01);
    }

    #[test]
    fn test_offline_render_to_wav() {
        let mut renderer = OfflineRenderer::new(44100, 2);
        // Create a simple voice
        let mut voice = Voice::new(44100.0, 440.0, Waveform::Sine);
        voice.note_on();
        renderer.add_voice(Box::new(voice));

        // Render 1 second
        let buffer = renderer.render(1.0);
        // Write to WAV file
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create("test_output.wav", spec).unwrap();
        for sample in buffer {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        // Check file exists
        assert!(std::path::Path::new("test_output.wav").exists());
        // Clean up
        let _ = std::fs::remove_file("test_output.wav");
    }
}
