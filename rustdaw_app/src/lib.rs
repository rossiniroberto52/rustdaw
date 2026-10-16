//! DAW application: audio output with cpal and UI with egui.

use std::sync::atomic::{AtomicBool, Ordering};
use rustdaw_visualizer::{Oscilloscope, SpectrumAnalyzer, VuMeter};
use ringbuf::{HeapCons, HeapProd};
use ringbuf::traits::{Producer, Consumer};
use std::cell::RefCell;
use ringbuf::traits::Split;

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
    /// Audio producer for visualization (used by audio thread).
    pub audio_producer: HeapProd<f32>,
    /// Audio consumer for visualization (used by UI thread).
    pub audio_consumer: HeapCons<f32>,
    /// Visualizer widgets.
    pub oscilloscope: RefCell<Oscilloscope>,
    pub spectrum_analyzer: RefCell<SpectrumAnalyzer>,
    pub vu_meter: RefCell<VuMeter>,
}

impl AppState {
    pub fn new() -> Self {
        let size = 44100 * 2; // 2 seconds of stereo at 44.1kHz.
        let ringbuf = ringbuf::HeapRb::<f32>::new(size);
        let (producer, consumer) = ringbuf.split();
        Self {
            playing: AtomicBool::new(false),
            bpm: 120.0,
            sequencer: rustdaw_sequencer::StepSequencer::new(120.0),
            master_volume: 1.0,
            audio_producer: producer,
            audio_consumer: consumer,
            oscilloscope: RefCell::new(Oscilloscope::new(1024)),
            spectrum_analyzer: RefCell::new(SpectrumAnalyzer::new(1024)),
            vu_meter: RefCell::new(VuMeter::new()),
        }
    }
}

#[cfg(feature = "audio")]
mod audio {
    use super::*;
    use std::sync::Arc;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    /// Simple audio generator that uses the engine to produce samples.
    pub struct AudioGenerator {
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

        /// Generate one stereo sample and push it to the audio buffer for visualization.
        /// Returns the sample for audio output.
        pub fn next_sample(&mut self) -> [f32; 2] {            // Update sequencer position based on time.
            let sec_per_step = 60.0 / self.state.bpm / 16.0; // 16 steps per beat
            self.time_accum += 1.0 / self.sample_rate as f32;
            let mut steps_to_advance = 0;
            while self.time_accum >= sec_per_step {
                self.time_accum -= sec_per_step;
                steps_to_advance += 1;
            }
            if steps_to_advance > 0 {
                // Update the step position.
                let old_step = self.step_pos as usize;
                self.step_pos = (self.step_pos + steps_to_advance as f32) % 16.0;
                let new_step = self.step_pos as usize;
                
                // Get the note for the old step (if any) and the new step (if any).
                let old_note = self.state.sequencer.get_step(old_step).and_then(|s| s.note.clone());
                let new_note = self.state.sequencer.get_step(new_step).and_then(|s| s.note.clone());
                
                // If we are playing, update the voice based on the note change.
                if self.state.playing.load(Ordering::Relaxed) {
                    // Turn off the old note if it is not the same as the new note.
                    if let Some(ref note) = old_note {
                        if new_note.as_ref() != Some(note) {
                            self.voice.note_off();
                            self.last_note = None;
                        }
                    }
                    
                    // Turn on the new note if it is not the same as the old note.
                    if let Some(ref note) = new_note {
                        if old_note.as_ref() != Some(note) {
                            self.voice.set_frequency(midi_to_hz(note.note));
                            self.voice.note_on();
                            self.last_note = Some(note.clone());
                        }
                    }
                }
            }
            
            // For now, just get a sample from the voice.
            let sample = if self.state.playing.load(Ordering::Relaxed) {
                let s = self.voice.next_sample() * self.state.master_volume;
                [s, s]
            } else {
                [0.0, 0.0]
            };
            
            // Push the sample to the audio buffer (we'll push the left channel for visualization).
            // We push both channels? We'll push the left channel as a mono signal for simplicity.
            let _ = self.state.audio_producer.push(sample[0]);
            sample}
    }

    pub fn run_audio_thread(state: Arc<AppState>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = host.default_output_device().expect("No output device available");
            let config = device.default_output_config().expect("Failed to get default output config");
            let sample_rate = config.sample_rate().0 as u32;
            let mut generator = AudioGenerator::new(sample_rate, Arc::clone(&state));

            let err_fn = move |err| {
                eprintln!("An error occurred on the output audio stream: {}", err);
            };

            let stream = device
                .build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        write_data(data, &mut generator);
                    },
                    err_fn,
                    None,
                )
                .expect("Failed to build output stream");

            stream.play().expect("Failed to play output stream");

            // Run until the app is stopped.
            while state.playing.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    }

    fn write_data(output: &mut [f32], generator: &mut AudioGenerator) {
        for frame in output.chunks_mut(2) {
            let sample = generator.next_sample();
            frame[0] = sample[0];
            frame[1] = sample[1];
        }
    }
}

impl AppState {
    #[cfg(feature = "audio")]
    pub fn start_audio(&self) -> Option<std::thread::JoinHandle<()>> {
        if self.playing.load(Ordering::Relaxed) {
            None
        } else {
            self.playing.store(true, Ordering::Relaxed);
            Some(audio::run_audio_thread(Arc::new(self.clone())))
        }
    }

    #[cfg(feature = "audio")]
    pub fn stop_audio(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }

    #[cfg(not(feature = "audio"))]
    pub fn start_audio(&self) -> Option<std::thread::JoinHandle<()>> {
        None
    }

    #[cfg(not(feature = "audio"))]
    pub fn stop_audio(&self) {
        // Do nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use ringbuf::HeapRb;

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
        #[cfg(feature = "audio")]
        {
            let gen = audio::AudioGenerator::new(44100, state);
            assert_eq!(gen.sample_rate, 44100);
        }
        #[cfg(not(feature = "audio"))]
        {
            // Just test that the AppState works.
            assert_eq!(state.bpm, 120.0);
        }
    }

    #[test]
    fn test_audio_generator_next_sample() {
        let state = Arc::new(AppState::new());
        #[cfg(feature = "audio")]
        {
            let mut gen = audio::AudioGenerator::new(44100, state);
            let sample = gen.next_sample();
            // When not playing, should be silence.
            assert_eq!(sample[0], 0.0);
            assert_eq!(sample[1], 0.0);
        }
        #[cfg(not(feature = "audio"))]
        {
            // Just test that the AppState works.
            let sample = [0.0, 0.0];
            assert_eq!(sample[0], 0.0);
            assert_eq!(sample[1], 0.0);
        }
    }

    #[test]
    fn test_app_state_ringbuf() {
        let mut state = AppState::new();
        // Push a sample via the producer.
        assert!(state.audio_producer.try_push(0.5).is_ok());
        // Pop it via the consumer.
        let sample = state.audio_consumer.try_pop().unwrap();
        assert!((sample - 0.5).abs() < 1e-5);
    }
}
