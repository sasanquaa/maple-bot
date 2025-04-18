use std::env;

fn main() {
    let dir = env::current_dir().unwrap().join("resources");
    let esc_setting = dir.join("esc_setting_ideal_ratio.png");
    let esc_menu = dir.join("esc_menu_ideal_ratio.png");
    let esc_event = dir.join("esc_event_ideal_ratio.png");
    let esc_community = dir.join("esc_community_ideal_ratio.png");
    let esc_character = dir.join("esc_character_ideal_ratio.png");
    let esc_ok = dir.join("esc_ok_ideal_ratio.png");
    let esc_cancel = dir.join("esc_ok_ideal_ratio.png");
    let elite_boss_bar = dir.join("elite_boss_bar_ideal_ratio.png");
    let player_ideal_ratio = dir.join("player_ideal_ratio.png");
    let player_default_ratio = dir.join("player_default_ratio.png");
    let erda_shower = dir.join("erda_shower_ideal_ratio.png");
    let portal = dir.join("portal_ideal_ratio.png");
    let rune = dir.join("rune_ideal_ratio.png");
    let rune_buff = dir.join("rune_buff_ideal_ratio.png");
    let sayram_elixir_buff = dir.join("sayram_elixir_buff_ideal_ratio.png");
    let aurelia_elixir_buff = dir.join("aurelia_elixir_buff_ideal_ratio.png");
    let exp_coupon_x3_buff = dir.join("exp_coupon_x3_buff_ideal_ratio.png");
    let bonus_exp_coupon_buff = dir.join("bonus_exp_coupon_buff_ideal_ratio.png");
    let legion_wealth_buff = dir.join("legion_wealth_buff_ideal_ratio.png");
    let legion_luck_buff = dir.join("legion_luck_buff_ideal_ratio.png");
    let cash_shop = dir.join("cash_shop.png");
    let hp_start = dir.join("hp_start_ideal_ratio.png");
    let hp_separator_1 = dir.join("hp_separator_ideal_ratio_1.png");
    let hp_separator_2 = dir.join("hp_separator_ideal_ratio_2.png");
    let hp_shield = dir.join("hp_shield_ideal_ratio.png");
    let hp_end = dir.join("hp_end_ideal_ratio.png");

    let mob_model = dir.join("mob.onnx");
    let rune_model = dir.join("rune.onnx");
    let minimap_model = dir.join("minimap_nms.onnx");
    let onnx_runtime = dir.join("onnxruntime.dll");
    let text_detection_model = dir.join("text_detection.onnx");
    let text_recognition_model = dir.join("text_recognition.onnx");
    let text_alphabet_txt = dir.join("alphabet_94.txt");
    println!(
        "cargo:rustc-env=ESC_SETTING_TEMPLATE={}",
        esc_setting.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_MENU_TEMPLATE={}",
        esc_menu.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_EVENT_TEMPLATE={}",
        esc_event.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_COMMUNITY_TEMPLATE={}",
        esc_community.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_CHARACTER_TEMPLATE={}",
        esc_character.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_OK_TEMPLATE={}",
        esc_ok.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ESC_CANCEL_TEMPLATE={}",
        esc_cancel.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=ELITE_BOSS_BAR_TEMPLATE={}",
        elite_boss_bar.to_str().unwrap()
    );
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
        "cargo:rustc-env=PORTAL_TEMPLATE={}",
        portal.to_str().unwrap()
    );
    println!("cargo:rustc-env=RUNE_TEMPLATE={}", rune.to_str().unwrap());
    println!(
        "cargo:rustc-env=RUNE_BUFF_TEMPLATE={}",
        rune_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=SAYRAM_ELIXIR_BUFF_TEMPLATE={}",
        sayram_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=AURELIA_ELIXIR_BUFF_TEMPLATE={}",
        aurelia_elixir_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=EXP_COUPON_X3_BUFF_TEMPLATE={}",
        exp_coupon_x3_buff.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=BONUS_EXP_COUPON_BUFF_TEMPLATE={}",
        bonus_exp_coupon_buff.to_str().unwrap()
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
        "cargo:rustc-env=CASH_SHOP_TEMPLATE={}",
        cash_shop.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_START_TEMPLATE={}",
        hp_start.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SEPARATOR_1_TEMPLATE={}",
        hp_separator_1.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SEPARATOR_2_TEMPLATE={}",
        hp_separator_2.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_SHIELD_TEMPLATE={}",
        hp_shield.to_str().unwrap()
    );
    println!(
        "cargo:rustc-env=HP_END_TEMPLATE={}",
        hp_end.to_str().unwrap()
    );

    println!(
        "cargo:rustc-env=ONNX_RUNTIME={}",
        onnx_runtime.to_str().unwrap()
    );
    println!("cargo:rustc-env=MOB_MODEL={}", mob_model.to_str().unwrap());
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
