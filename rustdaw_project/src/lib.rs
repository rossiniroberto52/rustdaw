//! Project management: save/load JSON and export .wav.

use serde::{Deserialize, Serialize};
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
    pub fn export_wav<P: AsRef<Path>>(&self, path: P, duration_seconds: f32) -> Result<(), Box<dyn std::error::Error>>
    where
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
    use tempfile::TempDir;

    #[test]
    fn test_project_save_and_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_project.json");

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
    }

    #[test]
    fn test_project_export_wav() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_export.wav");

        let project = DawProject::new();
        // Export 0.5 seconds of audio.
        let _ = project.export_wav(&path, 0.5);
        assert!(path.exists());
    }
}
