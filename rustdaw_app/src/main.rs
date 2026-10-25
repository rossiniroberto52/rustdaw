//! Main application entry point.

use rustdaw_app::{AppState, UiState};
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 650.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "RustDAW",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.families.insert(egui::FontFamily::Proportional, vec!["Noto Sans".to_owned(), "DejaVu Sans".to_owned()]);
            fonts.families.insert(egui::FontFamily::Monospace, vec!["DejaVu Sans Mono".to_owned()]);
            cc.egui_ctx.set_fonts(fonts);
            
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
    save_project_path: String,
    load_project_path: String,
    export_wav_path: String,
    export_duration: f32,
    selected_midi_note: u8,
    selected_velocity: u8,
    sampler_load_path: String,
    sampler_target_note: u8,
    clipboard_pattern: Option<[rustdaw_sequencer::Step; 16]>,
    show_shortcuts: bool,
}

impl MyApp {
    fn new(state: Arc<Mutex<AppState>>) -> Self {
        Self {
            state,
            save_project_path: "project.json".to_string(),
            load_project_path: "project.json".to_string(),
            export_wav_path: "export.wav".to_string(),
            export_duration: 4.0,
            selected_midi_note: 60,
            selected_velocity: 100,
            sampler_load_path: "".to_string(),
            sampler_target_note: 36,
            clipboard_pattern: None,
            show_shortcuts: false,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Get UI state from AppState (brief lock)
        let mut ui_state;
        let playing;
        let current_step;
        
        {
            let state = self.state.lock().unwrap();
            playing = state.playing.load(Ordering::Relaxed);
            
            current_step = if playing {
                let time = ctx.input(|i| i.time);
                let step_duration = 60.0 / state.bpm / state.pattern_length as f32;
                ((step_duration).recip() * time as f32 * state.pattern_length as f32) as usize % state.pattern_length as usize
            } else {
                usize::MAX
            };
            
            ui_state = state.ui_state();
        }
        
        if playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // Draw all UI components with the cloned UI state
        self.draw_transport_bar(ctx, &mut ui_state, playing, current_step);
        self.draw_main_content(ctx, &mut ui_state, playing, current_step);
        self.draw_status_bar(ctx, &mut ui_state, playing, current_step);
        
        if self.show_shortcuts {
            self.show_shortcuts_window(ctx);
        }
        
        // Write back modified UI state to AppState
        self.state.lock().unwrap().apply_ui_state(&ui_state);
    }
}

impl MyApp {
    fn draw_transport_bar(&mut self, ctx: &egui::Context, state: &mut UiState, playing: bool, _current_step: usize) {
        egui::TopBottomPanel::top("transport_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                
                let loop_btn = egui::Button::new(if state.loop_enabled { "🔁 Loop ON" } else { "🔁 Loop OFF" })
                    .fill(if state.loop_enabled { egui::Color32::from_rgb(60, 180, 90) } else { egui::Color32::from_gray(60) })
                    .min_size(egui::vec2(90.0, 32.0))
                    .rounding(egui::Rounding::same(4.0));
                if ui.add(loop_btn).clicked() { 
                    self.state.lock().unwrap().loop_enabled = !state.loop_enabled; 
                }
                
                ui.add_space(8.0);
                
                let rec_btn = egui::Button::new(if state.record_armed { "⏺  Rec Armed" } else { "⏺  Record" })
                    .fill(if state.record_armed { egui::Color32::from_rgb(200, 60, 60) } else { egui::Color32::from_gray(60) })
                    .min_size(egui::vec2(90.0, 32.0))
                    .rounding(egui::Rounding::same(4.0));
                if ui.add(rec_btn).clicked() { 
                    self.state.lock().unwrap().record_armed = !state.record_armed; 
                }
                
                ui.add_space(12.0);
                
                let play_text = if playing { "⏹  Stop" } else { "▶  Play" };
                let play_btn = egui::Button::new(play_text)
                    .fill(if playing { egui::Color32::from_rgb(200, 60, 60) } else { egui::Color32::from_rgb(60, 180, 90) })
                    .min_size(egui::vec2(100.0, 36.0))
                    .rounding(egui::Rounding::same(6.0));
                if ui.add(play_btn).clicked() {
                    let state = self.state.lock().unwrap();
                    state.playing.store(!playing, Ordering::Relaxed);
                    if !playing { let _ = state.start_audio(); } else { state.stop_audio(); }
                }
                
                ui.add_space(16.0);
                
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.metronome_enabled, "🔊 Metronome");
                    if ui.button("⏺").on_hover_text("Toggle Metronome").clicked() {
                        self.state.lock().unwrap().metronome_enabled = !state.metronome_enabled;
                    }
                    ui.add(egui::Slider::new(&mut state.metronome_volume, 0.0..=1.0).text("Vol").suffix("x"));
                });
                
                ui.add_space(16.0);
                
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("BPM").size(11.0).color(egui::Color32::GRAY));
                    ui.add(egui::DragValue::new(&mut state.bpm).range(30.0..=300.0).speed(0.5).suffix(" BPM").fixed_decimals(1));
                });
                
                ui.add_space(12.0);
                
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Swing").size(11.0).color(egui::Color32::GRAY));
                    ui.add(egui::Slider::new(&mut state.swing_amount, 0.0..=0.5).text("").fixed_decimals(2));
                });
                
                ui.add_space(12.0);
                
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Length").size(11.0).color(egui::Color32::GRAY));
                    ui.add(egui::DragValue::new(&mut state.pattern_length).range(4..=64).speed(1).suffix(" steps"));
                });
                
                ui.add_space(16.0);
                
                let vol_db = 20.0 * state.master_volume.max(0.0001).log10();
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(format!("VOL  {:.1} dB", vol_db)).size(11.0).color(egui::Color32::GRAY));
                    ui.add(egui::Slider::new(&mut state.master_volume, 0.0..=1.0).text("").show_value(false));
                });
                
                ui.add_space(16.0);
                
                let step_duration = 60.0 / state.bpm / state.pattern_length as f32;
                let pattern_secs = step_duration * state.pattern_length as f32;
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("PATTERN").size(11.0).color(egui::Color32::GRAY));
                    ui.label(egui::RichText::new(format!("{:.2}s  ({} steps)", pattern_secs, state.pattern_length)).monospace());
                });
            });
            
            ui.add_space(8.0);
        });
    }
    
    fn draw_main_content(&mut self, ctx: &egui::Context, state: &mut UiState, playing: bool, current_step: usize) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            
            // Use two panels side by side with a separator
            let available_width = ui.available_width();
            
            ui.horizontal(|ui| {
                // Left panel
                ui.allocate_ui_with_layout(egui::vec2(available_width * 0.55, ui.available_height()), egui::Layout::top_down(egui::Align::LEFT), |left| {
                    self.draw_left_column(left, state, playing, current_step);
                });
                
                ui.separator();
                
                // Right panel
                                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), ui.available_height()), egui::Layout::top_down(egui::Align::LEFT), |right| {
                                    self.draw_right_column(right, state);
                                });
            });
        });
    }
    
    fn draw_left_column(&mut self, left: &mut egui::Ui, state: &mut UiState, _playing: bool, current_step: usize) {
        // Step Sequencer
        egui::CollapsingHeader::new(egui::RichText::new("🎵  Step Sequencer").size(16.0).strong())
            .default_open(true)
            .show(left, |ui| {
                ui.add_space(4.0);
                
                // Note selector
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Note:").size(13.0));
                    let mut local_note = self.selected_midi_note;
                    let note_name = midi_note_to_name(local_note);
                    egui::ComboBox::from_label("").selected_text(note_name).width(100.0)
                        .show_ui(ui, |ui| {
                            for midi in 36..=103 {
                                let name = midi_note_to_name(midi);
                                ui.selectable_value(&mut local_note, midi as u8, name);
                            }
                        });
                    self.selected_midi_note = local_note;
                    
                    ui.add_space(8.0);
                    ui.label("Vel:");
                    ui.add(egui::DragValue::new(&mut self.selected_velocity).range(1..=127).speed(1).fixed_decimals(0));
                    ui.add_space(16.0);
                    
                    if ui.button("🗑 Clear Pattern").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                    }
                    
                    ui.add_space(8.0);
                    
                    if ui.button("⏪ Shift Left").clicked() {
                        let mut new_steps = [const { rustdaw_sequencer::Step { note: None } }; 16];
                        for i in 0..state.pattern_length as usize {
                            let src = (i + 1) % state.pattern_length as usize;
                            new_steps[i] = state.sequencer.steps[src];
                        }
                        for i in 0..state.pattern_length as usize { state.sequencer.steps[i] = new_steps[i]; }
                    }
                    
                    if ui.button("⏩ Shift Right").clicked() {
                        let mut new_steps = [const { rustdaw_sequencer::Step { note: None } }; 16];
                        for i in 0..state.pattern_length as usize {
                            let src = (i + state.pattern_length as usize - 1) % state.pattern_length as usize;
                            new_steps[i] = state.sequencer.steps[src];
                        }
                        for i in 0..state.pattern_length as usize { state.sequencer.steps[i] = new_steps[i]; }
                    }
                    
                    if ui.button("📋 Copy").clicked() {
                        self.clipboard_pattern = Some(state.sequencer.steps);
                    }
                    if ui.button("📄 Paste").clicked() {
                        if let Some(pattern) = self.clipboard_pattern {
                            let max_idx = (state.pattern_length as usize).min(pattern.len());
                            for i in 0..max_idx {
                                state.sequencer.steps[i] = pattern[i];
                            }
                        }
                    }
                    
                    if ui.button("↩ Undo").clicked() {
                        // TODO: implement undo
                    }
                });
                
                ui.add_space(8.0);
                
                // Step row
                ui.horizontal(|ui| {
                    for i in 0..state.pattern_length as usize {
                        let step = &mut state.sequencer.steps[i];
                        let is_empty = step.is_empty();
                        let is_current = i == current_step;
                        
                        let btn_text: String = if is_empty { "─".to_string() } else { format!("{:02}", step.note.unwrap().note) };
                        
                        let btn_color = if is_current { egui::Color32::from_rgb(100, 220, 120) }
                            else if is_empty { egui::Color32::from_gray(50) }
                            else { egui::Color32::from_rgb(0, 180, 100) };
                        
                        let resp = ui.add(
                            egui::Button::new(egui::RichText::new(btn_text).size(14.0).strong())
                                .fill(btn_color)
                                .min_size(egui::vec2(38.0, 44.0))
                                .rounding(egui::Rounding::same(4.0))
                        ).on_hover_text(format!("Step {}  •  L-click: toggle  •  R-click: clear  •  Vel: {}", i + 1, step.note.map(|n| n.velocity).unwrap_or(0)));
                        
                        if resp.clicked() {
                            if step.is_empty() {
                                step.note = Some(rustdaw_sequencer::Note::new(self.selected_midi_note, self.selected_velocity));
                            } else {
                                step.note = None;
                            }
                        }
                        if resp.secondary_clicked() { step.note = None; }
                        
                        ui.vertical_centered(|ui| {
                            ui.add_space(2.0);
                            let step_label = if (i + 1) % 4 == 1 { format!("{:>2}", i + 1) } else { format!("{}", i + 1) };
                            ui.label(egui::RichText::new(step_label).size(9.0).color(if (i + 1) % 4 == 1 { egui::Color32::WHITE } else { egui::Color32::GRAY }));
                        });
                    }
                });
                
                ui.add_space(4.0);
                ui.separator();
                
                // Quick patterns
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Quick patterns:").size(11.0).color(egui::Color32::GRAY));
                    
                    if ui.button("🥁 Basic Beat").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                        let pattern = [(0, 36), (4, 36), (8, 36), (12, 36), (2, 42), (6, 42), (10, 42), (14, 42)];
                        for (idx, note) in pattern { if idx < state.pattern_length as usize { state.sequencer.steps[idx].note = Some(rustdaw_sequencer::Note::new(note, 100)); } }
                    }
                    if ui.button("🎹 Four on Floor").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                        for i in 0..state.pattern_length as usize { if i % 4 == 0 { state.sequencer.steps[i].note = Some(rustdaw_sequencer::Note::new(36, 100)); } }
                    }
                    if ui.button("🎵 House").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                        let pattern = [(0, 36), (4, 36), (8, 36), (12, 36), (2, 42), (6, 42), (10, 42), (14, 42), (4, 60), (12, 60)];
                        for (idx, note) in pattern { if idx < state.pattern_length as usize { state.sequencer.steps[idx].note = Some(rustdaw_sequencer::Note::new(note, 100)); } }
                    }
                    if ui.button("🎹 Techno").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                        for i in 0..state.pattern_length as usize { if i % 2 == 0 { state.sequencer.steps[i].note = Some(rustdaw_sequencer::Note::new(36, 100)); } }
                    }
                    if ui.button("🎼 Breakbeat").clicked() {
                        for step in &mut state.sequencer.steps { step.note = None; }
                        let pattern = [(0, 36), (2, 36), (6, 36), (10, 36), (14, 36), (3, 42), (7, 42), (11, 42), (15, 42)];
                        for (idx, note) in pattern { if idx < state.pattern_length as usize { state.sequencer.steps[idx].note = Some(rustdaw_sequencer::Note::new(note, 100)); } }
                    }
                });
            });
            
            left.add_space(8.0);
            
            // Sampler Section
            egui::CollapsingHeader::new(egui::RichText::new("🎧  Sampler").size(16.0).strong())
                .default_open(true)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Load Sample:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.sampler_load_path).desired_width(200.0));
                        if ui.button("📂 Browse").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).pick_file() {
                                self.sampler_load_path = path.to_string_lossy().to_string();
                            }
                        }
                        if ui.button("➕ Load").clicked() && !self.sampler_load_path.is_empty() {
                            let path = std::path::PathBuf::from(&self.sampler_load_path);
                            let target_note = self.sampler_target_note;
                            if let Err(e) = self.state.lock().unwrap().load_sampler(target_note, &path) {
                                eprintln!("Failed to load sampler: {}", e);
                            } else { self.sampler_load_path.clear(); }
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("MIDI Note:").size(13.0));
                        let mut local_note = self.sampler_target_note;
                        let note_name = midi_note_to_name(local_note);
                        egui::ComboBox::from_label("").selected_text(note_name).width(100.0)
                            .show_ui(ui, |ui| {
                                for midi in 36..=103 {
                                    let name = midi_note_to_name(midi);
                                    ui.selectable_value(&mut local_note, midi as u8, name);
                                }
                            });
                        self.sampler_target_note = local_note;
                    });
                    
                    // Get loaded samplers from AppState
                    let loaded_notes: Vec<u8> = {
                        let state = self.state.lock().unwrap();
                        state.samplers.keys().copied().collect()
                    };
                    
                    if !loaded_notes.is_empty() {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.label(egui::RichText::new("Loaded Samples:").size(11.0).color(egui::Color32::GRAY));
                        for note in &loaded_notes {
                            ui.horizontal(|ui| {
                                ui.label(format!("Note {} ({})", note, midi_note_to_name(*note)));
                            });
                        }
                    }
                });
            
            left.add_space(8.0);
            
            // Master Section
            egui::CollapsingHeader::new(egui::RichText::new("🔊  Master").size(16.0).strong())
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
                        ui.add(egui::Slider::new(&mut state.master_volume, 0.0..=1.0).text("").show_value(false));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Pan Law:").size(11.0).color(egui::Color32::GRAY));
                        ui.label(egui::RichText::new("-3 dB (center)").monospace());
                    });
                });
            
            left.add_space(8.0);
            
            // Project Section
            egui::CollapsingHeader::new(egui::RichText::new("💾  Project").size(16.0).strong())
                .default_open(true)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Save:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.save_project_path).desired_width(200.0));
                        if ui.button("💾 Save").clicked() {
                            let path = PathBuf::from(&self.save_project_path);
                            if let Err(e) = self.state.lock().unwrap().save_project(&path) { eprintln!("Failed to save project: {}", e); }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Load:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.load_project_path).desired_width(200.0));
                        if ui.button("📂 Load").clicked() {
                            let path = PathBuf::from(&self.load_project_path);
                            if let Err(e) = self.state.lock().unwrap().load_project(&path) { eprintln!("Failed to load project: {}", e); }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Export:").size(13.0));
                        ui.add(egui::TextEdit::singleline(&mut self.export_wav_path).desired_width(160.0));
                        ui.add(egui::DragValue::new(&mut self.export_duration).range(1.0..=300.0).speed(0.5).suffix(" s").fixed_decimals(1));
                        if ui.button("🎧 Export WAV").clicked() {
                            let path = PathBuf::from(&self.export_wav_path);
                            let duration = self.export_duration;
                            if let Err(e) = self.state.lock().unwrap().export_wav(&path, duration) { eprintln!("Failed to export WAV: {}", e); }
                        }
                    });
                });
            
            // Polyphony
            egui::CollapsingHeader::new(egui::RichText::new("🎹  Polyphony").size(16.0).strong())
                .default_open(false)
                .show(left, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Max Voices:").size(13.0));
                        ui.add(egui::Slider::new(&mut state.max_voices, 1..=32).text(""));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Active:").size(13.0));
                        let playing = self.state.lock().unwrap().playing.load(Ordering::Relaxed);
                        ui.label(egui::RichText::new(if playing { "🟢 Running" } else { "⚪ Stopped" }).size(13.0));
                    });
                });
    }
    
    fn draw_right_column(&mut self, right: &mut egui::Ui, state: &mut UiState) {
        // Effects Section
        egui::CollapsingHeader::new(egui::RichText::new("🎛  Effects Chain").size(16.0).strong())
            .default_open(true)
            .show(right, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.effect_enabled, "Enable");
                    if state.effect_enabled { ui.label(egui::RichText::new("ON").color(egui::Color32::from_rgb(60, 180, 90)).strong()); }
                    else { ui.label(egui::RichText::new("OFF").color(egui::Color32::GRAY)); }
                });
                ui.add_space(8.0);
                
                egui::CollapsingHeader::new("🎚  Low-Pass Filter").default_open(true).show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| { ui.label("Cutoff:"); ui.add(egui::Slider::new(&mut state.effect_lpf_cutoff, 20.0..=20000.0).logarithmic(true).suffix(" Hz")); });
                    ui.horizontal(|ui| { ui.label("Resonance:"); ui.add(egui::Slider::new(&mut state.effect_lpf_resonance, 0.0..=10.0).suffix(" dB")); });
                });
                ui.add_space(4.0);
                
                egui::CollapsingHeader::new("⏱  Delay").default_open(true).show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| { ui.label("Time:"); ui.add(egui::Slider::new(&mut state.effect_delay_time, 0.0..=2.0).suffix(" s")); });
                    ui.horizontal(|ui| { ui.label("Feedback:"); ui.add(egui::Slider::new(&mut state.effect_delay_feedback, 0.0..=0.95)); });
                    ui.horizontal(|ui| { ui.label("Mix:"); ui.add(egui::Slider::new(&mut state.effect_delay_mix, 0.0..=1.0)); });
                    ui.horizontal(|ui| { ui.label("Sync:"); ui.checkbox(&mut state.effect_delay_sync, "Tempo Sync"); });
                });
                ui.add_space(4.0);
                
                egui::CollapsingHeader::new("🏛  Reverb").default_open(true).show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| { ui.label("Mix:"); ui.add(egui::Slider::new(&mut state.effect_reverb_mix, 0.0..=1.0)); });
                    ui.horizontal(|ui| { ui.label("Room Size:"); ui.add(egui::Slider::new(&mut state.effect_reverb_room, 0.0..=1.0)); });
                    ui.horizontal(|ui| { ui.label("Dampening:"); ui.add(egui::Slider::new(&mut state.effect_reverb_damp, 0.0..=1.0)); });
                });
            });
            
            right.add_space(8.0);
            
            // Visualizers
            egui::CollapsingHeader::new(egui::RichText::new("📊  Visualizers").size(16.0).strong())
                .default_open(true)
                .show(right, |ui| {
                    ui.add_space(4.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Oscilloscope").size(13.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut state.oscilloscope_grid, "Grid");
                        });
                    });
                    // Draw oscilloscope from AppState
                    {
                        let state = self.state.lock().unwrap();
                        let _ = state.oscilloscope.borrow_mut().ui(ui);
                    }
                    
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Spectrum Analyzer").size(13.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut state.spectrum_log_freq, "Log Freq");
                            ui.checkbox(&mut state.spectrum_db_scale, "dB Scale");
                        });
                    });
                    // Draw spectrum analyzer from AppState
                    {
                        let state = self.state.lock().unwrap();
                        let _ = state.spectrum_analyzer.borrow_mut().ui(ui);
                    }
                    
                    ui.add_space(12.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("VU Meter").size(13.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut state.vu_peak_hold, "Peak Hold");
                        });
                    });
                    // Draw VU meter from AppState
                    {
                        let state = self.state.lock().unwrap();
                        let _ = state.vu_meter.borrow_mut().ui(ui);
                    }
                });
    }
    
    fn draw_status_bar(&mut self, ctx: &egui::Context, state: &mut UiState, playing: bool, current_step: usize) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if playing { ui.label(egui::RichText::new("● PLAYING").color(egui::Color32::from_rgb(60, 180, 90)).strong()); }
                else { ui.label(egui::RichText::new("● STOPPED").color(egui::Color32::GRAY)); }
                
                ui.separator();
                
                if current_step != usize::MAX {
                    ui.label(egui::RichText::new(format!("Step: {}/{}", current_step + 1, state.pattern_length)).size(11.0).color(egui::Color32::from_rgb(60, 180, 90)).strong());
                    ui.separator();
                }
                
                ui.label(egui::RichText::new(format!("Voices: {} / {}", 0, state.max_voices)).size(11.0).color(egui::Color32::GRAY));
                ui.separator();
                ui.label(egui::RichText::new("RustDAW v0.2.0").size(11.0).color(egui::Color32::GRAY));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⌨").on_hover_text("Keyboard Shortcuts").clicked() { self.show_shortcuts = !self.show_shortcuts; }
                    ui.label(egui::RichText::new("Nous Research").size(11.0).color(egui::Color32::GRAY));
                });
            });
            ui.add_space(2.0);
        });
    }
    
    fn show_shortcuts_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Keyboard Shortcuts")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                let shortcuts = [
                    ("Space", "Play / Stop"),
                    ("L", "Toggle Loop"),
                    ("R", "Toggle Record Arm"),
                    ("M", "Toggle Metronome"),
                    ("← / →", "Shift Pattern Left/Right"),
                    ("C / V", "Copy / Paste Pattern"),
                    ("Delete", "Clear Pattern"),
                    ("H", "Show/Hide This Help"),
                ];
                for (key, desc) in shortcuts {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(key).monospace().size(12.0).color(egui::Color32::from_rgb(60, 180, 90)));
                        ui.label(desc);
                    });
                }
                ui.add_space(8.0);
                ui.separator();
                if ui.button("Close").clicked() { self.show_shortcuts = false; }
            });
    }
}