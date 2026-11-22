# Mouse Tweaks for Horizon

A Rust port of MouseTweaks (BSD 3-Clause, (c) 2023 Ivan Molodetskikh,
https://github.com/YaLTeR/MouseTweaks) for Horizon native mods, exact 1:1.
`src/runtime.rs` maps the original's handler code-for-code: left-drag
pickup, right-drag drop-one-per-slot with the skip-release cancel, wheel
push/pull with the original's scaling, search order, direction and
crafting-output batch handling. The seven `MouseTweaks.cfg` keys are the
config surface, driven by the `/mousetweaks` command (hooked into the mod
API 0.2 client command hook).

## Horizon feature gap

The drag and wheel logic needs a client slot and menu interaction surface
API 0.2 does not expose: the hovered slot number, the slot stacks and the
carried stack, and a client-side click-slot call. The screen mouse hooks
below are real (they record the events; the release hook applies the
skip-release cancel live), and the tick executor runs the 1:1 decision
logic; its clicks stay pending until that surface lands. Tracked on
horizon-rust/horizon, linked from the PR.

Pinned to Horizon f00125f61f794aaa3d4e950d301f353b35b6ee3d and mod API 0.2.
License: BSD 3-Clause (the original's), notice preserved in LICENSE.
Source: https://github.com/Sparticle62ops/horizon-mouse-tweaks
