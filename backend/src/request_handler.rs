use log::debug;
use opencv::core::{MatTraitConst, MatTraitConstManual, Vec4b};
use platforms::windows::{KeyInputKind, KeyKind, KeyReceiver, WgcCapture, WindowBoxCapture};
use tokio::sync::broadcast;

use crate::{
    Action, ActionCondition, ActionKey, Bound, CaptureMode, Configuration, GameState, KeyBinding,
    KeyBindingConfiguration, Minimap as MinimapData, PotionMode, RequestHandler, RotatorMode,
    Settings,
    buff::{BuffKind, BuffState},
    context::{Context, KeySenderKind},
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
    pub wgc_capture: Option<&'a mut WgcCapture>,
    pub window_box_capture: &'a WindowBoxCapture,
}

impl DefaultRequestHandler<'_> {
    pub fn poll_request(&mut self) {
        poll_request(self);
    }

    pub fn poll_key(&mut self) {
        poll_key(self);
    }

    fn update_rotator_actions(&mut self, mode: RotatorMode) {
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
        );
    }
}

impl RequestHandler for DefaultRequestHandler<'_> {
    fn on_rotate_actions(&mut self, halting: bool) {
        if self.minimap.data().is_some() {
            self.context.halting = halting;
            if halting {
                self.rotator.reset_queue();
                self.player.abort_actions();
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
        self.update_rotator_actions(minimap.rotation_mode.into());
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
        self.update_rotator_actions(
            self.minimap
                .data()
                .map(|minimap| minimap.rotation_mode)
                .unwrap_or_default()
                .into(),
        );
    }

    fn on_update_settings(&mut self, settings: Settings) {
        if !matches!(settings.capture_mode, CaptureMode::WindowsGraphicsCapture)
            && let Some(ref mut wgc_capture) = self.wgc_capture
        {
            wgc_capture.stop_capture();
        }
        if !matches!(settings.capture_mode, CaptureMode::BitBltArea) {
            self.window_box_capture.hide();
            if matches!(settings.input_method, InputMethod::Default) {
                self.context
                    .keys
                    .set_kind(KeySenderKind::Fixed(self.context.handle));
            }
        } else {
            self.window_box_capture.show();
            *self.key_receiver =
                KeyReceiver::new(self.window_box_capture.handle(), KeyInputKind::Foreground);
            if matches!(settings.input_method, InputMethod::Default) {
                self.context
                    .keys
                    .set_kind(KeySenderKind::Foreground(self.window_box_capture.handle()));
            }
        }
        if let InputMethod::Rpc = settings.input_method {
            self.context.keys.set_kind(KeySenderKind::Rpc(
                settings.input_method_rpc_server_url.clone(),
            ));
        }
        *self.settings = settings;
        self.buff_states.iter_mut().for_each(|state| {
            state.update_enabled_state(self.config, self.settings);
        });
        self.update_rotator_actions(
            self.minimap
                .data()
                .map(|minimap| minimap.rotation_mode)
                .unwrap_or_default()
                .into(),
        );
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
    vec
}
