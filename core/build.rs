use std::env;

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let minimap_top_left = dir.join("minimap_top_left_3.png");
    let minimap_bottom_right = dir.join("minimap_bottom_right_3.png");
    let player = dir.join("player.png");
    let erda_shower = dir.join("erda_shower.png");
    let minimap_model = dir.join("minimap_nms.onnx");
    let text_detection_model = dir.join("text_detection.onnx");
    let text_recognition_model = dir.join("text_recognition.onnx");
    let text_recognition_vocab = dir.join("text_recognition_vocab.txt");
    println!(
        "cargo:rustc-env=MINIMAP_TOP_LEFT_TEMPLATE={}",
        minimap_top_left.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MINIMAP_BOTTOM_RIGHT_TEMPLATE={}",
        minimap_bottom_right.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=PLAYER_TEMPLATE={}",
        player.to_str().unwrap()
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
        "cargo:rustc-env=TEXT_RECOGNITION_VOCAB={}",
        text_recognition_vocab.to_str().unwrap()
    );
}
