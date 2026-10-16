# RustDAW

An open-source Digital Audio Workstation written in Rust.

## Features

- Audio engine with oscillators, ADSR envelopes, and mixing
- Step sequencer (16 steps) and piano roll
- Sampler for loading and playing .wav files with pitch shifting
- Audio effects: low-pass filter, delay, reverb
- Audio output via CPAL and UI via egui
- Project saving/loading (JSON) and WAV export
- Audio visualizer (oscilloscope, spectrum analyzer, VU meter)

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release -p rustdaw_app
```

## License

MIT or Apache-2.0
