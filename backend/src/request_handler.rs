#[cfg(debug_assertions)]
use std::sync::LazyLock;
#[cfg(debug_assertions)]
use std::time::Instant;

#[cfg(debug_assertions)]
use include_dir::{Dir, include_dir};
use log::debug;
use opencv::core::{MatTraitConst, MatTraitConstManual, Vec4b};
#[cfg(debug_assertions)]
use opencv::{
    core::{Mat, ModifyInplace, Vector},
    imgcodecs::{IMREAD_COLOR, imdecode},
    imgproc::{COLOR_BGR2BGRA, cvt_color_def},
};
use platforms::windows::{Handle, KeyInputKind, KeyKind, KeyReceiver, query_capture_handles};
#[cfg(debug_assertions)]
use rand::distr::{Alphanumeric, SampleString};
use tokio::sync::broadcast;

#[cfg(debug_assertions)]
use crate::debug::{
    save_image_for_training, save_image_for_training_to, save_minimap_for_training,
};
#[cfg(debug_assertions)]
use crate::detect::{ArrowsCalibrating, ArrowsState, CachedDetector, Detector};
#[cfg(debug_assertions)]
use crate::mat::OwnedMat;
use crate::{
    Action, ActionCondition, ActionKey, Bound, Configuration, GameState, KeyBinding,
    KeyBindingConfiguration, Minimap as MinimapData, PotionMode, RequestHandler, Settings,
    bridge::{ImageCapture, ImageCaptureKind, KeySenderMethod},
    buff::{BuffKind, BuffState},
    context::Context,
    database::InputMethod,
    minimap::{Minimap, MinimapState},
    player::PlayerState,
    poll_request,
    rotator::Rotator,
    skill::SkillKind,
};

pub struct DefaultRequestHandler<'a> {
    pub context: &'a mut Context,
    pub config: &'a mut Configuration,
    pub settings: &'a mut Settings,
    pub buffs: &'a mut Vec<(BuffKind, KeyBinding)>,
    pub buff_states: &'a mut Vec<BuffState>,
    pub actions: &'a mut Vec<Action>,
    pub rotator: &'a mut Rotator,
    pub player: &'a mut PlayerState,
    pub minimap: &'a mut MinimapState,
    pub key_sender: &'a broadcast::Sender<KeyBinding>,
    pub key_receiver: &'a mut KeyReceiver,
    pub image_capture: &'a mut ImageCapture,
    pub capture_handles: &'a mut Vec<(String, Handle)>,
    pub selected_capture_handle: &'a mut Option<Handle>,
    #[cfg(debug_assertions)]
    pub recording_images_id: &'a mut Option<String>,
    #[cfg(debug_assertions)]
    pub infering_rune: &'a mut Option<(ArrowsCalibrating, Instant)>,
}

impl DefaultRequestHandler<'_> {
    pub fn poll_request(&mut self) {
        poll_request(self);
    }

    pub fn poll_key(&mut self) {
        poll_key(self);
    }

    #[cfg(debug_assertions)]
    pub fn poll_debug(&mut self) {
        if let Some((calibrating, instant)) = self.infering_rune.as_ref().copied() {
            if instant.elapsed().as_secs() >= 10 {
                debug!(target: "debug", "infer rune timed out");
                *self.infering_rune = None;
            } else {
                match self
                    .context
                    .detector_unwrap()
                    .detect_rune_arrows(calibrating)
                {
                    Ok(ArrowsState::Complete(arrows)) => {
                        debug!(target: "debug", "infer rune result {arrows:?}");
                        // TODO: Save
                        *self.infering_rune = None;
                    }
                    Ok(ArrowsState::Calibrating(calibrating)) => {
                        *self.infering_rune = Some((calibrating, instant));
                    }
                    Err(err) => {
                        debug!(target: "debug", "infer rune failed {err}");
                        *self.infering_rune = None;
                    }
                }
            }
        }

        if let Some(id) = self.recording_images_id.clone() {
            save_image_for_training_to(
                self.context.detector_unwrap().mat(),
                Some(id),
                false,
                false,
            );
        }
    }

    fn update_rotator_actions(&mut self) {
        let mode = self
            .minimap
            .data()
            .map(|minimap| minimap.rotation_mode)
            .unwrap_or_default()
            .into();
        let reset_on_erda = self
            .minimap
            .data()
            .map(|minimap| minimap.actions_any_reset_on_erda_condition)
            .unwrap_or_default();

        self.rotator.build_actions(
            mode,
            config_actions(self.config)
                .into_iter()
                .chain(self.actions.iter().copied())
                .collect::<Vec<_>>()
                .as_slice(),
            self.buffs,
            self.config.potion_key.key,
            self.settings.enable_rune_solving,
            reset_on_erda,
        );
    }
}

impl RequestHandler for DefaultRequestHandler<'_> {
    fn on_rotate_actions(&mut self, halting: bool) {
        if self.minimap.data().is_some() {
            self.context.halting = halting;
            if halting {
                self.rotator.reset_queue();
                self.player.clear_actions_aborted();
            }
        }
    }

    fn on_rotate_actions_halting(&self) -> bool {
        self.context.halting
    }

    fn on_create_minimap(&self, name: String) -> Option<MinimapData> {
        if let Minimap::Idle(idle) = self.context.minimap {
            Some(MinimapData {
                name,
                width: idle.bbox.width,
                height: idle.bbox.height,
                ..MinimapData::default()
            })
        } else {
            None
        }
    }

    fn on_update_minimap(&mut self, preset: Option<String>, minimap: MinimapData) {
        self.minimap.set_data(minimap);

        let minimap = self.minimap.data().unwrap();
        self.player.reset();
        self.player.config.rune_platforms_pathing = minimap.rune_platforms_pathing;
        self.player.config.rune_platforms_pathing_up_jump_only =
            minimap.rune_platforms_pathing_up_jump_only;
        self.player.config.auto_mob_platforms_pathing = minimap.auto_mob_platforms_pathing;
        self.player.config.auto_mob_platforms_pathing_up_jump_only =
            minimap.auto_mob_platforms_pathing_up_jump_only;
        self.player.config.auto_mob_platforms_bound = minimap.auto_mob_platforms_bound;
        *self.actions = preset
            .and_then(|preset| minimap.actions.get(&preset).cloned())
            .unwrap_or_default();
        self.update_rotator_actions();
    }

    fn on_update_configuration(&mut self, config: Configuration) {
        *self.config = config;
        *self.buffs = config_buffs(self.config);
        self.player.reset();
        self.player.config.class = self.config.class;
        self.player.config.interact_key = self.config.interact_key.key.into();
        self.player.config.grappling_key = self.config.ropelift_key.key.into();
        self.player.config.teleport_key = self.config.teleport_key.map(|key| key.key.into());
        self.player.config.jump_key = self.config.jump_key.key.into();
        self.player.config.upjump_key = self.config.up_jump_key.map(|key| key.key.into());
        self.player.config.cash_shop_key = self.config.cash_shop_key.key.into();
        self.player.config.potion_key = self.config.potion_key.key.into();
        self.player.config.use_potion_below_percent =
            match (self.config.potion_key.enabled, self.config.potion_mode) {
                (false, _) | (_, PotionMode::EveryMillis(_)) => None,
                (_, PotionMode::Percentage(percent)) => Some(percent / 100.0),
            };
        self.player.config.update_health_millis = Some(self.config.health_update_millis);
        self.buff_states.iter_mut().for_each(|state| {
            state.update_enabled_state(self.config, self.settings);
        });
        self.update_rotator_actions();
    }

    fn on_update_settings(&mut self, settings: Settings) {
        let handle_or_default = self.selected_capture_handle.unwrap_or(self.context.handle);

        if settings.capture_mode != self.settings.capture_mode {
            self.image_capture
                .set_mode(handle_or_default, settings.capture_mode);
        }

        if settings.input_method != self.settings.input_method {
            if let ImageCaptureKind::BitBltArea(capture) = self.image_capture.kind() {
                *self.key_receiver = KeyReceiver::new(capture.handle(), KeyInputKind::Foreground);
                if matches!(settings.input_method, InputMethod::Default) {
                    self.context.keys.set_method(KeySenderMethod::Default(
                        capture.handle(),
                        KeyInputKind::Foreground,
                    ));
                }
            } else if matches!(settings.input_method, InputMethod::Default) {
                self.context.keys.set_method(KeySenderMethod::Default(
                    handle_or_default,
                    KeyInputKind::Fixed,
                ));
            }
        }
        if let InputMethod::Rpc = settings.input_method {
            self.context.keys.set_method(KeySenderMethod::Rpc(
                settings.input_method_rpc_server_url.clone(),
            ));
        }

        *self.settings = settings;
        self.buff_states.iter_mut().for_each(|state| {
            state.update_enabled_state(self.config, self.settings);
        });
        self.update_rotator_actions();
    }

    #[inline]
    fn on_redetect_minimap(&mut self) {
        self.context.minimap = Minimap::Detecting;
    }

    #[inline]
    fn on_game_state(&self) -> GameState {
        GameState {
            position: self.player.last_known_pos.map(|pos| (pos.x, pos.y)),
            health: self.player.health,
            state: self.context.player.to_string(),
            normal_action: self.player.normal_action_name(),
            priority_action: self.player.priority_action_name(),
            erda_shower_state: self.context.skills[SkillKind::ErdaShower].to_string(),
            destinations: self
                .player
                .last_destinations
                .clone()
                .map(|points| {
                    points
                        .into_iter()
                        .map(|point| (point.x, point.y))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }
    }

    #[inline]
    fn on_minimap_frame(&self) -> Option<(Vec<u8>, usize, usize)> {
        self.context
            .detector
            .as_ref()
            .map(|detector| detector.mat())
            .and_then(|mat| extract_minimap(self.context, mat))
    }

    fn on_minimap_platforms_bound(&self) -> Option<Bound> {
        if let Minimap::Idle(idle) = self.context.minimap {
            idle.platforms_bound.map(|bound| bound.into())
        } else {
            None
        }
    }

    #[inline]
    fn on_key_receiver(&self) -> broadcast::Receiver<KeyBinding> {
        self.key_sender.subscribe()
    }

    fn on_query_capture_handles(&mut self) -> (Vec<String>, Option<usize>) {
        *self.capture_handles = query_capture_handles();

        let names = self
            .capture_handles
            .iter()
            .map(|(name, _)| name)
            .cloned()
            .collect::<Vec<_>>();
        let selected = if let Some(selected_handle) = self.selected_capture_handle {
            self.capture_handles
                .iter()
                .enumerate()
                .find(|(_, (_, handle))| handle == selected_handle)
                .map(|(i, _)| i)
        } else {
            None
        };
        (names, selected)
    }

    fn on_select_capture_handle(&mut self, index: Option<usize>) {
        let handle = index
            .and_then(|index| self.capture_handles.get(index))
            .map(|(_, handle)| *handle);
        let handle_or_default = handle.unwrap_or(self.context.handle);

        *self.selected_capture_handle = handle;
        self.image_capture
            .set_mode(handle_or_default, self.settings.capture_mode);
        if !matches!(self.settings.input_method, InputMethod::Rpc) {
            self.context.keys.set_method(KeySenderMethod::Default(
                handle_or_default,
                KeyInputKind::Fixed,
            ));
            *self.key_receiver = KeyReceiver::new(handle_or_default, KeyInputKind::Fixed);
        }
    }

    #[cfg(debug_assertions)]
    fn on_capture_image(&self, is_grayscale: bool) {
        if let Some(ref detector) = self.context.detector {
            save_image_for_training(detector.mat(), is_grayscale, false);
        }
    }

    #[cfg(debug_assertions)]
    fn on_infer_rune(&mut self) {
        *self.infering_rune = Some((ArrowsCalibrating::default(), Instant::now()));
    }

    #[cfg(debug_assertions)]
    fn on_infer_minimap(&self) {
        if let Some(ref detector) = self.context.detector {
            // FIXME: 160 matches one in minimap.rs
            if let Ok(rect) = detector.detect_minimap(160) {
                save_minimap_for_training(detector.mat(), rect);
            }
        }
    }

    #[cfg(debug_assertions)]
    fn on_record_images(&mut self, start: bool) {
        *self.recording_images_id = if start {
            Some(Alphanumeric.sample_string(&mut rand::rng(), 8))
        } else {
            None
        };
    }

    #[cfg(debug_assertions)]
    fn on_test_spin_rune(&self) {
        static SPIN_TEST_DIR: Dir<'static> = include_dir!("$SPIN_TEST_DIR");
        static SPIN_TEST_IMAGES: LazyLock<Vec<Mat>> = LazyLock::new(|| {
            let mut files = SPIN_TEST_DIR.files().collect::<Vec<_>>();
            files.sort_by_key(|file| file.path().to_str().unwrap());
            files
                .into_iter()
                .map(|file| {
                    let vec = Vector::from_slice(file.contents());
                    let mut mat = imdecode(&vec, IMREAD_COLOR).unwrap();
                    unsafe {
                        mat.modify_inplace(|mat, mat_mut| {
                            cvt_color_def(mat, mat_mut, COLOR_BGR2BGRA).unwrap();
                        });
                    }
                    mat
                })
                .collect()
        });

        let mut calibrating = ArrowsCalibrating::default();
        calibrating.enable_spin_test();

        for mat in &*SPIN_TEST_IMAGES {
            match CachedDetector::new(OwnedMat::from(mat.clone())).detect_rune_arrows(calibrating) {
                Ok(ArrowsState::Complete(arrows)) => {
                    debug!(target: "test", "spin test completed {arrows:?}");
                }
                Ok(ArrowsState::Calibrating(new_calibrating)) => {
                    calibrating = new_calibrating;
                }
                Err(err) => {
                    debug!(target: "test", "spin test error {err}");
                    break;
                }
            }
        }
    }
}

// TODO: should only handle a single matched key binding
#[inline]
fn poll_key(handler: &mut DefaultRequestHandler) {
    let Some(received_key) = handler.key_receiver.try_recv() else {
        return;
    };
    debug!(target: "handler", "received key {received_key:?}");
    if let KeyBindingConfiguration { key, enabled: true } = handler.settings.toggle_actions_key
        && KeyKind::from(key) == received_key
    {
        handler.on_rotate_actions(!handler.context.halting);
    }
    let _ = handler.key_sender.send(received_key.into());
}

#[inline]
fn extract_minimap(context: &Context, mat: &impl MatTraitConst) -> Option<(Vec<u8>, usize, usize)> {
    if let Minimap::Idle(idle) = context.minimap {
        let minimap = mat
            .roi(idle.bbox)
            .unwrap()
            .iter::<Vec4b>()
            .unwrap()
            .flat_map(|bgra| {
                let bgra = bgra.1;
                [bgra[2], bgra[1], bgra[0], 255]
            })
            .collect::<Vec<u8>>();
        return Some((minimap, idle.bbox.width as usize, idle.bbox.height as usize));
    }
    None
}

pub fn config_buffs(config: &Configuration) -> Vec<(BuffKind, KeyBinding)> {
    let mut buffs = Vec::new();
    if let KeyBindingConfiguration { key, enabled: true } = config.sayram_elixir_key {
        buffs.push((BuffKind::SayramElixir, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.aurelia_elixir_key {
        buffs.push((BuffKind::AureliaElixir, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.exp_x3_key {
        buffs.push((BuffKind::ExpCouponX3, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.bonus_exp_key {
        buffs.push((BuffKind::BonusExpCoupon, key));
    }
    // TODO: Disable until have better way to do this...
    if let KeyBindingConfiguration { key, enabled: true } = config.wealth_acquisition_potion_key {
        buffs.push((BuffKind::WealthAcquisitionPotion, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.exp_accumulation_potion_key {
        buffs.push((BuffKind::ExpAccumulationPotion, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.legion_luck_key {
        buffs.push((BuffKind::LegionLuck, key));
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.legion_wealth_key {
        buffs.push((BuffKind::LegionWealth, key));
    }
    buffs
}

fn config_actions(config: &Configuration) -> Vec<Action> {
    let mut vec = Vec::new();
    if let KeyBindingConfiguration { key, enabled: true } = config.feed_pet_key {
        let feed_pet_action = Action::Key(ActionKey {
            key,
            count: 1,
            condition: ActionCondition::EveryMillis(config.feed_pet_millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        });
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
        vec.push(feed_pet_action);
    }
    if let KeyBindingConfiguration { key, enabled: true } = config.potion_key
        && let PotionMode::EveryMillis(millis) = config.potion_mode
    {
        vec.push(Action::Key(ActionKey {
            key,
            count: 1,
            condition: ActionCondition::EveryMillis(millis),
            wait_before_use_millis: 350,
            wait_after_use_millis: 350,
            ..ActionKey::default()
        }));
    }
    vec.extend(
        config
            .actions
            .iter()
            .copied()
            .filter_map(|action| action.enabled.then_some(Action::from(action))),
    );
    vec
}
