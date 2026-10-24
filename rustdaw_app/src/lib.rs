//! DAW application: audio output with cpal and UI with egui.

use std::sync::atomic::AtomicBool;
use rustdaw_visualizer::{Oscilloscope, SpectrumAnalyzer, VuMeter};
use rustdaw_sampler::Sampler;
use ringbuf::{HeapCons, HeapProd};
use ringbuf::traits::Split;
use std::cell::RefCell;
use std::sync::Arc;

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
    /// Samplers for different drum sounds (kick, hat, etc.)
    pub samplers: SamplerMap,
    /// Effect parameters (controlled by UI, read by audio thread).
    pub effect_lpf_cutoff: f32,
    pub effect_delay_time: f32,
    pub effect_delay_feedback: f32,
    pub effect_delay_mix: f32,
    pub effect_reverb_mix: f32,
    pub effect_enabled: bool,
    /// Maximum number of polyphonic voices.
    pub max_voices: u32,
}

/// Map of MIDI note -> Sampler for drum machine style playback.
pub type SamplerMap = std::collections::HashMap<u8, Arc<Sampler>>;

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
            samplers: std::collections::HashMap::new(),
            // Effect defaults
            effect_lpf_cutoff: 20000.0,
            effect_delay_time: 0.3,
            effect_delay_feedback: 0.3,
            effect_delay_mix: 0.0,
            effect_reverb_mix: 0.3,
            effect_enabled: true,
            // Polyphony defaults
            max_voices: 8,
        }
    }

    /// Load a sampler for a specific MIDI note.
    pub fn load_sampler(&mut self, midi_note: u8, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let sampler = Sampler::from_wav(path)?;
        self.samplers.insert(midi_note, Arc::new(sampler));
        Ok(())
    }

    /// Save the current project to a JSON file.
    pub fn save_project<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let project = rustdaw_project::DawProject {
            sequencer: self.sequencer.clone(),
            piano_roll: None,
            master_volume: self.master_volume,
            bpm: self.bpm,
            notes: String::new(),
        };
        project.save(path)
    }

    /// Load a project from a JSON file and apply to this state.
    pub fn load_project<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let project = rustdaw_project::DawProject::load(path)?;
        self.sequencer = project.sequencer;
        self.master_volume = project.master_volume;
        self.bpm = project.bpm;
        Ok(())
    }

    /// Export the current project as a WAV file.
    pub fn export_wav<P: AsRef<std::path::Path>>(&self, path: P, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>> {
        let project = rustdaw_project::DawProject {
            sequencer: self.sequencer.clone(),
            piano_roll: None,
            master_volume: self.master_volume,
            bpm: self.bpm,
            notes: String::new(),
        };
        project.export_wav(path, duration_seconds)
    }
}

#[cfg(feature = "audio")]
mod audio {
    use super::*;
    use std::sync::Arc;
    use std::cell::RefCell;
    use rustdaw_effects::EffectChain;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    /// A voice with its associated MIDI note for tracking.
    struct TrackedVoice {
        voice: rustdaw_engine::Voice,
        midi_note: u8,
        /// Time when this voice was triggered (for voice stealing priority).
        trigger_time: f64,
    }

    impl TrackedVoice {
        fn new(sample_rate: f32, midi_note: u8, waveform: rustdaw_engine::Waveform) -> Self {
            let freq = midi_to_hz(midi_note);
            let mut voice = rustdaw_engine::Voice::new(sample_rate, freq, waveform);
            voice.note_on();
            Self {
                voice,
                midi_note,
                trigger_time: 0.0,
            }
        }
    }

    /// Voice pool with stealing support.
    struct VoicePool {
        voices: Vec<TrackedVoice>,
        max_voices: usize,
        sample_rate: f32,
        next_trigger_time: f64,
    }

    impl VoicePool {
        fn new(max_voices: usize, sample_rate: f32) -> Self {
            Self {
                voices: Vec::with_capacity(max_voices),
                max_voices,
                sample_rate,
                next_trigger_time: 0.0,
            }
        }

        fn set_max_voices(&mut self, max_voices: usize) {
            self.max_voices = max_voices.max(1);
            // If we have more voices than the new max, remove the oldest ones
            if self.voices.len() > self.max_voices {
                // Sort by trigger_time (oldest first) and keep only the newest max_voices
                self.voices.sort_by(|a, b| a.trigger_time.partial_cmp(&b.trigger_time).unwrap());
                self.voices.truncate(self.max_voices);
            }
        }

        fn note_on(&mut self, midi_note: u8, waveform: rustdaw_engine::Waveform) {
            self.next_trigger_time += 1.0;
            
            // First, try to find an existing voice for this note (retrigger)
            if let Some(tv) = self.voices.iter_mut().find(|tv| tv.midi_note == midi_note && tv.voice.is_active()) {
                tv.voice.set_frequency(midi_to_hz(midi_note));
                tv.voice.note_on();
                tv.trigger_time = self.next_trigger_time;
                return;
            }

            // Find a free slot (finished voice)
            if let Some(tv) = self.voices.iter_mut().find(|tv| !tv.voice.is_active()) {
                tv.voice = rustdaw_engine::Voice::new(self.sample_rate, midi_to_hz(midi_note), waveform);
                tv.voice.note_on();
                tv.midi_note = midi_note;
                tv.trigger_time = self.next_trigger_time;
                return;
            }

            // Pool is full - steal a voice
            if self.voices.len() >= self.max_voices {
                self.steal_voice(midi_note, waveform);
                return;
            }

            // Add new voice
            let mut tv = TrackedVoice::new(self.sample_rate, midi_note, waveform);
            tv.trigger_time = self.next_trigger_time;
            self.voices.push(tv);
        }

        fn steal_voice(&mut self, midi_note: u8, waveform: rustdaw_engine::Waveform) {
            // Priority: finished > released (in release phase) > oldest trigger_time
            let mut best_idx = 0;
            let mut best_priority = (2, 0.0); // (state_priority, trigger_time)
            
            for (i, tv) in self.voices.iter().enumerate() {
                let state_priority = if !tv.voice.is_active() { 0 }
                    else if tv.voice.envelope_is_releasing() { 1 }
                    else { 2 };
                
                if state_priority < best_priority.0 || 
                   (state_priority == best_priority.0 && tv.trigger_time < best_priority.1) {
                    best_priority = (state_priority, tv.trigger_time);
                    best_idx = i;
                }
            }

            // Replace the stolen voice
            let mut new_voice = TrackedVoice::new(self.sample_rate, midi_note, waveform);
            new_voice.trigger_time = self.next_trigger_time;
            self.voices[best_idx] = new_voice;
        }

        fn note_off(&mut self, midi_note: u8) {
            for tv in &mut self.voices {
                if tv.midi_note == midi_note && tv.voice.is_active() {
                    tv.voice.note_off();
                }
            }
        }

        fn set_waveform(&mut self, waveform: rustdaw_engine::Waveform) {
            for tv in &mut self.voices {
                tv.voice.set_waveform(waveform);
            }
        }

        fn next_sample(&mut self) -> [f32; 2] {
            let mut output = [0.0f32, 0.0f32];
            for tv in &mut self.voices {
                if tv.voice.is_active() {
                    let sample = tv.voice.next_sample();
                    output[0] += sample[0];
                    output[1] += sample[1];
                }
            }
            output
        }

        fn active_count(&self) -> usize {
            self.voices.iter().filter(|tv| tv.voice.is_active()).count()
        }
    }

    /// Simple audio generator that uses the engine to produce samples.
    pub struct AudioGenerator {
        sample_rate: u32,
        state: Arc<AppState>,
        /// Voice pool for polyphonic synthesis.
        voice_pool: VoicePool,
        /// Position in the sequencer (in steps).
        step_pos: f32,
        /// Time accumulator for stepping.
        time_accum: f32,
        /// Last note that was played (for note-off when step changes).
        last_note: Option<rustdaw_sequencer::Note>,
        /// Active samplers currently playing (midi_note -> sampler with mutable position).
        /// We use RefCell since we need to mutate the sampler's position during audio callback.
        active_samplers: std::collections::HashMap<u8, RefCell<Sampler>>,
        /// Effect chain (LPF -> Delay -> Reverb).
        effects: EffectChain,
    }

    impl AudioGenerator {
        pub fn new(sample_rate: u32, state: Arc<AppState>) -> Self {
            let max_voices = state.max_voices.max(1) as usize;
            Self {
                sample_rate,
                state,
                voice_pool: VoicePool::new(max_voices, sample_rate as f32),
                step_pos: 0.0,
                time_accum: 0.0,
                last_note: None,
                active_samplers: std::collections::HashMap::new(),
                effects: EffectChain::new(sample_rate as f32),
            }
        }

        /// Generate one stereo sample and push it to the audio buffer for visualization.
        /// Returns the sample for audio output.
        pub fn next_sample(&mut self) -> [f32; 2] {
            // Update sequencer position based on time.
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

                // If we are playing, update the voice/samplers based on the note change.
                if self.state.playing.load(Ordering::Relaxed) {
                    // Turn off the old note if it is not the same as the new note.
                    if let Some(ref note) = old_note {
                        if new_note.as_ref() != Some(note) {
                            let midi_note = note.note;
                            // Note off for both sampler and voice pool
                            self.voice_pool.note_off(midi_note);
                            if let Some(sampler_cell) = self.active_samplers.get(&midi_note) {
                                sampler_cell.borrow_mut().trigger(); // reset position, let it finish naturally
                            }
                            self.last_note = None;
                        }
                    }

                    // Turn on the new note if it is not the same as the old note.
                    if let Some(ref note) = new_note {
                        if old_note.as_ref() != Some(note) {
                            let midi_note = note.note;
                            // Check if we have a sampler for this MIDI note
                            if let Some(sampler_arc) = self.state.samplers.get(&midi_note) {
                                // Clone the sampler for this voice (each trigger gets its own position)
                                let mut sampler = (*sampler_arc).clone();
                                sampler.trigger(); // Reset position to start
                                sampler.set_pitch(1.0);
                                self.active_samplers.insert(midi_note, RefCell::new(sampler));
                            } else {
                                // Use voice pool for synthesized voice
                                let waveform = rustdaw_engine::Waveform::Sine; // could be per-note later
                                self.voice_pool.note_on(midi_note, waveform);
                            }
                            self.last_note = Some(note.clone());
                        }
                    }
                }
            }

            // Generate audio: mix voice pool + active samplers
            let mut sample = [0.0f32, 0.0f32];
            
            // Add voice pool output (synthesized polyphonic)
            if self.state.playing.load(Ordering::Relaxed) {
                let voice_sample = self.voice_pool.next_sample();
                sample[0] += voice_sample[0] * self.state.master_volume;
                sample[1] += voice_sample[1] * self.state.master_volume;
            }

            // Add active sampler outputs
            let mut finished_samplers = Vec::new();
            for (midi_note, sampler_cell) in &self.active_samplers {
                if let Some(sampler_sample) = sampler_cell.borrow_mut().next_sample() {
                    let level = self.state.master_volume;
                    sample[0] += sampler_sample * level;
                    sample[1] += sampler_sample * level;
                } else {
                    // Sampler finished (not looping and reached end)
                    finished_samplers.push(*midi_note);
                }
            }
            // Remove finished samplers
            for note in finished_samplers {
                self.active_samplers.remove(&note);
            }

            // Sync effect parameters from AppState (UI thread -> audio thread).
            self.effects.set_enabled(self.state.effect_enabled);
            self.effects.set_lpf_cutoff(self.state.effect_lpf_cutoff);
            self.effects.set_delay_time(self.state.effect_delay_time);
            self.effects.set_delay_feedback(self.state.effect_delay_feedback);
            self.effects.set_delay_mix(self.state.effect_delay_mix);
            self.effects.set_reverb_mix(self.state.effect_reverb_mix);
            self.voice_pool.set_max_voices(self.state.max_voices as usize);

            // Apply effects (process left channel as mono, get stereo back).
            // Convert stereo to mono for effects, then stereo out.
            let mono_in = (sample[0] + sample[1]) * 0.5;
            let [fx_l, fx_r] = self.effects.process(mono_in);
            sample[0] = fx_l;
            sample[1] = fx_r;

            // Push the sample to the audio buffer (left channel for visualization).
            let _ = self.state.audio_producer.push(sample[0]);
            sample
        }
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
    use std::sync::atomic::Ordering;
    use ringbuf::traits::{Producer, Consumer};

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new();
        assert!(!state.playing.load(Ordering::Relaxed));
        assert_eq!(state.bpm, 120.0);
        assert_eq!(state.master_volume, 1.0);
    }

    #[test]
    fn test_audio_generator_creation() {
        let _state = Arc::new(AppState::new());
        #[cfg(feature = "audio")]
        {
            let gen = audio::AudioGenerator::new(44100, _state.clone());
            assert_eq!(gen.sample_rate, 44100);
        }
        #[cfg(not(feature = "audio"))]
        {
            // Just test that the AppState works.
            assert_eq!(_state.bpm, 120.0);
        }
    }

    #[test]
    fn test_audio_generator_next_sample() {
        let _state = Arc::new(AppState::new());
        #[cfg(feature = "audio")]
        {
            let mut gen = audio::AudioGenerator::new(44100, _state.clone());
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
