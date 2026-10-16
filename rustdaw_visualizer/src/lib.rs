//! Audio visualizer: oscilloscope, spectrum analyzer, VU meter.

use eframe::egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, NumExt};
use eframe::egui::epaint::{PathShape, Shape};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use ringbuf::traits::{Consumer, Producer, Split};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

/// Audio buffer shared between audio thread and UI thread via a lock-free ring buffer.
pub struct AudioBuffer {
    /// The producer end of the ring buffer (for the audio thread).
    producer: HeapProd<f32>,
    /// The consumer end of the ring buffer (for the UI thread).
    consumer: HeapCons<f32>,
}

impl AudioBuffer {
    pub fn new(capacity: usize) -> Self {
        let rb = HeapRb::<f32>::new(capacity);
        let (producer, consumer) = rb.split();
        Self { producer, consumer }
    }

    /// Split the buffer into producer and consumer.
    /// This consumes the AudioBuffer and returns the two halves.
    pub fn split(self) -> (HeapProd<f32>, HeapCons<f32>) {
        (self.producer, self.consumer)
    }
}

/// Oscilloscope widget: draws the waveform.
pub struct Oscilloscope {
    /// Buffer to hold the samples for drawing.
    samples: Vec<f32>,
    /// Maximum number of samples to display.
    max_samples: usize,
}

impl Oscilloscope {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    /// Update the oscillator with new samples from the consumer.
    /// We consume all available samples and store them for drawing.
    pub fn update(&mut self, consumer: &mut impl Consumer<Item = f32>) {
        // Clear the current samples.
        self.samples.clear();
        // Pull all available samples from the consumer.
        while let Some(sample) = consumer.try_pop() {
            self.samples.push(sample);
            // We limit the number of samples to max_samples to avoid growing indefinitely.
            if self.samples.len() >= self.max_samples {
                break;
            }
        }
        // If we have fewer than max_samples, we keep what we have.
    }

    /// Draw the oscillator widget.
    pub fn ui(&mut self, ui: &mut eframe::egui::Ui) -> eframe::egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            eframe::egui::Vec2::new(ui.available_width(), 100.0),
            Sense::hover(),
        );
        ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(50)));

        if self.samples.is_empty() {
            return response;
        }

        // Normalize samples to [-1, 1] for drawing.
        let mut min_sample = self.samples[0];
        let mut max_sample = self.samples[0];
        for &s in &self.samples {
            if s < min_sample {
                min_sample = s;
            }
            if s > max_sample {
                max_sample = s;
            }
        }
        let range = max_sample - min_sample;
        if range < 1e-6 {
            // Avoid division by zero.
            return response;
        }

        // Draw the waveform.
        let mut points = Vec::with_capacity(self.samples.len());
        for (i, &sample) in self.samples.iter().enumerate() {
            let x = rect.left() + (i as f32 / (self.samples.len() - 1) as f32) * rect.width();
            // Normalize sample to [0, 1] range for y.
            let normalized = (sample - min_sample) / range;
            let y = rect.top() + (1.0_f32 - normalized) * rect.height(); // Flip y because screen coordinates.
            points.push(Pos2::new(x, y));
        }

        // Create a PathShape for the stroke.
        let shape = Shape::Path(PathShape {
            points,
            closed: false,
            stroke: Stroke::new(2.0_f32, Color32::from_rgb(0, 150, 255)).into(),
            fill: Color32::TRANSPARENT,
        });
        ui.painter().add(shape);
        response
    }
}

/// Spectrum analyzer widget: draws the frequency spectrum.
pub struct SpectrumAnalyzer {
    /// FFT planner.
    planner: FftPlanner<f32>,
    /// Buffer for FFT input (complex).
    fft_input: Vec<Complex<f32>>,
    /// Buffer for FFT output.
    fft_output: Vec<Complex<f32>>,
    /// Magnitude spectrum.
    spectrum: Vec<f32>,
    /// Size of the FFT.
    fft_size: usize,
    /// Window function (Hann).
    window: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let _fft = planner.plan_fft_forward(fft_size);
        let fft_input = vec![Complex { re: 0.0, im: 0.0 }; fft_size];
        let fft_output = vec![Complex { re: 0.0, im: 0.0 }; fft_size];
        let spectrum = vec![0.0; fft_size / 2]; // Only positive frequencies.
        let window = if fft_size > 1 {
            (0..fft_size)
                .map(|i| {
                    0.5_f32 * (1.0_f32 - (2.0_f32 * std::f32::consts::PI * i as f32 / ((fft_size - 1) as f32)).cos())
                })
                .collect()
        } else {
            vec![1.0; fft_size]
        };

        Self {
            planner,
            fft_input,
            fft_output,
            spectrum,
            fft_size,
            window,
        }
    }

    /// Update the spectrum analyzer with new samples from the consumer.
    /// We consume exactly fft_size samples (or as many as available, padding with zeros).
    pub fn update(&mut self, consumer: &mut impl Consumer<Item = f32>) {
        // Create a temporary buffer for the samples.
        let mut samples = Vec::with_capacity(self.fft_size);
        // Pull up to fft_size samples from the consumer.
        while let Some(sample) = consumer.try_pop() {
            samples.push(sample);
            if samples.len() >= self.fft_size {
                break;
            }
        }
        // If we have fewer than fft_size samples, pad with zeros.
        if samples.len() < self.fft_size {
            samples.resize(self.fft_size, 0.0);
        }
        self.process_buffer(&samples);
    }

        fn process_buffer(&mut self, samples: &[f32]) {
        // Apply window function.
        for i in 0..self.fft_size {
            self.fft_input[i] = Complex {
                re: samples[i] * self.window[i],
                im: 0.0,
            };
        }

        // Run FFT.
        let fft = self.planner.plan_fft_forward(self.fft_size);
        fft.process(&mut self.fft_input);

        // Copy the result to fft_output for magnitude computation.
        self.fft_output.copy_from_slice(&self.fft_input);

        // Compute magnitude spectrum (only positive frequencies).
        for i in 0..(self.fft_size / 2) {
            let re = self.fft_output[i].re;
            let im = self.fft_output[i].im;
            self.spectrum[i] = (re * re + im * im).sqrt();
        }

        // Apply smoothing (simple moving average over spectrum bins).
        if self.spectrum.len() > 2 {
            let mut smoothed = self.spectrum.clone();
            for i in 1..self.spectrum.len() - 1 {
                smoothed[i] =
                    (self.spectrum[i - 1] + self.spectrum[i] + self.spectrum[i + 1]) / 3.0;
            }
            self.spectrum = smoothed;
        }
    }


    /// Draw the spectrum analyzer widget.
    pub fn ui(&mut self, ui: &mut eframe::egui::Ui) -> eframe::egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            eframe::egui::Vec2::new(ui.available_width(), 100.0),
            Sense::hover(),
        );
        ui.painter()
            .rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(50)));

        if self.spectrum.is_empty() {
            return response;
        }

        // Find max value for scaling.
        let mut max_mag = 0.0;
        for &mag in &self.spectrum {
            if mag > max_mag {
                max_mag = mag;
            }
        }
        if max_mag < 1e-6 {
            max_mag = 1.0; // Avoid division by zero.
        }

        // Draw the spectrum as a line graph.
        let mut points = Vec::with_capacity(self.spectrum.len());
        for (i, &mag) in self.spectrum.iter().enumerate() {
            // Linear frequency axis for now. TODO: implement log axis.
            let x = rect.left() + (i as f32 / (self.spectrum.len() - 1) as f32) * rect.width();
            let normalized = mag / max_mag;
            let y = rect.top() + (1.0_f32 - normalized) * rect.height(); // Flip y.
            points.push(Pos2::new(x, y));
        }

        // Create a PathShape for the stroke.
        let shape = Shape::Path(PathShape {
            points,
            closed: false,
            stroke: Stroke::new(2.0_f32, Color32::from_rgb(0, 255, 0)).into(),
            fill: Color32::TRANSPARENT,
        });
        ui.painter().add(shape);
        response
    }
}

/// VU meter widget: shows peak and RMS levels.
pub struct VuMeter {
    /// Peak level (0.0 to 1.0).
    peak: f32,
    /// RMS level (0.0 to 1.0).
    rms: f32,
    /// Decay factor for peak (how fast it falls back).
    peak_decay: f32,
}

impl VuMeter {
    pub fn new() -> Self {
        Self {
            peak: 0.0,
            rms: 0.0,
            peak_decay: 0.95_f32, // Adjust as needed.
        }
    }

    /// Update the VU meter with new samples from the consumer.
    /// We consume all available samples to compute peak and RMS.
    pub fn update(&mut self, consumer: &mut impl Consumer<Item = f32>) {
        // We'll collect samples in a temporary vector to compute RMS and peak.
        let mut samples = Vec::new();
        while let Some(sample) = consumer.try_pop() {
            samples.push(sample);
        }
        if samples.is_empty() {
            return;
        }

        // Calculate RMS.
        let mut sum_squares = 0.0;
        for &s in &samples {
            sum_squares += s * s;
        }
        let rms = (sum_squares / samples.len() as f32).sqrt();

        // Calculate peak (absolute value).
        let mut peak = 0.0;
        for &s in &samples {
            let abs_s = s.abs();
            if abs_s > peak {
                peak = abs_s;
            }
        }

        // Apply decay to peak.
        self.peak = self.peak * self.peak_decay + peak * (1.0_f32 - self.peak_decay);
        self.rms = rms;
    }

    /// Draw the VU meter widget.
    pub fn ui(&mut self, ui: &mut eframe::egui::Ui) -> eframe::egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            eframe::egui::Vec2::new(20.0, 100.0),
            Sense::hover(),
        );
        ui.painter()
            .rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_gray(50)));

        // Draw background.
        ui.painter().rect_filled(rect, 0.0, Color32::from_gray(30));

        // Draw peak level.
        let peak_height = (self.peak * rect.height()).at_most(rect.height());
        let peak_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + rect.height() - peak_height),
            Vec2::new(rect.width(), peak_height),
        );
        ui.painter().rect_filled(peak_rect, 0.0, Color32::from_rgb(255, 0, 0));

        // Draw RMS level.
        let rms_height = (self.rms * rect.height()).at_most(rect.height());
        let rms_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + rect.height() - rms_height),
            Vec2::new(rect.width(), rms_height),
        );
        ui.painter().rect_filled(rms_rect, 0.0, Color32::from_rgb(0, 255, 0));

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use ringbuf::HeapRb;

    /// Helper to generate a sine wave.
    fn generate_sine_wave(freq: f32, sample_rate: f32, duration: f32) -> Vec<f32> {
        let num_samples = (duration * sample_rate) as usize;
        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..num_samples {
            let t = i as f32 / sample_rate;
            let sample = (2.0 * PI * freq * t).sin();
            samples.push(sample);
        }
        samples
    }

            #[test]
    fn test_spectrum_analyzer_fft() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let freq = 430.6640625; // Hz, exactly bin 10

        // Generate a sine wave with exactly fft_size samples.
        let samples = generate_sine_wave(freq, sample_rate, (fft_size as f32) / sample_rate);

        // Create a ringbuf and split it.
        let ringbuf = HeapRb::<f32>::new(fft_size * 2);
        let (mut producer, mut consumer) = ringbuf.split();

        // Feed the producer with the samples.
        for &s in &samples {
            // Use try_push because we are in a test and the buffer is big enough.
            assert!(producer.try_push(s).is_ok());
        }

        // Create spectrum analyzer.
        let mut analyzer = SpectrumAnalyzer::new(fft_size);

        // Update the analyzer by consuming from the consumer.
        analyzer.update(&mut consumer);

        // Debug: print first 10 samples
        println!("First 10 samples:");
        for i in 0..10.min(fft_size) {
            println!("  {}: {}", i, samples[i]);
        }
        println!("Last 10 samples:");
        for i in 0..10.min(fft_size) {
            println!("  {}: {}", fft_size - 10 + i, samples[fft_size - 10 + i]);
        }

        // Debug: print first 10 windowed samples
        let window = if fft_size > 1 {
            (0..fft_size)
                .map(|i| {
                    0.5_f32 * (1.0_f32 - (2.0_f32 * std::f32::consts::PI * i as f32 / ((fft_size - 1) as f32)).cos())
                })
                .collect()
        } else {
            vec![1.0; fft_size]
        };
        println!("First 10 windowed samples:");
        for i in 0..10.min(fft_size) {
            println!("  {}: {}", i, samples[i] * window[i]);
        }
        println!("Last 10 windowed samples:");
        for i in 0..10.min(fft_size) {
            println!("  {}: {}", fft_size - 10 + i, samples[fft_size - 10 + i] * window[fft_size - 10 + i]);
        }

        // Debug: print FFT output (first 10 bins, real and imag)
        println!("FFT output (first 10 bins, real and imag):");
        for i in 0..10.min(fft_size) {
            println!("  {}: {} + {}i", i, analyzer.fft_output[i].re, analyzer.fft_output[i].im);
        }
        println!("FFT output (bins 5-15):");
        for i in 5..15 {
            if i < fft_size {
                println!("  {}: {} + {}i", i, analyzer.fft_output[i].re, analyzer.fft_output[i].im);
            }
        }

        // Debug: print spectrum magnitudes (first 20 bins)
        println!("Spectrum magnitudes (first 20 bins):");
        for i in 0..20.min(analyzer.spectrum.len()) {
            println!("  {}: {}", i, analyzer.spectrum[i]);
        }
        println!("Spectrum magnitudes (bins 5-15):");
        for i in 5..15 {
            if i < analyzer.spectrum.len() {
                println!("  {}: {}", i, analyzer.spectrum[i]);
            }
        }

        // Find the bin with the maximum magnitude.
        let mut max_bin = 0;
        let mut max_mag = 0.0;
        for (i, &mag) in analyzer.spectrum.iter().enumerate() {
            if mag > max_mag {
                max_mag = mag;
                max_bin = i;
            }
        }
        println!("Max bin: {} with magnitude {}", max_bin, max_mag);

        // Calculate the expected bin for the given frequency.
        // Bin i corresponds to frequency: i * sample_rate / fft_size
        let expected_bin = (freq * fft_size as f32 / sample_rate).round() as usize;
        println!("Expected bin: {}", expected_bin);

        // Allow some tolerance due to windowing and finite sample size.
        let tolerance = 2; // bins
        assert!(
            (max_bin as i32 - expected_bin as i32).abs() <= tolerance,
            "Expected peak at bin {}, got bin {} (tolerance +/- {})",
            expected_bin,
            max_bin,
            tolerance
        );
    }



    #[test]
    fn test_audio_buffer() {
        let audio_buffer = AudioBuffer::new(1024);
        let (mut producer, mut consumer) = audio_buffer.split();

        // Push some samples.
        for i in 0..100 {
            assert!(producer.try_push(i as f32 * 0.01).is_ok());
        }
        // Pop them back.
        for i in 0..100 {
            let sample = consumer.try_pop().unwrap();
            assert!((sample - (i as f32 * 0.01)).abs() < 1e-5);
        }
        // Buffer should now be empty.
        assert!(consumer.try_pop().is_none());
    }
}
