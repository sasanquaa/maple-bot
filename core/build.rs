fn main() {
    let dir = std::env::current_dir().unwrap().join("resources");
    let minimap_top_left = dir.join("minimap_top_left_3.png");
    let minimap_bottom_right = dir.join("minimap_bottom_right_3.png");
    let player = dir.join("player.png");
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
}
