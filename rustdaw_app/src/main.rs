//! Main application entry point.

use rustdaw_app::AppState;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::path::PathBuf;
use eframe::egui;

fn midi_note_to_name(midi: u8) -> String {
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (midi as i32 / 12) - 1;
    let note_idx = midi as i32 % 12;
    format!("{}{}", note_names[note_idx as usize], octave)
}

fn main() {
    let state = Arc::new(Mutex::new(AppState::new()));
    let state_clone = Arc::clone(&state);

    // Run the egui UI.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 750.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "RustDAW",
        options,
        Box::new(|cc| {
            // Configure fonts
            let mut fonts = egui::FontDefinitions::default();
            fonts.families.insert(
                egui::FontFamily::Proportional,
                vec!["Noto Sans".to_owned(), "DejaVu Sans".to_owned()],
            );
            fonts.families.insert(
                egui::FontFamily::Monospace,
                vec!["DejaVu Sans Mono".to_owned()],
            );
            cc.egui_ctx.set_fonts(fonts);
            
            // Dark theme tweaks
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals.window_fill = egui::Color32::from_rgb(28, 28, 32);
            style.visuals.panel_fill = egui::Color32::from_rgb(32, 32, 36);
            style.visuals.extreme_bg_color = egui::Color32::from_rgb(24, 24, 28);
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
            cc.egui_ctx.set_style(style);
            
            Ok(Box::new(MyApp::new(state_clone)))
        }),
    );
}

struct MyApp {
    state: Arc<Mutex<AppState>>,
    /// File path for save project.
    save_project_path: String,
    /// File path for load project.
    load_project_path: String,
    /// File path for export WAV.
    export_wav_path: String,
    /// Export duration in seconds.
    export_duration: f32,
    /// Selected MIDI note for sequencer input (C-2 = 36 to G8 = 103)
    selected_midi_note: u8,
}

impl MyApp {
    fn new(state: Arc<Mutex<AppState>>) -> Self {
        Self {
            state,
            save_project_path: "project.json".to_string(),
            load_project_path: "project.json".to_string(),
            export_wav_path: "export.wav".to_string(),
            export_duration: 4.0,
            selected_midi_note: 60, // Middle C (C4)
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Lock the state to get a mutable reference.
        let mut state = self.state.lock().unwrap();
        let playing = state.playing.load(Ordering::Relaxed);
        
        // Request repaint at ~60fps when playing for step indicator
        if playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // Top transport bar
        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                
                // Play/Stop button with visual feedback
                let play_text = if playing { "⏹  Stop" } else { "▶  Play" };
                let play_btn = egui::Button::new(play_text)
                    .fill(if playing { 
                        egui::Color32::from_rgb(200, 60, 60) 
                    } else { 
                        egui::Color32::from_rgb(60, 180, 90) 
                    })
                    .min_size(egui::vec2(100.0, 36.0))
                    .rounding(egui::Rounding::same(6.0));
                
                if ui.add(play_btn).clicked() {
                    state.playing.store(!playing, Ordering::Relaxed);
                    if !playing {
                        let _ = state.start_audio();
                    } else {
                        state.stop_audio();
                    }
                }
                
                ui.add_space(16.0);
                
                // BPM with tap tempo hint
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("BPM").size(11.0).color(egui::Color32::GRAY));
                    ui.add(
                        egui::DragValue::new(&mut state.bpm)
                            .range(30.0..=300.0)
                            .speed(0.5)
                            .suffix(" BPM")
                            .fixed_decimals(1)
                    );
                });
                
                ui.add_space(16.0);
                
                // Master volume with dB
                let vol_db = 20.0 * state.master_volume.max(0.0001).log10();
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!("VOL  {:.1} dB", vol_db)).size(11.0).color(egui::Color32::GRAY));
                    ui.add(
                        egui::Slider::new(&mut state.master_volume, 0.0..=1.0)
                            .text("")
                            .show_value(false)
                    );
                });
                
                ui.add_space(16.0);
                
                // Pattern info
                let step_duration = 60.0 / state.bpm / 16.0;
                let pattern_secs = step_duration * 16.0;
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("PATTERN").size(11.0).color(egui::Color32::GRAY));
                    ui.label(egui::RichText::new(format!("{:.2}s  ({} steps)", pattern_secs, 16)).monospace());
                });
            });
            
            ui.add_space(8.0);
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            
            // Two-column layout: left = sequencer + controls, right = visualizers
            ui.columns(2, |cols| {
                // LEFT COLUMN
                let left = &mut cols[0];
                
                // Step Sequencer Section
                            let mut show_sequencer = true;
                            // Extract self data to avoid borrow conflicts
                            let selected_note = self.selected_midi_note;
                            let _note_name = midi_note_to_name(selected_note);
                            egui::CollapsingHeader::new(
                                egui::RichText::new("🎵  Step Sequencer (16 steps)").size(16.0).strong()
                            )
                            .default_open(true)
                            .show(left, |ui| {
                                show_sequencer = ui.is_enabled(); // workaround since no header_response()
                                ui.add_space(4.0);
                
                                // Note selector row
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Note:").size(13.0));
                    
                                    let mut local_selected_note = selected_note;
                                    let note_name = midi_note_to_name(local_selected_note);
                                    egui::ComboBox::from_label("")
                                        .selected_text(note_name)
                                        .width(100.0)
                                        .show_ui(ui, |ui| {
                                            for midi in 36..=103 {
                                                let name = midi_note_to_name(midi);
                                                ui.selectable_value(&mut local_selected_note, midi as u8, name);
                                            }
                                        });
                    
                                    // Write back the potentially changed note
                                    self.selected_midi_note = local_selected_note;
                    
                                    // Velocity
                                    ui.add_space(8.0);
                                    ui.label("Vel:");
                                    ui.add(egui::DragValue::new(&mut 100).range(1..=127).speed(1).fixed_decimals(0));
                    
                                    ui.add_space(16.0);
                    
                                    // Clear pattern button
                                    if ui.button("🗑 Clear Pattern").clicked() {
                                        for step in &mut state.sequencer.steps {
                                            step.note = None;
                                        }
                                    }
                                });
                    
                    ui.add_space(8.0);
                    
                    // Step row with numbers
                    ui.horizontal(|ui| {
                        for i in 0..16 {
                            let step = &mut state.sequencer.steps[i];
                            let is_empty = step.is_empty();
                            
                            let btn_text = if is_empty { "─".to_string() } else {
                                format!("{:02}", step.note.unwrap().note)
                            };
                            
                            let btn_color = if is_empty {
                                egui::Color32::from_gray(50)
                            } else {
                                egui::Color32::from_rgb(0, 180, 100)
                            };
                            
                            let resp = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(btn_text).size(14.0).strong()
                                )
                                .fill(btn_color)
                                .min_size(egui::vec2(40.0, 44.0))
                                .rounding(egui::Rounding::same(4.0))
                            ).on_hover_text(format!("Step {}  •  Left-click: toggle  •  Right-click: clear", i + 1));
                            
                            if resp.clicked() {
                                if step.is_empty() {
                                    step.note = Some(rustdaw_sequencer::Note::new(self.selected_midi_note, 100));
                                } else {
                                    step.note = None;
                                }
                            }
                            
                            // Right-click to clear
                            if resp.secondary_clicked() {
                                step.note = None;
                            }
                            
                            // Step number below button
                            ui.vertical_centered(|ui| {
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(format!("{}", i + 1)).size(10.0).color(egui::Color32::GRAY));
                            });
                        }
                    });
                    
                    ui.add_space(4.0);
                    ui.separator();
                    
                    // Quick pattern buttons
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Quick patterns:").size(11.0).color(egui::Color32::GRAY));
                        
                        if ui.button("🥁 Basic Beat").clicked() {
                            let pattern = [
                                (0, 36), (4, 36), (8, 36), (12, 36),  // Kick
                                (2, 42), (6, 42), (10, 42), (14, 42), // Hat
                            ];
                            for step in &mut state.sequencer.steps {
                                step.note = None;
                            }
                            for (idx, note) in pattern {
                                state.sequencer.steps[idx].note = Some(rustdaw_sequencer::Note::new(note, 100));
                            }
                        }
                        
                        if ui.button("🎹 Four on Floor").clicked() {
                            for step in &mut state.sequencer.steps {
                                step.note = None;
                            }
                            for i in 0..16 {
                                if i % 4 == 0 {
                                    state.sequencer.steps[i].note = Some(rustdaw_sequencer::Note::new(36, 100));
                                }
                            }
                        }
                        
                        if ui.button("🎵 House").clicked() {
                            for step in &mut state.sequencer.steps {
                                step.note = None;
                            }
                            let pattern = [
                                (0, 36), (4, 36), (8, 36), (12, 36),  // Kick
                                (2, 42), (6, 42), (10, 42), (14, 42), // Hat
                                (4, 60), (12, 60),                     // Synth
                            ];
                            for (idx, note) in pattern {
                                state.sequencer.steps[idx].note = Some(rustdaw_sequencer::Note::new(note, 100));
                            }
                        }
                    });
                });
                
                left.add_space(8.0);
                
                // Master Section
                egui::CollapsingHeader::new(
                    egui::RichText::new("🔊  Master").size(16.0).strong()
                )
                .default_open(true)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Volume").size(11.0).color(egui::Color32::GRAY));
                            let vol_db = 20.0 * state.master_volume.max(0.0001).log10();
                            ui.label(egui::RichText::new(format!("{:.1} dB", vol_db)).size(18.0).monospace().color(
                                if state.master_volume > 0.9 { egui::Color32::from_rgb(255, 100, 100) } 
                                else if state.master_volume > 0.7 { egui::Color32::from_rgb(255, 200, 80) }
                                else { egui::Color32::WHITE }
                            ));
                        });
                        
                        ui.add_space(16.0);
                        
                        ui.add(
                            egui::Slider::new(&mut state.master_volume, 0.0..=1.0)
                                .text("")
                                .show_value(false)
                        );
                    });
                    
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Pan Law:").size(11.0).color(egui::Color32::GRAY));
                        ui.label(egui::RichText::new("-3 dB (center)").monospace());
                    });
                });
                
                left.add_space(8.0);
                
                // Project Section
                egui::CollapsingHeader::new(
                    egui::RichText::new("💾  Project").size(16.0).strong()
                )
                .default_open(true)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Save:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.save_project_path).desired_width(200.0));
                        if ui.button("💾 Save").clicked() {
                            let path = PathBuf::from(&self.save_project_path);
                            if let Err(e) = state.save_project(&path) {
                                eprintln!("Failed to save project: {}", e);
                            }
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Load:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.load_project_path).desired_width(200.0));
                        if ui.button("📂 Load").clicked() {
                            let path = PathBuf::from(&self.load_project_path);
                            if let Err(e) = state.load_project(&path) {
                                eprintln!("Failed to load project: {}", e);
                            }
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Export:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.export_wav_path).desired_width(160.0));
                        ui.add(egui::DragValue::new(&mut self.export_duration)
                            .range(1.0..=300.0)
                            .speed(0.5)
                            .suffix(" s")
                            .fixed_decimals(1));
                        if ui.button("🎧 Export WAV").clicked() {
                            let path = PathBuf::from(&self.export_wav_path);
                            if let Err(e) = state.export_wav(&path, self.export_duration) {
                                eprintln!("Failed to export WAV: {}", e);
                            }
                        }
                    });
                });
                
                // Polyphony Section
                egui::CollapsingHeader::new(
                    egui::RichText::new("🎹  Polyphony").size(16.0).strong()
                )
                .default_open(false)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Max Voices:").size(13.0));
                        ui.add(
                            egui::Slider::new(&mut state.max_voices, 1..=32)
                                .text("")
                        );
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Active:").size(13.0));
                        ui.label(egui::RichText::new(if state.playing.load(Ordering::Relaxed) { "🟢 Running" } else { "⚪ Stopped" }).size(13.0));
                    });
                }); // End of Polyphony CollapsingHeader
            
            // RIGHT COLUMN (inside the same columns closure)
            let right = &mut cols[1];
            
            // Effects Section
            egui::CollapsingHeader::new(
                egui::RichText::new("🎛  Effects Chain").size(16.0).strong()
            )
            .default_open(true)
            .show(right, |ui| {
                ui.add_space(4.0);
                
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.effect_enabled, "Enable");
                    if state.effect_enabled {
                        ui.label(egui::RichText::new("ON").color(egui::Color32::from_rgb(60, 180, 90)).strong());
                    } else {
                        ui.label(egui::RichText::new("OFF").color(egui::Color32::GRAY));
                    }
                });
                
                ui.add_space(8.0);
                
                // LPF
                egui::CollapsingHeader::new("🎚  Low-Pass Filter")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Cutoff:");
                            ui.add(
                                egui::Slider::new(&mut state.effect_lpf_cutoff, 20.0..=20000.0)
                                    .logarithmic(true)
                                    .suffix(" Hz")
                            );
                        });
                    });
                
                ui.add_space(4.0);
                
                // Delay
                egui::CollapsingHeader::new("⏱  Delay")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Time:");
                            ui.add(
                                egui::Slider::new(&mut state.effect_delay_time, 0.0..=2.0)
                                    .suffix(" s")
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Feedback:");
                            ui.add(
                                egui::Slider::new(&mut state.effect_delay_feedback, 0.0..=0.95)
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Mix:");
                            ui.add(
                                egui::Slider::new(&mut state.effect_delay_mix, 0.0..=1.0)
                            );
                        });
                    });
                
                ui.add_space(4.0);
                
                // Reverb
                egui::CollapsingHeader::new("🏛  Reverb")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Mix:");
                            ui.add(
                                egui::Slider::new(&mut state.effect_reverb_mix, 0.0..=1.0)
                            );
                        });
                    });
            });
            
            right.add_space(8.0);
            
            // Visualizers Section
            egui::CollapsingHeader::new(
                egui::RichText::new("📊  Visualizers").size(16.0).strong()
            )
            .default_open(true)
            .show(right, |ui| {
                ui.add_space(4.0);
                
                // Oscilloscope
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Oscilloscope").size(13.0).strong());
                });
                let _ = state.oscilloscope.borrow_mut().ui(ui);
                
                ui.add_space(12.0);
                
                // Spectrum Analyzer
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Spectrum Analyzer").size(13.0).strong());
                });
                let _ = state.spectrum_analyzer.borrow_mut().ui(ui);
                
                ui.add_space(12.0);
                
                // VU Meter
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("VU Meter").size(13.0).strong());
                });
                let _ = state.vu_meter.borrow_mut().ui(ui);
            });
        }); // End of ui.columns(2, |cols| { ... }) closure
        
        }); // End of egui::CentralPanel::default().show()
        
        // Status bar at bottom
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // Playing indicator
                let playing = state.playing.load(Ordering::Relaxed);
                if playing {
                    ui.label(egui::RichText::new("● PLAYING").color(egui::Color32::from_rgb(60, 180, 90)).strong());
                } else {
                    ui.label(egui::RichText::new("● STOPPED").color(egui::Color32::GRAY));
                }
                
                ui.separator();
                
                // CPU/Performance hint
                ui.label(egui::RichText::new(format!("Voices: {} / {}", 0, state.max_voices)).size(11.0).color(egui::Color32::GRAY));
                
                ui.separator();
                
                // Version
                ui.label(egui::RichText::new("RustDAW v0.1.0").size(11.0).color(egui::Color32::GRAY));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("Nous Research").size(11.0).color(egui::Color32::GRAY));
                });
            });
            
            ui.add_space(2.0);
        });
    }
}