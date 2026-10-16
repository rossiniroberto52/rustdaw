//! Main application entry point.

use eframe::egui;
use rustdaw_app::AppState;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

fn main() {
    // Initialize logging.
    // let _ = env_logger::init();

    let state = Arc::new(Mutex::new(AppState::new()));
    let state_clone = Arc::clone(&state);

    // Run the egui UI.
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RustDAW",
        options,
        Box::new(|cc| {
            // We'll pass the state to the UI via a closure.
            Ok(Box::new(MyApp::new(state_clone)))
        }),
    );
}

struct MyApp {
    state: Arc<Mutex<AppState>>,
}

impl MyApp {
    fn new(state: Arc<Mutex<AppState>>) -> Self {
        Self { state }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Lock the state to get a mutable reference.
        let mut state = self.state.lock().unwrap();

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RustDAW");
            ui.separator();

            // Play/stop button.
            let playing = state.playing.load(Ordering::Relaxed);
            if ui.button(if playing { "Stop" } else { "Play" }).clicked() {
                state.playing.store(!playing, Ordering::Relaxed);
            }

            ui.add_space(10.0);

            // BPM control.
            ui.label(format!("BPM: {:.1}", state.bpm));
            ui.add(
                eframe::egui::Slider::new(&mut state.bpm, 30.0..=300.0)
                    .text("Beats per minute"),
            );

            ui.add_space(10.0);

            // Step sequencer display.
            ui.label("Step Sequencer (16 steps)");
            eframe::egui::Grid::new("sequencer_grid")
                .num_columns(16)
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for i in 0..16 {
                        let step = &state.sequencer.steps[i];
                        let is_empty = step.is_empty();
                        let color = if is_empty {
                            eframe::egui::Color32::from_gray(50)
                        } else {
                            eframe::egui::Color32::from_rgb(0, 150, 255)
                        };
                        ui.colored_label(color, format!("{}", i + 1));
                        ui.end_row();
                    }
                });

            ui.add_space(20.0);

            // Master volume.
            ui.label(format!("Master Volume: {:.0}%", state.master_volume * 100.0));
            ui.add(
                eframe::egui::Slider::new(&mut state.master_volume, 0.0..=1.0)
                    .text("Volume"),
            );

            ui.add_space(20.0);

            // Visualizer section.
            ui.heading("Visualizer");
            ui.separator();

            // Oscilloscope.
            ui.label("Oscilloscope");
            let _ = state.oscilloscope.borrow_mut().ui(ui);
            ui.add_space(10.0);

            // Spectrum analyzer.
            ui.label("Spectrum Analyzer");
            let _ = state.spectrum_analyzer.borrow_mut().ui(ui);
            ui.add_space(10.0);

            // VU meter.
            ui.label("VU Meter");
            let _ = state.vu_meter.borrow_mut().ui(ui);
        });
    }
}
