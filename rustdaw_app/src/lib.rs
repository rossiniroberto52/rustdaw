//! DAW application: audio output with cpal and UI with egui.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// Shared state between audio thread and UI thread.
pub struct AppState {
    /// Whether the audio is playing.
    pub playing: AtomicBool,
    /// Current BPM.
    pub bpm: f32,
    /// The step sequencer.
    pub sequencer: rustdaw_sequencer::StepSequencer,
    /// Mixer volume.
    pub master_volume: f32,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            bpm: 120.0,
            sequencer: rustdaw_sequencer::StepSequencer::new(120.0),
            master_volume: 1.0,
        }
    }
}

/// Simple audio generator that uses the engine to produce samples.
struct AudioGenerator {
    sample_rate: u32,
    state: Arc<AppState>,
    /// Engine components: we'll use a simple voice for now.
    voice: rustdaw_engine::Voice,
    /// Position in the sequencer (in steps).
    step_pos: f32,
    /// Time accumulator for stepping.
    time_accum: f32,
}

impl AudioGenerator {
    pub fn new(sample_rate: u32, state: Arc<AppState>) -> Self {
        let mut voice = rustdaw_engine::Voice::new(sample_rate as f32, 60.0, rustdaw_engine::Waveform::Sine);
        voice.note_on();
        Self {
            sample_rate,
            state,
            voice,
            step_pos: 0.0,
            time_accum: 0.0,
        }
    }

    /// Generate one stereo sample.
    pub fn next_sample(&mut self) -> [f32; 2] {
        // Update sequencer position based on time.
        let sec_per_step = 60.0 / self.state.bpm / 16.0; // 16 steps per beat
        self.time_accum += 1.0 / self.sample_rate as f32;
        while self.time_accum >= sec_per_step {
            self.time_accum -= sec_per_step;
            self.step_pos = (self.step_pos + 1.0) % 16.0;
            // TODO: Trigger notes based on sequencer step.
        }

        // For now, just get a sample from the voice.
        let sample = if self.state.playing.load(Ordering::Relaxed) {
            self.voice.next_sample() * self.state.master_volume
        } else {
            0.0
        };
        [sample, sample]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new();
        assert!(!state.playing.load(Ordering::Relaxed));
        assert_eq!(state.bpm, 120.0);
        assert_eq!(state.master_volume, 1.0);
    }

    #[test]
    fn test_audio_generator_creation() {
        let state = Arc::new(AppState::new());
        let gen = AudioGenerator::new(44100, state);
        assert_eq!(gen.sample_rate, 44100);
    }

    #[test]
    fn test_audio_generator_next_sample() {
        let state = Arc::new(AppState::new());
        let mut gen = AudioGenerator::new(44100, state);
        let sample = gen.next_sample();
        // When not playing, should be silence.
        assert_eq!(sample[0], 0.0);
        assert_eq!(sample[1], 0.0);
    }
}
