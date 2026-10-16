/// Project management: save/load JSON and export .wav.
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

/// Represents the entire state of the DAW that can be saved to a project file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DawProject {
    /// The step sequencer state.
    pub sequencer: rustdaw_sequencer::StepSequencer,
    /// The piano roll state (if we were to implement it fully; for now, placeholder).
    pub piano_roll: Option<rustdaw_sequencer::PianoRoll>,
    /// Master volume.
    pub master_volume: f32,
    /// BPM.
    pub bpm: f32,
    /// Any additional notes or metadata.
    pub notes: String,
}

impl DawProject {
    /// Create a new empty project with default BPM 120.0.
    pub fn new() -> Self {
        Self {
            sequencer: rustdaw_sequencer::StepSequencer::new(120.0),
            piano_roll: None,
            master_volume: 1.0,
            bpm: 120.0,
            notes: String::new(),
        }
    }

    /// Save the project to a JSON file.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load a project from a JSON file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let json = fs::read_to_string(path)?;
        let project: DawProject = serde_json::from_str(&json)?;
        Ok(project)
    }

    /// Export the current project as a WAV file using the sequencer and a voice.
    /// This function generates a demo pattern with kick, hat, and synth over 4 measures.
    pub fn export_wav<P: AsRef<Path>>(&self, path: P, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        use rustdaw_engine::{Voice, Waveform};
        use hound::{WavSpec, WavWriter};

        // Create a voice for audio generation.
        let mut voice = Voice::new(44100.0, 60.0, Waveform::Sine);
        // Set envelope parameters for short notes: fast attack, decay, sustain, release.
        voice.set_envelope_params(0.005, 0.02, 0.3, 0.05);

        // We will use the project's sequencer.
        let mut sequencer = self.sequencer.clone();
        // Set the BPM from the project.
        sequencer.set_bpm(self.bpm);
        // We will set up a pattern for one measure (16 steps) where:
        //   Step 0: kick (MIDI 36)
        //   Step 2: hat (MIDI 42)
        //   Step 4: synth (MIDI 60)
        //   Step 6: hat (MIDI 42)
        //   Step 8: kick (MIDI 36)
        //   Step 10: hat (MIDI 42)
        //   Step 12: synth (MIDI 60)
        //   Step 14: hat (MIDI 42)
        // And the rest are silent.

        // Clear all steps first.
        for i in 0..16 {
            sequencer.steps[i].note = None;
        }
        let pattern = [36, 0, 42, 0, 60, 0, 42, 0, 36, 0, 42, 0, 60, 0, 42, 0];
        for (i, &note) in pattern.iter().enumerate() {
            if note != 0 {
                sequencer.steps[i].note = Some(rustdaw_sequencer::Note { note, velocity: 100 });
            }
        }

        // Now, we generate the audio using the same logic as AudioGenerator.
        let sample_rate = 44100;
        let num_samples = (duration_seconds * sample_rate as f32) as usize;
        let mut left_channel = Vec::with_capacity(num_samples);
        let mut right_channel = Vec::with_capacity(num_samples);

        // Helper function to convert MIDI note to frequency.
        fn midi_to_hz(note: u8) -> f32 {
            440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
        }

        // We need to track the sequencer position and time accumulator.
        let mut step_pos: f32 = 0.0;
        let mut time_accum: f32 = 0.0;
        let sec_per_step = 60.0 / self.bpm / 16.0; // 16 steps per beat

        // We will also track the last note to avoid retriggering the same note.
        let mut last_note: Option<rustdaw_sequencer::Note> = None;

        for _ in 0..num_samples {
            // Update sequencer position based on time.
            time_accum += 1.0 / sample_rate as f32;
            let mut steps_to_advance = 0;
            while time_accum >= sec_per_step {
                time_accum -= sec_per_step;
                steps_to_advance += 1;
            }
            if steps_to_advance > 0 {
                // Update the step position.
                let old_step = step_pos as usize;
                step_pos = (step_pos + steps_to_advance as f32) % 16.0;
                let new_step = step_pos as usize;

                // Get the note for the old step (if any) and the new step (if any).
                let old_note = sequencer.get_step(old_step).and_then(|s| s.note.clone());
                let new_note = sequencer.get_step(new_step).and_then(|s| s.note.clone());

                // If we are playing (we always are for the demo), update the voice based on the note change.
                {
                    // Turn off the old note if it is not the same as the new note.
                    if let Some(ref note) = old_note {
                        if new_note.as_ref() != Some(note) {
                            voice.note_off();
                            last_note = None;
                        }
                    }

                    // Turn on the new note if it is not the same as the old note.
                    if let Some(ref note) = new_note {
                        if last_note.as_ref() != Some(note) {
                            voice.set_frequency(midi_to_hz(note.note));
                            voice.note_on();
                            last_note = Some(note.clone());
                        }
                    }
                }
            }

            // Get a sample from the voice.
            let sample = voice.next_sample();
            left_channel.push(sample[0] * self.master_volume);
            right_channel.push(sample[1] * self.master_volume);
        }

        // Interleave left and right channels for stereo.
        let mut interleaved = Vec::with_capacity(num_samples * 2);
        for i in 0..num_samples {
            interleaved.push(left_channel[i]);
            interleaved.push(right_channel[i]);
        }

        // Write to WAV file.
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec)?;
        for sample in interleaved {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_project_save_and_load() {
        let dir = env::temp_dir();
        let path = dir.join("test_project.json");

        let mut project = DawProject::new();
        project.sequencer.set_bpm(140.0);
        project.master_volume = 0.8;
        project.notes = String::from("Test project");

        // Save
        project.save(&path).unwrap();

        // Load
        let loaded = DawProject::load(&path).unwrap();
        assert_eq!(loaded.sequencer.bpm, 140.0);
        assert!((loaded.master_volume - 0.8).abs() < 1e-5);
        assert_eq!(loaded.notes, "Test project");

        // Clean up
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_project_export_wav() {
        let path = std::path::Path::new("demo_pattern.wav");
        let project = DawProject::new();
        // Export 4 seconds of audio.
        let _ = project.export_wav(path, 4.0);
        assert!(path.exists());
        // Note: we leave the file for inspection.

        // Now, let's analyze and print the RMS per 125 ms window.
        use hound::{WavReader, SampleFormat};
        let mut reader = WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 44100);
        assert_eq!(reader.spec().bits_per_sample, 32);
        assert_eq!(reader.spec().sample_format, SampleFormat::Float);

        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len() % 2, 0); // stereo
        let mono_samples: Vec<f32> = samples.chunks(2).map(|chunk| (chunk[0] + chunk[1]) / 2.0).collect();

        // Compute RMS in 125 ms windows
        let window_size = (0.125 * 44100.0) as usize; // 125 ms
        assert!(window_size > 0);
        let mut rms_values = Vec::new();
        for chunk in mono_samples.chunks(window_size) {
            if chunk.is_empty() {
                break;
            }
            let sum_squares: f64 = chunk.iter().map(|&s| s as f64 * s as f64).sum();
            let rms = (sum_squares / chunk.len() as f64).sqrt() as f32;
            rms_values.push(rms);
        }
        assert!(!rms_values.is_empty());

        // Print the RMS values
        println!("RMS values per 125 ms window:");
        for (i, rms) in rms_values.iter().enumerate() {
            println!("Window {}: {:.6}", i, rms);
        }
    }

    #[test]
    fn test_export_wav_rhythm() {
        use hound::{WavReader, SampleFormat};
        use std::env;

        let mut project = DawProject::new();
        project.sequencer.set_bpm(120.0);
        // Set up the pattern: kick, hat, synth, hat, kick, hat, synth, hat (over 16 steps)
        let pattern = [36, 0, 42, 0, 60, 0, 42, 0, 36, 0, 42, 0, 60, 0, 42, 0];
        for (i, &note) in pattern.iter().enumerate() {
            if note != 0 {
                project.sequencer.steps[i].note = Some(rustdaw_sequencer::Note { note, velocity: 100 });
            } else {
                project.sequencer.steps[i].note = None;
            }
        }

        let dir = env::temp_dir();
        let path = dir.join("test_rhythm.wav");
        let _ = project.export_wav(&path, 4.0);
        assert!(path.exists());

        // Read the WAV file
        let mut reader = WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 44100);
        assert_eq!(reader.spec().bits_per_sample, 32);
        assert_eq!(reader.spec().sample_format, SampleFormat::Float);

        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len() % 2, 0); // stereo
        let mono_samples: Vec<f32> = samples.chunks(2).map(|chunk| (chunk[0] + chunk[1]) / 2.0).collect();

        // Compute RMS in 125 ms windows
        let window_size = (0.125 * 44100.0) as usize; // 125 ms
        assert!(window_size > 0);
        let mut rms_values = Vec::new();
        for chunk in mono_samples.chunks(window_size) {
            if chunk.is_empty() {
                break;
            }
            let sum_squares: f64 = chunk.iter().map(|&s| s as f64 * s as f64).sum();
            let rms = (sum_squares / chunk.len() as f64).sqrt() as f32;
            rms_values.push(rms);
        }
        assert!(!rms_values.is_empty());

        // Check that RMS is not constant (i.e., there is variation)
        let mean = rms_values.iter().sum::<f32>() / rms_values.len() as f32;
        let variance: f32 = rms_values.iter().map(|&v| (v - mean) * (v - mean)).sum::<f32>() / rms_values.len() as f32;
        // If variance is very small, the signal is constant
        assert!(variance > 1e-4, "RMS is constant (variance = {}), expected rhythmic pattern", variance);

        // Additionally, check that the RMS peaks are at least 3x the valleys by looking at the max and min.
        let max_rms = rms_values.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        let min_rms = rms_values.iter().cloned().fold(f32::INFINITY, |a, b| a.min(b));
        assert!(max_rms > 3.0 * min_rms, "Max RMS ({}) is not more than 3x min RMS ({})", max_rms, min_rms);
    }
}