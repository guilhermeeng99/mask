//! Dev utility: find which feature-frame counts a voice model accepts.
//! `cargo run --example probe_voice_t -- <model.onnx>`

use mask_lib::vc::engine::{detect_backend, RvcSession};

fn main() {
    let model = std::env::args()
        .nth(1)
        .expect("usage: probe_voice_t <model>");
    let appdata = std::env::var("APPDATA").unwrap();
    let data_dir = std::path::Path::new(&appdata).join("dev.guilherme.mask");
    let companions = mask_lib::vc::companion_paths(&data_dir);

    for t in [32usize, 50, 64, 65, 96, 100, 128, 150, 200] {
        // chunk samples at 48 kHz that produce ~t frames (100 fps).
        let chunk_len = t * 480;
        let chunk: Vec<f32> = (0..chunk_len)
            .map(|i| {
                let time = i as f32 / 48_000.0;
                0.4 * (2.0 * (110.0 * time).fract() - 1.0)
            })
            .collect();
        let result = RvcSession::load(
            &companions,
            std::path::Path::new(&model),
            40_000,
            0,
            detect_backend(),
        );
        match result {
            Ok(mut s) => match s.process(&chunk) {
                Ok(out) => println!("T~{t}: OK ({} samples out)", out.len()),
                Err(e) => println!(
                    "T~{t}: process FAIL: {}",
                    &e.to_string()[..120.min(e.to_string().len())]
                ),
            },
            Err(e) => {
                println!(
                    "T~{t}: load FAIL: {}",
                    &e.to_string()[..160.min(e.to_string().len())]
                );
                break; // load failure is T-independent with the dry run inside
            }
        }
    }
}
