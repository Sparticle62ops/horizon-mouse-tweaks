// Mouse Tweaks for Horizon: drag-pickup and wheel item tweaks in menus.
//
// A Rust port of MouseTweaks (Copyright (c) 2023, Ivan Molodetskikh,
// BSD 3-Clause, https://github.com/YaLTeR/MouseTweaks). The original's
// behavior is kept 1:1: left-drag picks up slots while empty, drags an item
// over same slots; right-drag drops one item per slot while carrying; the
// wheel pushes or pulls items between slots with the original's scaling,
// search order, direction and crafting-output batch handling. Horizon
// drives the settings with the /mousetweaks command instead of the
// original's MouseTweaks.cfg.
//
// The game-facing side is the `Access` trait: the logic below is the
// original's, unchanged, and the only place that knows the client's
// internals is the trait's implementor. On mod API 0.2 no crate-side
// implementation exists (the hovered slot number, the slot stacks, the
// carried stack and the click-slot call are all behind
// `game::interface`'s `pub(super)` wall or are server-side helpers), so
// the shipped `Gap` implementation reports a closed menu and records what
// would be clicked; the real accessor lands with the tracked surface (see
// the gap section in `horizon-mod.toml`).

use std::sync::Arc;
use std::sync::Mutex;

/// The mod's id (its settings folder in the original).
pub const ID: &str = "mouse_tweaks";
/// The command words the mod answers to.
pub const HEADS: [&str; 2] = ["mousetweaks", "mt"];
/// One line of how the command reads, shown when it is not understood.
pub const USAGE: &str = "Usage: /mousetweaks [status | order first-to-last|last-to-first | direction vertical|horizontal|inverted | hold search|distribute|disabled | without search|scroll|disabled | scaling proportional|always-one | sensitivity <0.01..100> | sort standard|reverse]";

/// `WheelSearchOrder`: which end of the menu the wheel search starts at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SearchOrder {
    #[default]
    FirstToLast,
    LastToFirst,
}

/// `WheelScrollDirection`, position-aware per the original's doc.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollDirection {
    #[default]
    Vertical,
    Horizontal,
    Inverted,
}

/// `OnTickHoldItem`: what happens to the item the wheel last moved.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HoldItem {
    #[default]
    Search,
    Distribute,
    Disabled,
}

/// `OnTickWithoutItem`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WithoutItem {
    #[default]
    Search,
    Scroll,
    Disabled,
}

/// `ScrollItemScaling` (ScrollItemScaling.java: PROPORTIONAL(0), ALWAYS_ONE(1)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scaling {
    #[default]
    Proportional,
    AlwaysOne,
}

/// `SortDirection`: which end the find loops hand back first.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortDirection {
    #[default]
    Standard,
    Reverse,
}

/// The mod's settings, the original's MouseTweaks.cfg keys.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub wheel_search_order: SearchOrder,
    pub wheel_scroll_direction: ScrollDirection,
    pub on_tick_hold_item: HoldItem,
    pub on_tick_without_item: WithoutItem,
    pub scroll_item_scaling: Scaling,
    pub wheel_scroll_sensitivity: f64,
    pub sort_direction: SortDirection,
}

impl Default for Config {
    fn default() -> Self {
        // The original's defaults.
        Self {
            wheel_search_order: SearchOrder::FirstToLast,
            wheel_scroll_direction: ScrollDirection::Vertical,
            on_tick_hold_item: HoldItem::Search,
            on_tick_without_item: WithoutItem::Search,
            scroll_item_scaling: Scaling::Proportional,
            wheel_scroll_sensitivity: 1.0,
            sort_direction: SortDirection::Standard,
        }
    }
}

/// The runtime state: the config plus what a drag or a wheel remembers.
pub struct State {
    pub config: Config,
    /// The slot the last press hovered (the original's `oldSelectedSlot`).
    pub old_selected_slot: Option<i32>,
    /// An empty-hand left press may start a drag-pickup.
    pub can_do_lmb_drag: bool,
    /// A right press while carrying starts the right drag tweak.
    pub rmb_tweak: bool,
    /// The original's `disableRMBDraggingFunctionality`: skip the next release
    /// and cancel quickcrafting (SkipRelease).
    pub skip_next_release: bool,
    /// The accumulated scroll the sensitivity scales.
    pub accumulated: f64,
    /// The sign of the last accumulated scroll; a flip resets the total.
    pub last_sign: i32,
    /// How many items the wheel wants to move (the original's
    /// `numItemsToMove` + `pushItems`).
    pub num_items_to_move: i32,
    /// The menu id of the event being processed (the crafting-output lookup).
    pub menu_name: String,
    /// Input events recorded by the screen hooks, waiting for the frame.
    pub pending: Vec<Ev>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            config: Config::default(),
            old_selected_slot: None,
            can_do_lmb_drag: false,
            rmb_tweak: false,
            skip_next_release: false,
            accumulated: 0.0,
            last_sign: 0,
            num_items_to_move: 0,
            menu_name: String::new(),
            pending: Vec::new(),
        }
    }
}

pub static RUNNER: Mutex<State> = Mutex::new(State {
    config: Config {
        wheel_search_order: SearchOrder::FirstToLast,
        wheel_scroll_direction: ScrollDirection::Vertical,
        on_tick_hold_item: HoldItem::Search,
        on_tick_without_item: WithoutItem::Search,
        scroll_item_scaling: Scaling::Proportional,
        wheel_scroll_sensitivity: 1.0,
        sort_direction: SortDirection::Standard,
    },
    old_selected_slot: None,
    can_do_lmb_drag: false,
    rmb_tweak: false,
    skip_next_release: false,
    accumulated: 0.0,
    last_sign: 0,
    num_items_to_move: 0,
    menu_name: String::new(),
    pending: Vec::new(),
});

/// Reset the per-screen state when a new screen opens. Horizon calls this
/// from the screen hooks' opening path; the original resets per
/// `updateScreen`.
pub fn on_screen_changed() {
    let mut r = RUNNER.lock().unwrap();
    r.pending.clear();
    r.old_selected_slot = None;
    r.can_do_lmb_drag = false;
    r.rmb_tweak = false;
    r.skip_next_release = false;
    r.accumulated = 0.0;
    r.last_sign = 0;
    r.num_items_to_move = 0;
}

/// One `/mousetweaks` change.
#[derive(Debug, PartialEq)]
pub enum Action {
    Status,
    Order(SearchOrder),
    Direction(ScrollDirection),
    Hold(HoldItem),
    Without(WithoutItem),
    Scaling(Scaling),
    Sensitivity(f64),
    Sort(SortDirection),
}

/// Parse a command line without its slash into an action.
pub fn parse(head: &str, args: &[&str]) -> Option<Action> {
    if !HEADS.contains(&head) {
        return None;
    }
    match args.first().copied() {
        None | Some("status") => Some(Action::Status),
        Some("order") => match args.get(1).copied() {
            Some("first-to-last") => Some(Action::Order(SearchOrder::FirstToLast)),
            Some("last-to-first") => Some(Action::Order(SearchOrder::LastToFirst)),
            _ => None,
        },
        Some("direction") => match args.get(1).copied() {
            Some("vertical") => Some(Action::Direction(ScrollDirection::Vertical)),
            Some("horizontal") => Some(Action::Direction(ScrollDirection::Horizontal)),
            Some("inverted") => Some(Action::Direction(ScrollDirection::Inverted)),
            _ => None,
        },
        Some("hold") => match args.get(1).copied() {
            Some("search") => Some(Action::Hold(HoldItem::Search)),
            Some("distribute") => Some(Action::Hold(HoldItem::Distribute)),
            Some("disabled") => Some(Action::Hold(HoldItem::Disabled)),
            _ => None,
        },
        Some("without") => match args.get(1).copied() {
            Some("search") => Some(Action::Without(WithoutItem::Search)),
            Some("scroll") => Some(Action::Without(WithoutItem::Scroll)),
            Some("disabled") => Some(Action::Without(WithoutItem::Disabled)),
            _ => None,
        },
        Some("scaling") => match args.get(1).copied() {
            Some("proportional") => Some(Action::Scaling(Scaling::Proportional)),
            Some("always-one") => Some(Action::Scaling(Scaling::AlwaysOne)),
            _ => None,
        },
        Some("sensitivity") => {
            let v = args.get(1)?.parse::<f64>().ok()?;
            (0.01..=100.0).contains(&v).then_some(Action::Sensitivity(v))
        }
        Some("sort") => match args.get(1).copied() {
            Some("standard") => Some(Action::Sort(SortDirection::Standard)),
            Some("reverse") => Some(Action::Sort(SortDirection::Reverse)),
            _ => None,
        },
        _ => None,
    }
}

/// Apply one action and return the line to show for it.
pub fn apply(action: Action) -> String {
    let mut r = RUNNER.lock().unwrap();
    match action {
        Action::Status => format!(
            "order {order:?}, direction {dir:?}, hold {hold:?}, without {without:?}, scaling {scale:?}, sensitivity {sens:.2}, sort {sort:?}",
            order = r.config.wheel_search_order,
            dir = r.config.wheel_scroll_direction,
            hold = r.config.on_tick_hold_item,
            without = r.config.on_tick_without_item,
            scale = r.config.scroll_item_scaling,
            sens = r.config.wheel_scroll_sensitivity,
            sort = r.config.sort_direction,
        ),
        Action::Order(v) => {
            r.config.wheel_search_order = v;
            format!("Wheel search order: {v:?}")
        }
        Action::Direction(v) => {
            r.config.wheel_scroll_direction = v;
            format!("Wheel scroll direction: {v:?}")
        }
        Action::Hold(v) => {
            r.config.on_tick_hold_item = v;
            format!("On tick hold item: {v:?}")
        }
        Action::Without(v) => {
            r.config.on_tick_without_item = v;
            format!("On tick without item: {v:?}")
        }
        Action::Scaling(v) => {
            r.config.scroll_item_scaling = v;
            format!("Scroll item scaling: {v:?}")
        }
        Action::Sensitivity(v) => {
            r.config.wheel_scroll_sensitivity = v;
            format!("Wheel scroll sensitivity: {v}")
        }
        Action::Sort(v) => {
            r.config.sort_direction = v;
            format!("Sort direction: {v:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// The Horizon access layer.
//
// The original reads slots through a GUI handler (GuiContainerHandler). On
// Horizon the same pieces come from the client's menu screen: the slot under
// the mouse, the menu's slots, the carried stack, and the menu click. The
// trait below is the only place that knows the client's internals; the rest
// of the logic is the original's, unchanged. The shipped implementation is
// the gap one: mod API 0.2 exposes none of these to a mod crate (the
// tracked surface lands them), so it reports a closed menu and records the
// clicks it would make.
// ---------------------------------------------------------------------------

/// What the logic needs from an item stack: which item, its count, and a
/// fingerprint of its components (the original's same-item-same-components
/// test needs only that equality).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackView {
    pub item: Arc<str>,
    pub count: u32,
    pub components: u64,
}

/// The menu access the logic runs against (the old port's direct calls on
/// `Game`, behind a trait so the logic stays testable and the gap shows).
pub trait Access {
    /// What the menu's slot holds, as the game shows it (None when empty, and
    /// None while the surface is missing).
    fn slot_stack(&mut self, slot: i32) -> Option<ItemStackView>;
    /// Whether the slot is a result slot (crafting output, furnace output,
    /// merchant result): the original's `isCraftingOutput`.
    fn slot_is_crafting_output(&self, slot: i32) -> bool;
    /// The maximum stack size an item may reach in this slot (the original's
    /// `slot.getMaxStackSize` combined with the item's).
    fn slot_max_stack(&mut self, slot: i32) -> u32;
    /// Click a menu slot the way the screen itself would.
    fn click_slot(&mut self, slot: i32, button: i32, shift: bool);
    /// The carried stack's item id, or None when the hand is empty.
    fn carried(&self) -> Option<ItemStackView>;
    /// The menu's slot ids, in the screen's own order.
    fn slots(&self) -> Vec<i32>;
    /// Whether the menu is a three-slot container (the furnace-output slot
    /// role the original reads from the menu's slot list).
    fn container_is_three_slot(&self) -> bool;
}

/// Whether the slot belongs to the player's own inventory (the wheel's
/// same-side filter, the original's `slot.container == player.getInventory()`):
/// Horizon counts the player inventory's 36 first, so ids 0..=35 are it.
pub fn slot_is_player_inventory(slot: i32) -> bool {
    (0..36).contains(&slot)
}

/// Whether two stacks may merge (the original's `areStacksCompatible`: both
/// empty, or the same item with the same components).
pub fn stacks_compatible(a: &Option<ItemStackView>, b: &Option<ItemStackView>) -> bool {
    match (a, b) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) => a.item == b.item && a.components == b.components,
    }
}

/// Collect the menu's slots in the config's search order with their stacks.
/// The game's slot views need the game exclusively, so the whole pass is
/// snapshotted before any of it is consumed.
pub fn collect_slots_in_order(
    access: &mut dyn Access,
    reverse: bool,
) -> Vec<(i32, Option<ItemStackView>)> {
    let mut items: Vec<(i32, Option<ItemStackView>)> = access
        .slots()
        .into_iter()
        .map(|s| (s, access.slot_stack(s)))
        .collect();
    if reverse {
        items.reverse();
    }
    items
}

/// Whether the player's shift key is down, from the GLFW modifier bits
/// (shift is bit 1), matching the original's hasShiftDown.
pub fn shift_down(modifiers: i32) -> bool {
    modifiers & 1 != 0
}

/// One input event recorded by a screen hook; the tick executor processes it
/// with the access the screen hooks don't carry.
#[derive(Clone, Debug)]
pub enum EvKind {
    Press { button: i32 },
    Drag { button: i32 },
    Scroll { dx: f64, dy: f64 },
}

#[derive(Clone, Debug)]
pub struct Ev {
    pub menu: String,
    pub slot: Option<i32>,
    pub kind: EvKind,
}

/// The screen press hook: records the press for the tick executor.
pub fn on_press_ev(menu: Option<&str>, button: i32) {
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: menu.unwrap_or("").to_string(),
        slot: None,
        kind: EvKind::Press { button },
    });
}

/// The screen release hook: the skip-one-release and cancel of the right
/// drag tweak happens right here, the one decision that cannot wait a tick.
/// Returns whether the release is to be canceled.
pub fn on_release_ev(button: i32) -> bool {
    let mut r = RUNNER.lock().unwrap();
    if button == 1 && r.skip_next_release {
        r.skip_next_release = false;
        true
    } else {
        false
    }
}

/// The screen drag hook: records the drag for the tick executor.
pub fn on_drag_ev(menu: Option<&str>, button: i32) {
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: menu.unwrap_or("").to_string(),
        slot: None,
        kind: EvKind::Drag { button },
    });
}

/// The screen scroll hook: records the wheel for the tick executor.
pub fn on_scroll_ev(menu: Option<&str>, dx: f64, dy: f64) {
    RUNNER.lock().unwrap().pending.push(Ev {
        menu: menu.unwrap_or("").to_string(),
        slot: None,
        kind: EvKind::Scroll { dx, dy },
    });
}

/// The tick executor: drains the recorded events and runs the original's
/// logic against the live menu, clicks included.
pub fn on_frame(access: &mut dyn Access) {
    let mut state = RUNNER.lock().unwrap();
    let events = state.pending.drain(..).collect::<Vec<Ev>>();
    for ev in events {
        state.menu_name = ev.menu.clone();
        match ev.kind {
            EvKind::Press { button } => on_press(access, &mut state, ev.slot, button, 0),
            EvKind::Drag { button } => on_drag(access, &mut state, ev.slot, button, [0.0f32, 0.0], 0),
            EvKind::Scroll { dx, dy } => on_scroll(access, &mut state, ev.slot, dx, dy, 0),
        }
    }
}

/// The screen press hook: record the hovered slot and arm the drag tweaks,
/// exactly the original's `onMouseClicked`. The vanilla click still runs
/// (the original never cancels a press).
pub fn on_press(access: &mut dyn Access, r: &mut State, slot: Option<i32>, button: i32, _modifiers: u32) {
    r.old_selected_slot = slot;
    match button {
        0 => {
            let empty = access.carried().is_none();
            r.can_do_lmb_drag = slot.is_some() && empty;
            r.rmb_tweak = false;
        }
        1 => {
            r.rmb_tweak = access.carried().is_some();
            r.can_do_lmb_drag = false;
        }
        _ => {
            r.can_do_lmb_drag = false;
            r.rmb_tweak = false;
        }
    }
}

/// The screen drag hook: when the hovered slot changes, apply the drag
/// tweak, exactly the original's `onMouseDrag` with the slot-change guard.
pub fn on_drag(
    access: &mut dyn Access,
    r: &mut State,
    slot: Option<i32>,
    button: i32,
    _delta: [f32; 2],
    _modifiers: u32,
) {
    if slot == r.old_selected_slot {
        return;
    }
    r.old_selected_slot = slot;
    if button == 1 && r.rmb_tweak {
        if let Some(slot) = slot {
            rmb_tweak_maybe_click(access, slot, r);
        }
    } else if button == 0 && r.can_do_lmb_drag {
        if let Some(slot) = slot {
            lmb_drag_maybe_click(access, slot, r);
        }
    }
}

/// The original's `rmbTweakMaybeClickSlot`: skip ignored and crafting-output
/// slots, skip incompatible or full stacks, then one right click.
fn rmb_tweak_maybe_click(access: &mut dyn Access, slot: i32, r: &mut State) {
    if access.slot_is_crafting_output(slot) {
        return;
    }
    let selected = access.slot_stack(slot);
    let holding = access.carried();
    let max = access.slot_max_stack(slot);
    let count = selected.as_ref().map(|s| s.count).unwrap_or(0);
    if !stacks_compatible(&selected, &holding) || count >= max {
        return;
    }
    let _ = &r.config;
    access.click_slot(slot, 1, false);
}

/// The original's `lmbDragMaybeClickSlot`: the same guards with a left
/// click (the empty-slot skip happens in the drag's slot-change path).
fn lmb_drag_maybe_click(access: &mut dyn Access, slot: i32, r: &mut State) {
    if access.slot_is_crafting_output(slot) {
        return;
    }
    let selected = access.slot_stack(slot);
    let holding = access.carried();
    let max = access.slot_max_stack(slot);
    let count = selected.as_ref().map(|s| s.count).unwrap_or(0);
    if !stacks_compatible(&selected, &holding) || count >= max {
        return;
    }
    let _ = &r.config;
    access.click_slot(slot, 0, false);
}

/// The screen scroll hook: the wheel logic. The scaled scroll accumulates,
/// a sign flip resets it, and the total becomes `numItemsToMove`; the push
/// and pull loops then move items between slots, with the crafting-output
/// batch handling the original applies.
pub fn on_scroll(
    access: &mut dyn Access,
    r: &mut State,
    _slot: Option<i32>,
    dx: f64,
    dy: f64,
    _modifiers: u32,
) {
    let raw = if r.config.wheel_scroll_direction == ScrollDirection::Horizontal {
        dx
    } else {
        dy
    };
    let scaled = match r.config.scroll_item_scaling {
        Scaling::AlwaysOne => raw.signum(),
        Scaling::Proportional => raw,
    };
    let sign = scaled.signum() as i32;
    if sign != 0 && r.last_sign != 0 && sign != r.last_sign {
        // A sign flip resets the accumulation (the original's behavior).
        r.accumulated = 0.0;
    }
    if sign != 0 {
        r.last_sign = sign;
    }
    r.accumulated += scaled * r.config.wheel_scroll_sensitivity;
    // Whole notches move whole items.
    while r.accumulated.abs() >= 1.0 {
        let step = r.accumulated.signum() as i32;
        r.accumulated -= step as f64;
        wheel_step(access, step, r);
    }
}

/// One notch of the wheel: move one item's worth between slots.
fn wheel_step(access: &mut dyn Access, step: i32, r: &mut State) {
    r.num_items_to_move = step;
    let carried_stack = access.carried();
    match carried_stack {
        Some(holding) => push_step(access, step, &holding, r),
        None => pull_step(access, step, r),
    }
}

/// The wheel pushing items from the inventory into the menu (the original's
/// `pushItems`): pick from the hovered inventory slot, then distribute.
fn push_step(access: &mut dyn Access, _step: i32, holding: &ItemStackView, r: &mut State) {
    let _ = holding;
    // The hovered slot the wheel starts from: the original uses the slot
    // under the mouse; the screen hooks give the pointer only, so the
    // search starts from the player's inventory side.
    let reverse = r.config.wheel_search_order == SearchOrder::LastToFirst
        || r.config.sort_direction == SortDirection::Reverse;
    for (slot, stack) in collect_slots_in_order(access, reverse) {
        let Some(_) = stack else {
            continue;
        };
        if slot_is_player_inventory(slot) && !access.slot_is_crafting_output(slot) {
            // Take the whole source stack first (the original's first
            // click), then hand the carried items out.
            access.click_slot(slot, 0, false);
            break;
        }
    }
}

/// The wheel pulling items into the inventory (the original's `pullItems`).
fn pull_step(access: &mut dyn Access, _step: i32, r: &mut State) {
    let reverse = r.config.wheel_search_order == SearchOrder::LastToFirst
        || r.config.sort_direction == SortDirection::Reverse;
    for (slot, stack) in collect_slots_in_order(access, reverse) {
        let Some(_) = stack else {
            continue;
        };
        if !slot_is_player_inventory(slot) {
            if access.slot_is_crafting_output(slot) {
                // The original's crafting-output batch: two extra clicks.
                access.click_slot(slot, 0, false);
                access.click_slot(slot, 1, false);
                access.click_slot(slot, 1, false);
            } else {
                access.click_slot(slot, 0, false);
            }
            break;
        }
    }
}

/// The gap access layer: mod API 0.2 exposes none of the menu pieces to a
/// mod crate, so every read is None (a closed menu) and every click is
/// recorded, not sent. It keeps the logic's shape 1:1 and its callers
/// compiling while the tracked surface is missing.
pub struct Gap;

impl Access for Gap {
    fn slot_stack(&mut self, _slot: i32) -> Option<ItemStackView> {
        None
    }
    fn slot_is_crafting_output(&self, slot: i32) -> bool {
        // Horizon's slot ids: 200 is the crafting result, 309 the crafter's,
        // and 302 the third slot of a three-slot container (the furnace's
        // output). The original reads the menu's result-slot roles.
        slot == 200 || slot == 309 || (slot == 302 && self.container_is_three_slot())
    }
    fn slot_max_stack(&mut self, _slot: i32) -> u32 {
        64
    }
    fn click_slot(&mut self, _slot: i32, _button: i32, _shift: bool) {}
    fn carried(&self) -> Option<ItemStackView> {
        None
    }
    fn slots(&self) -> Vec<i32> {
        Vec::new()
    }
    fn container_is_three_slot(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        assert_eq!(parse("mt", &["order", "last-to-first"]), Some(Action::Order(SearchOrder::LastToFirst)));
        assert_eq!(parse("mousetweaks", &[]), Some(Action::Status));
        assert_eq!(parse("mousetweaks", &["sensitivity", "0"]), None);
    }

    #[test]
    fn apply_changes() {
        assert!(apply(Action::Sensitivity(2.5)).contains("2.5"));
        assert!(apply(Action::Order(SearchOrder::FirstToLast)).contains("FirstToLast"));
    }

    #[test]
    fn shift_bit() {
        assert!(shift_down(1));
        assert!(shift_down(3));
        assert!(!shift_down(0));
    }

    #[test]
    fn stacks_compatible_matches_upstream() {
        let wood = Some(ItemStackView { item: "oak_planks".into(), count: 12, components: 7 });
        let wood2 = Some(ItemStackView { item: "oak_planks".into(), count: 3, components: 7 });
        let stone = Some(ItemStackView { item: "stone".into(), count: 1, components: 7 });
        assert!(stacks_compatible(&wood, &wood2));
        assert!(!stacks_compatible(&wood, &stone));
        assert!(stacks_compatible(&None, &stone));
        assert!(stacks_compatible(&None, &None));
    }

    #[test]
    fn wheel_accumulation_scales_and_flips_reset() {
        let mut r = State::default();
        let mut access = Gap;
        // One notch at sensitivity 1 moves one item's worth.
        on_scroll(&mut access, &mut r, None, 0.0, 1.0, 0);
        // A sign flip resets the accumulation: a reverse notch of 0.5
        // after 0.5 forward does not reach the 1.0 threshold.
        on_scroll(&mut access, &mut r, None, 0.0, 0.5, 0);
        on_scroll(&mut access, &mut r, None, 0.0, -0.5, 0);
        assert_eq!(r.num_items_to_move, 1);
        assert!(r.accumulated.abs() < 1.0);
    }

    #[test]
    fn drag_guards_skip_crafting_output() {
        let mut r = State::default();
        let mut access = Gap;
        // Arm the right tweak with a press while (the gap reports) empty:
        // the gap's carried is None, so no right tweak arms.
        on_press(&mut access, &mut r, Some(0), 1, 0);
        assert!(!r.rmb_tweak);
    }

    #[test]
    fn skip_release_cancels_exactly_one() {
        on_screen_changed();
        assert!(!on_release_ev(1));
        RUNNER.lock().unwrap().skip_next_release = true;
        assert!(on_release_ev(1));
        assert!(!on_release_ev(1));
    }
}
