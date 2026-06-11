//! Dev utility: print input/output names and shapes of an ONNX file.
//! `cargo run --example inspect_onnx -- <path>`

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect_onnx <path>");
    let session = ort::session::Session::builder()
        .unwrap()
        .commit_from_file(&path)
        .unwrap();
    println!("== inputs");
    for input in session.inputs() {
        println!("  {} {:?}", input.name(), input.dtype());
    }
    println!("== outputs");
    for output in session.outputs() {
        println!("  {} {:?}", output.name(), output.dtype());
    }
}
