//! Main application entry point.

use eframe::egui;
use rustdaw_app::AppState;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    // Initialize logging.
    let _ = env_logger::init();

    let state = Arc::new(AppState::new());
    let state_clone = Arc::clone(&state);

    // Start audio thread (if we had audio hardware).
    // In this server environment, we won't actually start the audio thread because
    // there's no audio output, but we'll leave the code here for completeness.
    /*
    thread::spawn(move || {
        // This is a simplified audio thread using cpal.
        // In a real implementation, we would use a lock-free ringbuffer.
        // For now, we just sleep to simulate work.
        loop {
            thread::sleep(Duration::from_millis(10));
            // Generate audio samples and output via cpal would go here.
        }
    });
    */

    // Run the egui UI.
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RustDAW",
        options,
        Box::new(|cc| {
            // We'll pass the state to the UI via a closure.
            Box::new(MyApp::new(state_clone))
        }),
    );
}

struct MyApp {
    state: Arc<AppState>,
}

impl MyApp {
    fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RustDAW");
            ui.separator();

            // Play/stop button.
            let playing = self.state.playing.load(Ordering::Relaxed);
            if ui.button(if playing { "Stop" } else { "Play" }).clicked() {
                self.state
                    .playing
                    .store(!playing, Ordering::Relaxed);
            }

            ui.add_space(10.0);

            // BPM control.
            ui.label(format!("BPM: {:.1}", self.state.bpm));
            ui.add(
                egui::Slider::new(&mut self.state.bpm, 30.0..=300.0)
                    .text("Beats per minute"),
            );

            ui.add_space(10.0);

            // Step sequencer display.
            ui.label("Step Sequencer (16 steps)");
            egui::Grid::new("sequencer_grid")
                .num_columns(16)
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for i in 0..16 {
                        let step = &self.state.sequencer.steps[i];
                        let is_empty = step.is_empty();
                        let color = if is_empty {
                            egui::Color32::from_gray(50)
                        } else {
                            egui::Color32::from_rgb(0, 150, 255)
                        };
                        ui.colored_label(color, format!("{}", i + 1));
                        ui.end_row();
                    }
                });

            ui.add_space(20.0);

            // Master volume.
            ui.label(format!("Master Volume: {:.0}%", self.state.master_volume * 100.0));
            ui.add(
                egui::Slider::new(&mut self.state.master_volume, 0.0..=1.0)
                    .text("Volume"),
            );

            ui.add_space(20.0);

            // Visualizer section.
            ui.heading("Visualizer");
            ui.separator();

            // Oscilloscope.
            ui.label("Oscilloscope");
            let _ = self.state.oscilloscope.ui(ui);
            ui.add_space(10.0);

            // Spectrum analyzer.
            ui.label("Spectrum Analyzer");
            let _ = self.state.spectrum_analyzer.ui(ui);
            ui.add_space(10.0);

            // VU meter.
            ui.label("VU Meter");
            let _ = self.state.vu_meter.ui(ui);
        });
    }
}
