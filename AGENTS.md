# RustDAW - Agent Reference

## Project Overview
Open-source Digital Audio Workstation written in Rust. Workspace with 7 crates + root package.

## Workspace Structure

```
/home/rossini/rustdaw/
├── Cargo.toml              # Workspace manifest (7 members)
├── README.md               # Project docs
├── src/main.rs             # Root binary (stub - prints "Hello, world!")
├── analyze.rs              # Standalone WAV analysis tool (RMS per 125ms window)
├── analyze_rms.rs          # Duplicate of analyze.rs
├── demo_pattern.wav        # Generated test output (4s, stereo, 44.1kHz, f32)
├── rustdaw_engine/         # Core audio engine
├── rustdaw_sequencer/      # Step sequencer (16 steps) + piano roll
├── rustdaw_sampler/        # WAV sampler with pitch shifting
├── rustdaw_effects/        # Filter, delay, reverb
├── rustdaw_visualizer/     # Oscilloscope, spectrum analyzer, VU meter (egui)
├── rustdaw_app/            # Main app: audio (cpal) + UI (egui)
├── rustdaw_project/        # Project save/load (JSON) + WAV export
└── target/                 # Build artifacts
```

## Crate Dependency Graph

```
rustdaw_app
  ├── rustdaw_engine
  ├── rustdaw_sequencer
  ├── rustdaw_sampler
  ├── rustdaw_effects
  ├── rustdaw_visualizer
  └── rustdaw_project (depends on app, engine, sequencer, sampler, effects)

rustdaw_sampler → rustdaw_engine (for WAV reader)
rustdaw_project → rustdaw_engine, rustdaw_sequencer, rustdaw_sampler, rustdaw_effects, rustdaw_app
rustdaw_visualizer → eframe, rustfft, ringbuf
```

## Key Entry Points

| Crate | Entry Point | Purpose |
|-------|-------------|---------|
| `rustdaw_app` | `src/main.rs` → `AppState::new()` | Main GUI app (run with `cargo run --release -p rustdaw_app`) |
| `rustdaw_project` | `DawProject::export_wav()` | Generates `demo_pattern.wav` (4 measures: kick/hat/synth) |
| `analyze.rs` | `cargo run --bin analyze` | Analyzes WAV RMS per 125ms window |

## Build / Run Commands

```bash
# Build all (release)
cargo build --release

# Run main app (requires audio feature)
cargo run --release -p rustdaw_app

# Run WAV analysis on demo_pattern.wav
cargo run --bin analyze

# Run tests
cargo test --workspace
```

## Audio Feature
The `audio` feature (enabled via `cargo run --release -p rustdaw_app --features audio`) activates:
- CPAL audio output stream
- Real-time audio thread (`AudioGenerator`)
- Sequencer-driven voice playback

Without the feature, UI runs but audio is silent.

## Key Types by Crate

### rustdaw_engine
- `Waveform` - Sine, Square, Sawtooth, Triangle, Noise
- `Oscillator` - Basic oscillator with `next_sample()`
- `Adsr` - ADSR envelope with `trigger_on/off()`, `next_sample()`
- `Voice` - Oscillator + ADSR, implements `AudioProcessor`
- `Mixer` - Multi-channel mixer with master volume
- `OfflineRenderer` - Renders to buffer, writes WAV via `wav::writer`
- `wav::reader` / `wav::writer` - WAV I/O (PCM 16/32-bit)

### rustdaw_sequencer
- `Note { note: u8, velocity: u8 }` - MIDI note
- `Step { note: Option<Note> }` - Single sequencer step
- `StepSequencer { steps: [Step; 16], bpm, steps_per_beat }`
- `PianoRoll { grid: Vec<Vec<u8>>, ticks_per_beat, length_ticks }`

### rustdaw_sampler
- `Sampler { samples, sample_rate, position, pitch, looped }`
- `from_wav(path)` - Loads mono 32-bit float WAV
- `next_sample()` - Returns `Option<f32>` (linear interpolation)

### rustdaw_effects
- `LowPassFilter { fc, fs, prev }` - One-pole LPF
- `Delay { delay_time, feedback, mix, fs, buffer }`
- `Reverb` - Schroeder: 4 comb filters + 4 all-pass filters

### rustdaw_visualizer
- `AudioBuffer { producer, consumer }` - Lock-free ring buffer (split for audio/UI threads)
- `Oscilloscope` - Waveform display
- `SpectrumAnalyzer` - FFT magnitude spectrum (Hann window, smoothing)
- `VuMeter` - Peak + RMS level display

### rustdaw_app
- `AppState` - Shared state (Arc<Mutex<>>): playing, bpm, sequencer, master_volume, audio ring buffer, visualizer widgets
- `AudioGenerator` (audio feature) - Sequencer-driven voice, pushes samples to ring buffer
- `run_audio_thread()` - Spawns CPAL output stream

### rustdaw_project
- `DawProject` - Serializable project state (sequencer, piano_roll, master_volume, bpm, notes)
- `save(path)` / `load(path)` - JSON serialization
- `export_wav(path, duration)` - Generates 4-measure pattern (kick/hat/synth) using engine voices

## Test Coverage
Each crate has `#[cfg(test)]` modules. Run with `cargo test --workspace`.

Key test patterns:
- Engine: oscillator, ADSR, offline render to WAV
- Sequencer: step get/set/clear, step_duration, piano roll
- Sampler: load WAV, next_sample, looping
- Effects: LPF stability, delay mix, reverb NaN check
- Visualizer: audio buffer, spectrum FFT bin detection
- App: state creation, ring buffer, audio generator (feature-gated)
- Project: save/load JSON, export_wav rhythm validation (RMS variance, peak < 0.9, kick freq < 150Hz, hat silence in last 20ms)

## Known Issues / TODOs

### Fixed (this session)
- ✅ `AudioGenerator` missing `last_note` field — added to struct and initialized
- ✅ Massive duplicate doc comment in `rustdaw_project/src/lib.rs:48-95` — cleaned up
- ✅ Unused variables `is_kick`/`is_hat`/`is_synth` in test — removed duplicates, prefixed unused with `_`
- ✅ Removed duplicate `analyze_rms.rs` (identical to `analyze.rs`)
- ✅ Unused imports: `std::env`, `Ordering` (where not needed), `Producer`/`Consumer` (scoped to tests)
- ✅ Dead code: `damp` field in `CombFilter`, unused `mut out` in Reverb, unused `fs` params
- ✅ Test bug: `test_export_wav_detailed` used wrong step duration (31.25ms vs 125ms) — fixed to match `export_wav`
- ✅ **Sampler integration** — added `SamplerMap` to `AppState`, `load_sampler()` method, `AudioGenerator` now triggers samplers on sequencer steps (falls back to synthesized voice if no sampler loaded)
- ✅ **Project UI** — added Save/Load Project (JSON) and Export WAV buttons with file path text inputs in egui UI
- ✅ **Effects integration** — added `EffectChain` (LPF → Delay → Reverb) to audio thread, effect parameters in `AppState` synced per-sample, full GUI controls (enable toggle, LPF cutoff log slider, delay time/feedback/mix, reverb mix)
- ✅ **Polyphony** — added `VoicePool` with voice stealing (priority: finished > releasing > oldest), dynamic max voices (1-32), per-note tracking, GUI slider, real-time resize

### Build / System Dependencies
- **ALSA dev libraries required** for `audio` feature: `sudo apt install libasound2-dev pkg-config`

### Architecture / Missing Integrations
- **MIDI input**: Zero support
- **Multi-track**: `Mixer` supports multiple channels, but only 1 voice added
- **Piano roll**: Not integrated into UI
- Sampler only supports mono 32-bit float WAV
- No MIDI input support