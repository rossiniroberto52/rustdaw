//! Musical sequencer: step sequencer (16 steps) and piano roll data model.

use serde::{Deserialize, serialize};

/// Representa uma nota musical (MIDI note number) e sua velocidade (velocity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct Note {
    /// MIDI note number (0-127)
    pub note: u8,
    /// Velocity (0-127)
    pub velocity: u8,
}

impl Note {
    pub fn new(note: u8, velocity: u8) -> Self {
        Self { note, velocity }
    }
}

/// Passo do sequencer: pode estar vazio ou conter uma nota.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Step {
    /// Nota opcional neste passo.
    pub note: Option<Note>,
}

impl Step {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_note(note: Note) -> Self {
        Self { note: Some(note) }
    }

    pub fn is_empty(&self) -> bool {
        self.note.is_none()
    }
}

/// Sequencer de passos com 16 passos.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StepSequencer {
    /// Os 16 passos.
    pub steps: [Step; 16],
    /// BPM (batidas por minuto).
    pub bpm: f32,
    /// Número de passos por batida (geralmente 16 para sequencer de 16avos).
    pub steps_per_beat: u32,
}

impl StepSequencer {
    pub fn new(bpm: f32) -> Self {
        Self {
            steps: [Step::new(); 16],
            bpm,
            steps_per_beat: 16,
        }
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm;
    }

    pub fn get_step(&self, index: usize) -> Option<&Step> {
        self.steps.get(index % 16)
    }

    pub fn set_step(&mut self, index: usize, step: Step) {
        self.steps[index % 16] = step;
    }

    pub fn clear_step(&mut self, index: usize) {
        self.steps[index % 16] = Step::new();
    }

    /// Returns the duration of a step in seconds.
    pub fn step_duration(&self) -> f32 {
        60.0 / self.bpm / (self.steps_per_beat as f32)
    }
}

/// Modelo de dados para piano roll (roll de piano).
/// Cada linha representa uma nota MIDI (0-127) e cada coluna representa um tempo discretizado.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PianoRoll {
    /// Grade de notas: linhas = notas MIDI (0-127), colunas = ticks (unidades de tempo).
    /// O valor é a velocidade (0-127) ou 0 se não houver nota.
    pub grid: Vec<Vec<u8>>,
    /// Resolução: ticks por batida (PPQN - pulses per quarter note).
    pub ticks_per_beat: u32,
    /// Comprimento total em ticks.
    pub length_ticks: u32,
}

impl PianoRoll {
    pub fn new(ticks_per_beat: u32, length_ticks: u32) -> Self {
        // 128 notas MIDI (0-127)
        let grid = vec![vec![0u8; length_ticks as usize]; 128];
        Self {
            grid,
            ticks_per_beat,
            length_ticks,
        }
    }

    /// Define uma nota na grade.
    pub fn set_note(&mut self, midi_note: u8, tick: u32, velocity: u8) {
        if midi_note < 128 && tick < self.length_ticks {
            self.grid[midi_note as usize][tick as usize] = velocity;
        }
    }

    /// Remove uma nota da grade.
    pub fn clear_note(&mut self, midi_note: u8, tick: u32) {
        if midi_note < 128 && tick < self.length_ticks {
            self.grid[midi_note as usize][tick as usize] = 0;
        }
    }

    /// Obtém a velocidade de uma nota em um determinado tick.
    pub fn get_velocity(&self, midi_note: u8, tick: u32) -> u8 {
        if midi_note < 128 && tick < self.length_ticks {
            self.grid[midi_note as usize][tick as usize]
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_sequencer_creation() {
        let seq = StepSequencer::new(120.0);
        assert_eq!(seq.bpm, 120.0);
        assert_eq!(seq.steps.len(), 16);
        for step in &seq.steps {
            assert!(step.is_empty());
        }
    }

    #[test]
    fn test_step_sequencer_set_get_step() {
        let mut seq = StepSequencer::new(120.0);
        let note = Note::new(60, 100); // C4
        seq.set_step(0, Step::with_note(note));
        assert_eq!(seq.get_step(0).unwrap().note, Some(note));
        assert_eq!(seq.get_step(16).unwrap().note, Some(note)); // wrap around
    }

    #[test]
    fn test_step_sequencer_clear_step() {
        let mut seq = StepSequencer::new(120.0);
        let note = Note::new(60, 100);
        seq.set_step(0, Step::with_note(note));
        assert!(seq.get_step(0).unwrap().note.is_some());
        seq.clear_step(0);
        assert!(seq.get_step(0).unwrap().note.is_none());
    }

    #[test]
    fn test_step_sequencer_step_duration() {
        let seq = StepSequencer::new(120.0);
        // 120 BPM, 16 steps per beat -> each step is a 16th note
        // 60 seconds / 120 beats per minute = 0.5 seconds per beat
        // 0.5 seconds / 16 steps per beat = 0.03125 seconds per step
        assert!((seq.step_duration() - 0.03125).abs() < 1e-5);
    }

    #[test]
    fn test_piano_roll_creation() {
        let roll = PianoRoll::new(480, 960); // 480 PPQN, 2 measures of 4/4
        assert_eq!(roll.ticks_per_beat, 480);
        assert_eq!(roll.length_ticks, 960);
        assert_eq!(roll.grid.len(), 128);
        assert_eq!(roll.grid[0].len(), 960);
        for row in &roll.grid {
            for &val in row {
                assert_eq!(val, 0);
            }
        }
    }

    #[test]
    fn test_piano_roll_set_get_note() {
        let mut roll = PianoRoll::new(480, 960);
        roll.set_note(60, 0, 100); // C4 no início
        assert_eq!(roll.get_velocity(60, 0), 100);
        assert_eq!(roll.get_velocity(61, 0), 0); // nota vazia
    }

    #[test]
    fn test_piano_roll_clear_note() {
        let mut roll = PianoRoll::new(480, 960);
        roll.set_note(60, 0, 100);
        assert_eq!(roll.get_velocity(60, 0), 100);
        roll.clear_note(60, 0);
        assert_eq!(roll.get_velocity(60, 0), 0);
    }
}
