//! Mouse Tweaks for Horizon: drag-pickup and wheel item tweaks in menus.
//!
//! Exact 1:1 Rust port of MouseTweaks by Ivan Molodetskikh (YaLTeR,
//! BSD-3-Clause, https://github.com/YaLTeR/MouseTweaks). Upstream is three
//! mixins over the container screen (ScreenMouseDragMixin,
//! ScreenMouseClickedMixin, ScreenMouseScrolledMixin) driving one handler
//! class. This port maps that class code-for-code in `runtime`, against the
//! mod API 0.2 screen mouse hooks; the game-facing access (the hovered slot,
//! the slot stacks, the carried stack, the menu click) is behind the
//! tracked client-slot surface (see the gap section in `horizon-mod.toml`).

pub mod runtime;

pub use runtime::{
    apply, on_drag, on_drag_ev, on_frame, on_press, on_press_ev, on_release_ev, on_screen_changed,
    on_scroll, on_scroll_ev, parse, shift_down, stacks_compatible, Action, Access, Config, Ev,
    EvKind, Gap, HoldItem, ItemStackView, RUNNER, Scaling, ScrollDirection, SearchOrder, State,
    USAGE,
};

/// The mod.
#[derive(Default)]
pub struct MouseTweaks;

#[mod_api::horizon_mod]
impl mod_api::Hooks for MouseTweaks {
    fn register(&self, _registrar: &mut dyn mod_api::Registrar, _cx: &mod_api::RegistrationContext) {
    }

    /// 1:1 of the original's screen mouse mixins: the press, release, drag
    /// and wheel hooks record what the screen saw; the release hook applies
    /// the right drag's skip-release cancel live (that decision cannot wait
    /// a tick), the rest waits for the tick executor.
    fn screen_mouse_button_pressed_pre(
        &self,
        _host: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenMouseButtonPressedPreEvent<'_>,
    ) -> mod_api::Flow {
        if event.screen.id == "inventory" || event.screen.id == "creative" {
            runtime::on_press_ev(event.screen.menu, event.button);
        }
        mod_api::Flow::Continue
    }

    fn screen_mouse_button_released_pre(
        &self,
        _host: &dyn mod_api::ClientHost,
        _event: &mut mod_api::ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> mod_api::Flow {
        if runtime::on_release_ev(_event.button) {
            mod_api::Flow::Cancel
        } else {
            mod_api::Flow::Continue
        }
    }

    fn screen_mouse_dragged_pre(
        &self,
        _host: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenMouseDraggedPreEvent<'_>,
    ) -> mod_api::Flow {
        if event.screen.id == "inventory" || event.screen.id == "creative" {
            runtime::on_drag_ev(event.screen.menu, event.button);
        }
        mod_api::Flow::Continue
    }

    fn screen_mouse_scrolled_pre(
        &self,
        _host: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenMouseScrolledPreEvent<'_>,
    ) -> mod_api::Flow {
        if event.screen.id == "inventory" || event.screen.id == "creative" {
            runtime::on_scroll_ev(event.screen.menu, event.scroll[0], event.scroll[1]);
        }
        mod_api::Flow::Continue
    }

    /// The tick executor: drains the recorded events and runs the original's
    /// logic against the live menu. On mod API 0.2 the menu access behind
    /// `runtime::Gap` reports a closed menu, so the decisions are computed
    /// and the clicks stay pending until the tracked client-slot surface
    /// lands.
    fn client_tick_pre(&self, _host: &mut dyn mod_api::ClientHost, _event: &mut mod_api::ClientTickPreEvent) {
        runtime::on_frame(&mut runtime::Gap);
    }

    /// `/mousetweaks`, the config surface.
    fn register_client_commands(
        &self,
        _host: &mut dyn mod_api::ClientHost,
        event: &mut mod_api::RegisterClientCommandsEvent<'_>,
    ) {
        event.commands.push(mod_api::Command::new(runtime::USAGE));
    }

    /// Dispatch a typed `/mousetweaks ...` line: parse, apply, answer.
    fn command(
        &self,
        _host: &mut dyn mod_api::ClientHost,
        event: &mut mod_api::CommandEvent<'_>,
    ) -> mod_api::Flow {
        let line = event.command.trim();
        let Some(rest) = line.strip_prefix('/') else {
            return mod_api::Flow::Continue;
        };
        let mut words = rest.split_whitespace();
        let Some(head) = words.next() else {
            return mod_api::Flow::Continue;
        };
        if !runtime::HEADS.contains(&head) {
            return mod_api::Flow::Continue;
        }
        let args: Vec<&str> = words.collect();
        let Some(action) = runtime::parse(head, &args) else {
            return mod_api::Flow::Cancel;
        };
        let _line = runtime::apply(action);
        mod_api::Flow::Cancel
    }
}
