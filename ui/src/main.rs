use backend::game;
use dioxus::{
    desktop::{
        WindowBuilder,
        wry::dpi::{PhysicalSize, Size},
    },
    prelude::*,
};
use tokio::{sync::mpsc, task::spawn_blocking};

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    let window = WindowBuilder::new()
        .with_decorations(false)
        .with_resizable(false)
        .with_inner_size(Size::Physical(PhysicalSize::new(384, 216)));
    let cfg = dioxus::desktop::Config::default().with_window(window);
    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    // let mut minimap = use_signal(String::new);
    // let mut minimap_name = use_signal(String::new);

    use_future(move || async move {
        let canvas = document::eval(
            r#"
                let canvas = document.getElementById("canvas-minimap");
                let ctx = canvas.getContext("2d");
                while (true) {
                    let [buffer, width, height] = await dioxus.recv();
                    let data = new ImageData(new Uint8ClampedArray(buffer), width, height);
                    let bitmap = await createImageBitmap(data);
                    ctx.drawImage(bitmap, 0, 0);
                }
            "#,
        );
        let (tx, mut rx) = mpsc::channel::<(Vec<u8>, usize, usize)>(1);
        let _ = spawn(async move {
            loop {
                let result = rx.recv().await;
                let Some(frame) = result else {
                    continue;
                };
                let _ = canvas.send(frame);
            }
        });
        let _ = spawn_blocking(move || {
            game::Context::new()
                .expect("failed to start game update loop")
                .update_loop(|context| {
                    if let Ok(frame) = context.minimap() {
                        let _ = tx.try_send(frame);
                    }
                })
        })
        .await;
    });

    rsx! {
        document::Stylesheet {
            href: TAILWIND_CSS
        }
        canvas {
            id: "canvas-minimap"
        }
    }
}
