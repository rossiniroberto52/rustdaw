use hound::{WavReader, SampleFormat};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "demo_pattern.wav";
    let mut reader = WavReader::open(path)?;
    println!("WAV spec: {:?}", reader.spec());
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.spec().sample_rate, 44100);
    assert_eq!(reader.spec().bits_per_sample, 32);
    assert_eq!(reader.spec().sample_format, SampleFormat::Float);

    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    assert_eq!(samples.len() % 2, 0); // stereo
    let mono_samples: Vec<f32> = samples.chunks(2).map(|chunk| (chunk[0] + chunk[1]) / 2.0).collect();

    // Compute RMS in 125 ms windows
    let window_size = (0.125 * 44100.0) as usize; // 125 ms
    println!("Window size: {} samples", window_size);
    let mut rms_values = Vec::new();
    for chunk in mono_samples.chunks(window_size) {
        if chunk.is_empty() {
            break;
        }
        let sum_squares: f64 = chunk.iter().map(|&s| s as f64 * s as f64).sum();
        let rms = (sum_squares / chunk.len() as f64).sqrt() as f32;
        rms_values.push(rms);
    }
    println!("Number of RMS windows: {}", rms_values.len());
    
    // Print first 32 RMS values (4 seconds @ 125ms = 32 windows)
    for (i, &rms) in rms_values.iter().enumerate().take(32) {
        println!("Window {}: RMS = {:.4}", i, rms);
    }
    
    // Check that RMS is not constant (i.e., there is variation)
    if rms_values.len() > 1 {
        let mean = rms_values.iter().sum::<f32>() / rms_values.len() as f32;
        let variance: f32 = rms_values.iter().map(|&v| (v - mean) * (v - mean)).sum::<f32>() / rms_values.len() as f32;
        println!("Mean RMS: {:.4}", mean);
        println!("Variance: {:.6}", variance);
        if variance > 1e-4 {
            println!("SUCCESS: RMS varies (variance > 1e-4), indicating rhythmic pattern");
        } else {
            println!("FAILURE: RMS is constant (variance = {}), expected rhythmic pattern", variance);
        }
    }
    
    Ok(())
}