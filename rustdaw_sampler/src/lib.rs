//! Sampler: load .wav files and play with pitch shifting.

use hound::{WavReader, WavSpec};
use rustdaw_engine::{Oscillator, Waveform};
use std::path::Path;

/// Simple sampler that holds audio data and can play it back at different pitches.
pub struct Sampler {
    /// Audio samples (mono, normalized to [-1.0, 1.0]).
    pub samples: Vec<f32>,
    /// Sample rate of the loaded audio.
    pub sample_rate: u32,
    /// Current playback position (in samples).
    position: f32,
    /// Pitch ratio (1.0 = original pitch, 2.0 = one octave up, 0.5 = one octave down).
    pub pitch: f32,
    /// Whether the sampler is active (looping).
    pub looped: bool,
}

impl Sampler {
    /// Load a WAV file from disk.
    /// Only supports 32-bit float PCM WAV files for simplicity.
    pub fn from_wav<P: AsRef<Path>>(path: P) -> Result<Self, &'static str> {
        let mut reader = WavReader::open(path).map_err(|_| "Could not open WAV file")?;
        let spec = reader.spec();
        if spec.sample_format != hound::SampleFormat::Float || spec.bits_per_sample != 32 {
            return Err("Only 32-bit float PCM WAV files are supported");
        }
        if spec.channels != 1 {
            return Err("Only mono WAV files are supported");
        }
        let samples: Vec<f32> = reader.into_samples().map(|s| s.unwrap()).collect();
        Ok(Sampler {
            samples,
            sample_rate: spec.sample_rate,
            position: 0.0,
            pitch: 1.0,
            looped: false,
        })
    }

    /// Set pitch ratio (1.0 = original pitch).
    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch.max(0.01); // avoid zero or negative
    }

    /// Set looping.
    pub fn set_loop(&mut self, looped: bool) {
        self.looped = looped;
    }

    /// Trigger the sampler (reset position to start).
    pub fn trigger(&mut self) {
        self.position = 0.0;
    }

    /// Get the next sample, advancing position by pitch ratio.
    /// Returns None if the sampler has finished and is not looping.
    pub fn next_sample(&mut self) -> Option<f32> {
        if self.samples.is_empty() {
            return None;
        }
        let idx = self.position as usize;
        let frac = self.position - idx as f32;
        let value = if idx + 1 < self.samples.len() {
            self.samples[idx] * (1.0 - frac) + self.samples[idx + 1] * frac
        } else {
            self.samples[idx]
        };
        // Advance position
        self.position += self.pitch;
        // Check bounds
        if self.position >= self.samples.len() as f32 {
            if self.looped {
                self.position -= self.samples.len() as f32;
            } else {
                return None;
            }
        }
        Some(value)
    }

    /// Check if the sampler is active (has more samples to play or looping).
    pub fn is_active(&self) -> bool {
        self.looped || self.position < self.samples.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};
    use std::fs;

    /// Helper to create a test WAV file with a sine wave.
    fn create_test_wav(path: &str, frequency: f32, duration: f32, sample_rate: u32) {
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec).unwrap();
        let num_samples = (duration * sample_rate as f32) as usize;
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn test_sampler_load_wav() {
        let wav_path = "test_sine.wav";
        create_test_wav(wav_path, 440.0, 1.0, 44100);
        let sampler = Sampler::from_wav(wav_path).expect("Failed to load WAV");
        assert_eq!(sampler.sample_rate, 44100);
        assert_eq!(sampler.samples.len(), 44100);
        // Clean up
        let _ = fs::remove_file(wav_path);
    }

    #[test]
    fn test_sampler_pitch_shift() {
        let wav_path = "test_sine.wav";
        create_test_wav(wav_path, 440.0, 1.0, 44100);
        let mut sampler = Sampler::from_wav(wav_path).expect("Failed to load WAV");
        // Set pitch to 2.0 (one octave up)
        sampler.set_pitch(2.0);
        sampler.trigger();
        // Collect a few samples
        let mut samples = Vec::new();
        for _ in 0..100 {
            if let Some(s) = sampler.next_sample() {
                samples.push(s);
            }
        }
        assert!(!samples.is_empty());
        // Clean up
        let _ = fs::remove_file(wav_path);
    }

    #[test]
    fn test_sampler_looping() {
        let wav_path = "test_sine.wav";
        create_test_wav(wav_path, 440.0, 0.1, 44100); // short sample
        let mut sampler = Sampler::from_wav(wav_path).expect("Failed to load WAV");
        sampler.set_loop(true);
        sampler.trigger();
        // Play for longer than the sample length
        for _ in 0..(sampler.sample_rate as usize * 2) {
            assert!(sampler.next_sample().is_some());
        }
        // Clean up
        let _ = fs::remove_file(wav_path);
    }
}
