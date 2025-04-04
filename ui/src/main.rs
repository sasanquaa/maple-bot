#![feature(variant_count)]
#![feature(map_try_insert)]

use std::string::ToString;

use action::Actions;
use backend::{Configuration as ConfigurationData, Minimap as MinimapData, start_update_loop};
use configuration::Configuration;
use dioxus::{
    desktop::{
        WindowBuilder,
        tao::{platform::windows::WindowBuilderExtWindows, window::WindowSizeConstraints},
        wry::dpi::{PhysicalSize, PixelUnit, Size},
    },
    prelude::*,
};
use minimap::Minimap;
use tab::Tab;
use tracing_log::LogTracer;

mod action;
mod configuration;
mod icons;
mod input;
mod key;
mod minimap;
mod platform;
mod rotation;
mod select;
mod tab;

const TAILWIND_CSS: Asset = asset!("public/tailwind.css");
const AUTO_NUMERIC_JS: Asset = asset!("assets/autoNumeric.min.js");

// TODO: Fix spaghetti UI
fn main() {
    LogTracer::init().unwrap();
    start_update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(448, 820)))
        .with_inner_size_constraints(WindowSizeConstraints::new(
            Some(PixelUnit::Physical(448.into())),
            Some(PixelUnit::Physical(820.into())),
            None,
            None,
        ))
        .with_resizable(true)
        .with_drag_and_drop(false)
        .with_title("Maple Bot");
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    const TAB_CONFIGURATION: &str = "Configuration";
    const TAB_ACTIONS: &str = "Actions";

    let minimap = use_signal::<Option<MinimapData>>(|| None);
    let preset = use_signal::<Option<String>>(|| None);
    let last_preset = use_signal::<Option<(i64, String)>>(|| None);
    let configs = use_signal_sync(Vec::<ConfigurationData>::new);
    let config = use_signal_sync::<Option<ConfigurationData>>(|| None);
    let copy_position = use_signal::<Option<(i32, i32)>>(|| None);
    let mut active_tab = use_signal(|| TAB_CONFIGURATION.to_string());
    let mut script_loaded = use_signal(|| false);

    // Thanks dioxus
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            const scriptInterval = setInterval(async () => {
                try {
                    AutoNumeric;
                    await dioxus.send(true);
                    clearInterval(scriptInterval);
                } catch(_) { }
            }, 10);
        "#,
        );
        eval.recv::<bool>().await.unwrap();
        script_loaded.set(true);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: AUTO_NUMERIC_JS }
        if script_loaded() {
            div { class: "flex flex-col max-w-xl h-screen mx-auto space-y-2",
                Minimap {
                    minimap,
                    preset,
                    last_preset,
                    copy_position,
                    config,
                }
                Tab {
                    tabs: vec![TAB_CONFIGURATION.to_string(), TAB_ACTIONS.to_string()],
                    class: "py-2 px-4 font-medium text-sm focus:outline-none",
                    selected_class: "bg-white text-gray-800",
                    unselected_class: "hover:text-gray-700 text-gray-400 bg-gray-100",
                    on_tab: move |tab| {
                        active_tab.set(tab);
                    },
                    tab: active_tab(),
                }
                match active_tab().as_str() {
                    TAB_CONFIGURATION => rsx! {
                        div { class: "px-2 pb-2 pt-2 overflow-y-auto scrollbar h-full",
                            Configuration { configs, config }
                        }
                    },
                    TAB_ACTIONS => rsx! {
                        Actions { minimap, preset, copy_position }
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
}
