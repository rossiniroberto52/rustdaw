//! Project management: save/load JSON and export .wav.

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

    /// Export the current project as a WAV file using the offline renderer.
    /// This is a simplified version that just renders the sequencer with a single voice.
    /// In a full DAW, we would route through the mixer, effects, etc.
    /// Export the current project as a WAV file using the sequencer and a voice.
    /// This function generates a demo pattern with kick, hat, and synth over 4 measures.
    pub fn export_wav<P: AsRef<Path>>(&self, path: P, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>>
    where
        P: AsRef<Path>,
    {
        use rustdaw_app::AudioGenerator;
        use hound::{WavSpec, WavWriter};

        // Create an audio generator with the project's sample rate (44100 Hz) and the project's BPM.
        let mut audio_gen = AudioGenerator::new(44100);
        // Set the BPM from the project.
        {
            let mut state = audio_gen.state.lock().unwrap();
            state.sequencer.set_bpm(self.bpm);
            // We'll set up a simple pattern: kick on step 0, hat on steps 4,8,12, and synth on steps 0,2,4,6,8,10,12,14 (every other step).
            // But note: the AudioGenerator currently only plays one note at a time.
            // We cannot play kick and synth simultaneously with one voice.
            // We will approximate the drum pattern by having the voice play a sequence of notes that represent the kick, hat, and synth.
            // We'll define a pattern of notes (as MIDI numbers) for each 16-step pattern (one measure) and then loop it 4 times.

            // Clear all steps first.
            for i in 0..16 {
                state.sequencer.steps[i].note = None;
            }
            // Let's define a pattern for one measure (16 steps) where:
            //   Step 0: kick (MIDI 36)
            //   Step 2: hat (MIDI 42)
            //   Step 4: synth (MIDI 60)
            //   Step 6: hat (MIDI 42)
            //   Step 8: kick (MIDI 36)
            //   Step 10: hat (MIDI 42)
            //   Step 12: synth (MIDI 60)
            //   Step 14: hat (MIDI 42)
            // And the rest are silent.

            let pattern = [36, 0, 42, 0, 60, 0, 42, 0, 36, 0, 42, 0, 60, 0, 42, 0];
            for (i, &note) in pattern.iter().enumerate() {
                if note != 0 {
                    state.sequencer.steps[i].note = Some(rustdaw_sequencer::Note { note, velocity: 100 });
                }
            }
        }
        // Note: the AudioGenerator's voice uses a fixed ADSR envelope. We cannot change it per note.

        // Now, we generate the audio.
        let sample_rate = 44100;
        let num_samples = (duration_seconds * sample_rate as f32) as usize;
        let mut left_channel = Vec::with_capacity(num_samples);
        let mut right_channel = Vec::with_capacity(num_samples);

        for _ in 0..num_samples {
            let sample = audio_gen.next_sample();
            left_channel.push(sample[0]);
            right_channel.push(sample[1]);
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
        P: AsRef<Path>,
    {
        use hound::{WavSpec, WavWriter};
        use rustdaw_engine::{OfflineRenderer, Voice, Waveform};

        let mut renderer = OfflineRenderer::new(44100, 2);
        // Create a voice for each active step? For simplicity, we'll just use one voice that plays
        // the notes from the sequencer in order. This is a very basic interpretation.
        let mut voice = Voice::new(44100.0, 60.0, Waveform::Sine);
        // We'll just note that this is a placeholder for a real export function.
        // In a real implementation, we would read the sequencer and trigger notes accordingly.
        renderer.add_voice(Box::new(voice));

        // Render the audio.
        let buffer = renderer.render(duration_seconds);
        // Write to WAV file.
        let spec = WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec)?;
        for sample in buffer {
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
        let dir = env::temp_dir();
        let path = dir.join("test_export.wav");

        let project = DawProject::new();
        // Export 0.5 seconds of audio.
        let _ = project.export_wav(&path, 0.5);
        assert!(path.exists());

        // Clean up
        let _ = fs::remove_file(&path);
    }
}
