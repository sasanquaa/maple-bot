use std::ops::DerefMut;

use backend::context::update_loop;
use components::{
    button::{OneButton, TwoButtons},
    options::Options,
};
use dioxus::{
    desktop::{
        WindowBuilder,
        wry::dpi::{PhysicalSize, Size},
    },
    document::EvalError,
    logger::tracing::Level,
    prelude::*,
};
use tokio::{sync::mpsc, task::spawn_blocking};
use tracing_log::LogTracer;

mod components;

const TAILWIND_CSS: Asset = asset!("public/tailwind.css");

// 許してくれよ！UIなんてよくわからん
// 使えば十分よ！๑-﹏-๑

fn main() {
    dioxus::logger::init(Level::DEBUG);
    LogTracer::init().unwrap();
    update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(700, 400)))
        .with_resizable(false)
        .with_maximizable(false)
        .with_title("Maple Bot")
        .with_always_on_top(true);
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div {
            class: "flex",
            // Minimap {}
            // Characters {}
        }
    }
}

// #[component]
// fn Minimap() -> Element {
//     let mut grid_width = use_signal_sync(|| 0);
//     let mut grid_height = use_signal_sync(|| 0);

//     use_future(move || async move {
//         let (tx, mut rx) = mpsc::channel::<(Vec<u8>, usize, usize)>(1);
//         let _ = spawn(async move {
//             let mut canvas = document::eval(include_str!("js/minimap.js"));
//             loop {
//                 let result = rx.recv().await;
//                 let Some(frame) = result else {
//                     continue;
//                 };
//                 let Err(error) = canvas.send(frame) else {
//                     continue;
//                 };
//                 if matches!(error, EvalError::Finished) {
//                     // probably: https://github.com/DioxusLabs/dioxus/issues/2979
//                     canvas = document::eval(include_str!("js/minimap.js"));
//                 }
//             }
//         });
//         let _ = spawn_blocking(move || {
//             game::Context::new()
//                 .expect("failed to start game update loop")
//                 .update_loop(|context| {
//                     if let Ok((bytes, width, height)) = context.minimap() {
//                         let cur_width = *grid_width.peek();
//                         let cur_height = *grid_height.peek();
//                         if cur_width != width || cur_height != height {
//                             *grid_width.write() = width;
//                             *grid_height.write() = height;
//                         }
//                         let _ = tx.try_send((bytes, width, height));
//                     }
//                 })
//         })
//         .await;
//     });

//     rsx! {
//         div {
//             class: "grid grid-flow-row auto-rows-max p-[16px] w-[350px] place-items-center",
//             p {
//                 "Player State"
//             }
//             div {
//                 class: "flex w-full relative",
//                 canvas {
//                     class: "w-full",
//                     id: "canvas-minimap",
//                 },
//                 canvas {
//                     id: "canvas-minimap-magnifier",
//                     class: "absolute hidden",
//                 }
//             }
//             p {
//                 "Action 1"
//             }
//             p {
//                 "Action 2"
//             }
//             p {
//                 "Action 3"
//             }
//             p {
//                 "Action 4"
//             }
//         }
//     }
// }

// #[component]
// fn Characters() -> Element {
//     #[derive(Clone, Copy, Debug)]
//     struct EditingInvalid {
//         character_name: bool,
//         skill_name: bool,
//         skill_keys: bool,
//     }
//     static DEFAULT_CHARACTER: Character = Character {
//         id: None,
//         name: String::new(),
//         skills: vec![],
//     };
//     static DEFAULT_SKILL: Skill = Skill {
//         name: String::new(),
//         kind: SkillKind::Other,
//         binding: SkillBinding::Single('y'),
//     };
//     static DEFAULT_INVALID: EditingInvalid = EditingInvalid {
//         character_name: true,
//         skill_name: true,
//         skill_keys: false,
//     };

//     let mut characters = use_resource(|| async {
//         spawn_blocking(|| query_characters().unwrap_or_default())
//             .await
//             .unwrap()
//     });
//     let mut editing = use_signal(|| false);
//     let mut editing_character = use_signal(|| DEFAULT_CHARACTER.clone());
//     let mut editing_skill = use_signal(|| DEFAULT_SKILL.clone());
//     let mut editing_invalid = use_signal(|| DEFAULT_INVALID);
//     let mut editing_invalid_message = use_signal(|| "".to_owned());

//     use_effect(move || {
//         if editing() {
//             *editing_character.write() = DEFAULT_CHARACTER.clone();
//             *editing_skill.write() = DEFAULT_SKILL.clone();
//             *editing_invalid.write() = DEFAULT_INVALID;
//             *editing_invalid_message.write() = "".to_owned();
//         }
//     });

//     rsx! {
//         match characters() {
//             Some(characters_vec) => rsx! {
//                 div {
//                     class: "grid grid-flow-row grid-cols-1 gap-y-3 w-fit h-fit",
//                     if !editing() {
//                         OneButton {
//                             on_ok: move |_| {
//                                 *editing.write() = true;
//                             },
//                             "Create character"
//                         }
//                     } else {
//                         input {
//                             class: "font-meiryo",
//                             oninput: move |e| {
//                                 let value = e.value();
//                                 editing_invalid.write().deref_mut().character_name = value.is_empty();
//                                 editing_character.write().deref_mut().name = value;
//                             },
//                             placeholder: "Character name",
//                             value: editing_character().name
//                         }
//                         for skill in editing_character().skills {
//                             p {
//                                 class: "text-xs font-meiryo",
//                                 {
//                                     format!("{} / {}", skill.name, match skill.binding {
//                                         SkillBinding::Single(c) => c.to_string(),
//                                         SkillBinding::Composite(chars) => chars.iter().collect(),
//                                     })
//                                 }
//                             }
//                         }
//                         hr {
//                             class: "border border-black"
//                         }
//                         div {
//                             class: "grid grid-flow-row grid-cols-1 gap-y-4",
//                             OneButton {
//                                 on_ok: move |_| {
//                                     let invalid = *editing_invalid.peek();
//                                     if invalid.skill_keys || invalid.skill_name {
//                                         *editing_invalid_message.write() = "One of the skill input is invalid".to_owned();
//                                     } else {
//                                         *editing_invalid_message.write() = "".to_owned();
//                                         editing_character.write().deref_mut().skills.push((*editing_skill.peek()).clone());
//                                     }
//                                 },
//                                 "Add skill"
//                             }
//                             p {
//                                 class: "text-xs font-meiryo text-gray-500",
//                                 "Skill keys and name must have at least 1 character"
//                             }
//                             Options<SkillKind> {
//                                 label: "Skill",
//                                 options: vec![
//                                     (SkillKind::ErdaShower, "ErdaShower".to_owned()),
//                                     (SkillKind::SolJanus, "SolJanus".to_owned()),
//                                     (SkillKind::UpJump, "UpJump".to_owned()),
//                                     (SkillKind::RopeLift, "RopeLift".to_owned()),
//                                     (SkillKind::DoubleJump, "DoubleJump".to_owned()),
//                                     (SkillKind::Other, "Other".to_owned()),
//                                 ],
//                                 on_select: move |(value, name)| {
//                                     if !matches!(value, SkillKind::Other) {
//                                         editing_invalid.write().deref_mut().skill_name = false;
//                                         editing_skill.write().deref_mut().name = name;
//                                     } else {
//                                         editing_invalid.write().deref_mut().skill_name = true;
//                                         editing_skill.write().deref_mut().name = String::new();
//                                     }
//                                     editing_skill.write().deref_mut().kind = value;
//                                 },
//                                 selected: editing_skill().kind
//                             }
//                             input {
//                                 class: "font-meiryo",
//                                 oninput: move |e| {
//                                     let value = e.value();
//                                     editing_invalid.write().deref_mut().skill_keys = value.is_empty();
//                                     if value.is_empty() {
//                                         // spagetti
//                                         editing_skill.write().deref_mut().binding = SkillBinding::Composite(vec![]);
//                                         return
//                                     }
//                                     let binding = if value.len() > 1 {
//                                         SkillBinding::Composite(value.chars().collect::<Vec<_>>())
//                                     } else {
//                                         SkillBinding::Single(value.chars().next().unwrap())
//                                     };
//                                     editing_skill.write().deref_mut().binding = binding;
//                                 },
//                                 placeholder: "Skill keys (1 character = 1 key)",
//                                 value: match editing_skill().binding {
//                                     SkillBinding::Single(c) => c.to_string(),
//                                     SkillBinding::Composite(chars) => chars.iter().collect(),
//                                 }
//                             }
//                             if matches!(editing_skill().kind, SkillKind::Other) {
//                                 input {
//                                     class: "font-meiryo",
//                                     oninput: move |e| {
//                                         let value = e.value();
//                                         editing_invalid.write().deref_mut().skill_name = value.is_empty();
//                                         editing_skill.write().deref_mut().name = value;
//                                     },
//                                     placeholder: "Skill name",
//                                     value: editing_skill().name
//                                 }
//                             }
//                         }
//                         hr {
//                             class: "border border-black"
//                         }
//                         if !editing_invalid_message().is_empty() {
//                             p {
//                                 class: "text-xs font-meiryo text-red-500",
//                                 {editing_invalid_message}
//                             }
//                         }
//                         TwoButtons {
//                             on_ok: move |_| {
//                                 let invalid = *editing_invalid.peek();
//                                 if invalid.character_name {
//                                     *editing_invalid_message.write() = "Character name is invalid".to_owned();
//                                 } else {
//                                     upsert_character(editing_character.write().deref_mut()).unwrap();
//                                     characters.restart();
//                                     *editing.write() = false;
//                                 }
//                             },
//                             ok_body: rsx! {"Save"},
//                             on_cancel: move |_| {
//                                 *editing.write() = false;
//                             },
//                             cancel_body: rsx! {"Cancel"}
//                         }
//                     }
//                     ul {
//                         for character in characters_vec {
//                             li {
//                                 p {
//                                     class: "text-sm text-dark font-meiryo",
//                                     {character.name}
//                                 }
//                             }
//                         }
//                     }
//                 }
//             },
//             None => rsx! {},
//         }
//     }
// }
