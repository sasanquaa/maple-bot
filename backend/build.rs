use std::env;

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let player_ideal_ratio = dir.join("player_ideal_ratio.png");
    let player_default_ratio = dir.join("player_default_ratio.png");
    let erda_shower = dir.join("erda_shower_ideal_ratio.png");
    let rune = dir.join("rune_ideal_ratio.png");
    let rune_buff = dir.join("rune_buff_ideal_ratio.png");
    let exp_coupon_x3_buff = dir.join("exp_coupon_x3_buff_ideal_ratio.png");
    let legion_wealth_buff = dir.join("legion_wealth_buff_ideal_ratio.png");
    let legion_luck_buff = dir.join("legion_luck_buff_ideal_ratio.png");
    let sayram_elixir_buff = dir.join("sayram_elixir_buff_ideal_ratio.png");
    let cash_shop = dir.join("cash_shop.png");

    let rune_model = dir.join("rune.onnx");
    let minimap_model = dir.join("minimap_nms.onnx");
    let onnx_runtime = dir.join("onnxruntime.dll");
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
    println!("cargo:rustc-env=RUNE_TEMPLATE={}", rune.to_str().unwrap());
    println!(
        "cargo:rustc-env=RUNE_BUFF_TEMPLATE={}",
        rune_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_COUPON_X3_BUFF_TEMPLATE={}",
        exp_coupon_x3_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LEGION_WEALTH_BUFF_TEMPLATE={}",
        legion_wealth_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=LEGION_LUCK_BUFF_TEMPLATE={}",
        legion_luck_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SAYRAM_ELIXIR_BUFF_TEMPLATE={}",
        sayram_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=CASH_SHOP_TEMPLATE={}",
        cash_shop.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=ONNX_RUNTIME={}",
        onnx_runtime.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=MINIMAP_MODEL={}",
        minimap_model.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=RUNE_MODEL={}",
        rune_model.to_str().unwrap()
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
