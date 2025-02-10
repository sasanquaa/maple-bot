use std::env;

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let player_ideal_ratio = dir.join("player_ideal_ratio.png");
    let player_default_ratio = dir.join("player_default_ratio.png");
    let erda_shower = dir.join("erda_shower_ideal_ratio.png");
    let minimap_model = dir.join("minimap_nms.onnx");
    let text_detection_model = dir.join("text_detection.onnx");
    let text_recognition_model = dir.join("text_recognition.onnx");
    let text_alphabet_txt = dir.join("alphabet_94.txt");
    println!(
        "cargo:rustc-env=PLAYER_DEFAULT_RATIO_TEMPLATE={}",
        player_default_ratio.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_IDEAL_RATIO_TEMPLATE={}",
        player_ideal_ratio.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ERDA_SHOWER_TEMPLATE={}",
        erda_shower.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MINIMAP_MODEL={}",
        minimap_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_DETECTION_MODEL={}",
        text_detection_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_RECOGNITION_MODEL={}",
        text_recognition_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=TEXT_RECOGNITION_ALPHABET={}",
        text_alphabet_txt.to_str().unwrap()
    );
}
