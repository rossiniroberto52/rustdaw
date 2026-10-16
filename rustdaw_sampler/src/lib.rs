//! Sampler: load .wav file and play with pitch.

use std::path::Path;

/// Simple sampler that holds audio data and can play it back at different pitches.
pub struct Sampler {
    /// Audio sample (mono, normalized to [-1.0, 1.0]).
    pub samples: Vec<f32>,
    /// Sample rate of the loaded audio.
    pub sample_rate: u32,
    /// Current playback position (in samples).
    pub position: f32,
    /// Pitch ratio (1.0 = original pitch, 2.0 = one octave up, 0.5 = one octave down).
    pub pitch: f32,
    /// Whether the sampler is active (looping).
    pub looped: bool,
}

impl Sampler {
    /// Load a WAV file from disk.
    /// Only support 32-bit float PCM WAV file for simplicity.
    pub fn from_wav<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        use rustdaw_engine::wav::reader;
        let (spec, samples) = reader(path)?;
        if spec.channels != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Only mono WAV file is supported",
            ));
        }
        if spec.bits_per_sample != 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Only 32-bit float PCM WAV file is supported",
            ));
        }
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
    /// Return None if the sampler has finished and is not looping.
    pub fn next_sample(&mut self) -> Option<f32> {
        if self.samples.is_empty() {
            return None;
        }
        // If current position is beyond the end and not looping, we are done.
        if self.position >= self.samples.len() as f32 && !self.looped {
            return None;
        }
        // If looping and beyond the end, wrap position.
        if self.position >= self.samples.len() as f32 && self.looped {
            self.position = self.position % self.samples.len() as f32;
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
        Some(value)
    }

    /// Check if the sampler is active (has more samples to play or looping).
    pub fn is_active(&self) -> bool {
        self.looped || !self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sampler_load_wav() {
        // Create a temporary WAV file using the engine's wav writer
        let wav_path = "test_sampler.wav";
        {
            use rustdaw_engine::wav::{writer, WavSpec, SampleFormat};
            let spec = WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 32,
                sample_format: SampleFormat::F32,
            };
            // Create a simple sine wave
            let duration = 0.1; // seconds
            let sample_count = (spec.sample_rate as f32 * duration) as usize;
            let mut samples = Vec::with_capacity(sample_count);
            for i in 0..sample_count {
                let t = i as f32 / spec.sample_rate as f32;
                samples.push((2.0 * std::f32::consts::PI * 440.0 * t).sin());
            }
            writer(wav_path, &spec, &samples).expect("Failed to write WAV file");
        }

        // Load the WAV file with the sampler
        let _sampler = Sampler::from_wav(wav_path).expect("Failed to load WAV");
        // Clean up
        let _ = fs::remove_file(wav_path);
    }

    #[test]
    fn test_sampler_next_sample() {
        let mut sampler = Sampler {
            samples: vec![0.0, 1.0, 0.0, -1.0],
            sample_rate: 44100,
            position: 0.0,
            pitch: 1.0,
            looped: false,
        };
        assert_eq!(sampler.next_sample(), Some(0.0));
        assert_eq!(sampler.next_sample(), Some(1.0));
        assert_eq!(sampler.next_sample(), Some(0.0));
        assert_eq!(sampler.next_sample(), Some(-1.0));
        assert_eq!(sampler.next_sample(), None); // finished and not looping
    }

    #[test]
    fn test_sampler_looping() {
        let mut sampler = Sampler {
            samples: vec![0.0, 1.0],
            sample_rate: 44100,
            position: 0.0,
            pitch: 1.0,
            looped: true,
        };
        assert_eq!(sampler.next_sample(), Some(0.0));
        assert_eq!(sampler.next_sample(), Some(1.0));
        assert_eq!(sampler.next_sample(), Some(0.0)); // looped
        assert_eq!(sampler.next_sample(), Some(1.0));
    }
}
