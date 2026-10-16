
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;
use std::f32::consts::PI;

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

fn main() {
    let sample_rate = 44100.0;
    let fft_size = 1024;
    let freq = 440.0; // Hz

    // Generate a 440 Hz sine wave.
    let samples = generate_sine_wave(freq, sample_rate, 1.0);

    println!("First 10 samples: {:?}", &samples[0..10]);

    // Create window (Hann)
    let window = if fft_size > 1 {
        (0..fft_size)
            .map(|i| {
                0.5_f32 * (1.0_f32 - (2.0_f32 * PI * i as f32 / ((fft_size - 1) as f32)).cos())
            })
            .collect()
    } else {
        vec![1.0; fft_size]
    };

    println!("First 10 window values: {:?}", &window[0..10]);

    // Apply window
    let mut windowed_samples = Vec::with_capacity(fft_size);
    for i in 0..fft_size {
        windowed_samples.push(samples[i] * window[i]);
    }
    println!("First 10 windowed samples: {:?}", &windowed_samples[0..10]);

    // Prepare FFT input
    let mut fft_input = vec![Complex { re: 0.0, im: 0.0 }; fft_size];
    for i in 0..fft_size {
        fft_input[i] = Complex { re: windowed_samples[i], im: 0.0 };
    }

    // Run FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut fft_output = fft_input.clone(); // FFT will process in-place, so we clone.
    fft.process(&mut fft_output);

    println!("FFT output (first 10 bins, real and imag):");
    for i in 0..10.min(fft_size) {
        println!("  {}: {} + {}i", i, fft_output[i].re, fft_output[i].im);
    }

    // Compute magnitude spectrum (positive frequencies)
    let mut spectrum = Vec::with_capacity(fft_size / 2);
    for i in 0..(fft_size / 2) {
        let re = fft_output[i].re;
        let im = fft_output[i].im;
        spectrum.push((re * re + im * im).sqrt());
    }

    println!("Spectrum magnitudes (first 20 bins):");
    for i in 0..20.min(spectrum.len()) {
        println!("  {}: {}", i, spectrum[i]);
    }

    // Find max bin
    let mut max_bin = 0;
    let mut max_mag = 0.0;
    for (i, &mag) in spectrum.iter().enumerate() {
        if mag > max_mag {
            max_mag = mag;
            max_bin = i;
        }
    }
    println!("Max bin: {} with magnitude {}", max_bin, max_mag);

    // Expected bin
    let expected_bin = (freq * fft_size as f32 / sample_rate).round() as usize;
    println!("Expected bin: {}", expected_bin);
}
