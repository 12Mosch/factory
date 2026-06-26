# AGENTS.md

This is a Factorio clone built with the Bevy game engine.

## Task Completion Requirements

Before considering a task completed, the following commands must pass unless the task is explicitly documentation-only or the environment prevents running them:

* `cargo fmt --all`
* `cargo check`
* `cargo clippy --all-targets -- -D warnings`
* `cargo test`

If any command fails, do not mark the task as complete. Fix the failure. 

## Core Priorities

1. Correctness and reliability come first.
2. Performance is a core requirement, especially for simulation, ECS systems, asset loading, and rendering-critical paths.
3. Behavior must remain predictable under load, during frame drops, and when failures occur.

If a tradeoff is required, choose correctness, robustness, and deterministic behavior over short-term convenience or premature optimization.

## Maintainability

Long-term maintainability is a core priority.

Before adding new functionality, check whether existing logic can be reused, generalized, or extracted into a shared module. Duplicate logic is a code smell and should be avoided unless there is a clear reason for keeping implementations separate.

Do not solve problems by adding isolated local logic when the behavior belongs in a reusable system, component, resource, plugin, or utility module.

Do not be afraid to refactor existing code when it improves clarity, correctness, or future extensibility. Avoid shortcuts that make future simulation, debugging, or performance work harder.

## Bevy Architecture Guidelines

The Bevy 0.19 Rust API documentation should be considered a primary reference: https://docs.rs/bevy/0.19.0/bevy/

Prefer idiomatic Bevy ECS patterns.

Use components for entity-local state, resources for global singleton state, messages for buffered inter-system communication, events and observers for immediate reactive behavior, and plugins to group related app logic and configuration.

Keep systems small, focused, and testable. Avoid large systems that mix input handling, simulation logic, rendering setup, and UI updates.

Separate simulation logic from presentation logic whenever possible. Rendering, UI, animation, audio, and debug visualization must not be required for the core simulation to work.

Use `FixedUpdate` for simulation and gameplay rules that require stable timestep behavior. Use `Update` for frame-based work such as UI, presentation, input collection, camera control, and audio control.

When player input affects fixed-step simulation, collect input in frame-based systems and consume a stable input state from the fixed-step simulation.

Avoid hidden global state. Prefer explicit resources and clear system ordering when behavior depends on execution order.

Only add explicit system ordering when correctness requires it. Otherwise, keep systems parallel-friendly and let Bevy schedule them based on data access.
