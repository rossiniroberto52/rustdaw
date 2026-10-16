
use rustdaw_visualizer::{SpectrumAnalyzer, generate_sine_wave};
use ringbuf::HeapRb;

fn main() {
    let sample_rate = 44100.0;
    let fft_size = 1024;
    let freq = 440.0; // Hz

    // Generate a 440 Hz sine wave.
    let samples = generate_sine_wave(freq, sample_rate, 1.0);

    println!("First 10 samples: {:?}", &samples[0..10]);

    // Create a ringbuf and split it.
    let ringbuf = HeapRb::<f32>::new(fft_size * 2);
    let (mut producer, mut consumer) = ringbuf.split();

    // Feed the producer with the samples.
    for &s in &samples[..fft_size] {
        // Use try_push because we are in a test and the buffer is big enough.
        assert!(producer.try_push(s).is_ok());
    }

    // Create spectrum analyzer.
    let mut analyzer = SpectrumAnalyzer::new(fft_size);

    // Update the analyzer by consuming from the consumer.
    analyzer.update(&mut consumer);

    // Print the spectrum magnitudes for the first 20 bins.
    println!("Spectrum magnitudes (first 20 bins):");
    for i in 0..20 {
        println!("Bin {}: {}", i, analyzer.spectrum[i]);
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

    // Calculate the expected bin for 440 Hz.
    // Bin i corresponds to frequency: i * sample_rate / fft_size
    let expected_bin = (freq * fft_size as f32 / sample_rate).round() as usize;
    println!("Expected bin: {}", expected_bin);
}
