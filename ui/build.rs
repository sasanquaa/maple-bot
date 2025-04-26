use std::{env, process::Command};

fn main() {
    let public = env::current_dir().unwrap().join("public");
    let assets = env::current_dir().unwrap().join("assets");
    let tailwind_in = assets.join("tailwind.css");
    let tailwind_out = public.join("tailwind.css");
    println!(
        "cargo:rustc-env=TAILWIND_CSS={}",
        tailwind_out.to_str().unwrap()
    );
    Command::new("npx")
        .arg("@tailwindcss/cli")
        .arg("-i")
        .arg(tailwind_in.to_str().unwrap())
        .arg("-o")
        .arg(tailwind_out.to_str().unwrap())
        .output()
        .expect("failed to build tailwindcss");
}
