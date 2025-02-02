use std::env;

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let player = dir.join("player.png");
    let erda_shower = dir.join("erda_shower.png");
    let minimap_model = dir.join("minimap_nms.onnx");
    let text_detection_model = dir.join("text_detection.onnx");
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
}
