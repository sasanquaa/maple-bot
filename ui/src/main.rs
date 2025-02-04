use std::{thread::sleep, time::Duration};

use backend::game;
use dioxus::prelude::*;

fn main() {
    game::Context::new().unwrap().start();
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut count = use_signal(|| 0);

    use_effect(move || {
        spawn_forever(async move {
            loop {
                count.set(count + 1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    });

    rsx! {
        div {
            h1 { "Counter: {count} " }
        }
    }
}
