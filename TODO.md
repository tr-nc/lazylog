# Rules for CLAUDE

You should ONLY work on the item(s) that is marked with TODO, and nothing more.

# Future Refactoring Tasks for app.rs

## 3. Move Rendering Methods to Separate Files
Extract rendering methods to separate files since they are self-contained and only read state:

```
src/
  app/
    mod.rs           // App struct definition and core logic
    render.rs        // All render_* methods
    events.rs        // Event handling methods
    scrolling.rs     // All scrolling methods
    selection.rs     // Selection management
```

In `app/mod.rs`:
```rust
mod render;
mod events;
mod scrolling;
mod selection;
```

**Benefit**: This would reduce app.rs by ~600 lines and improve modularity.

## 4. Create Focused Sub-Structures
Extract related fields into smaller structs:

```rust
struct AppState {
    is_exiting: bool,
    autoscroll: bool,
    text_wrapping_enabled: bool,
    show_debug_logs: bool,
    show_help_popup: bool,
}

struct FilterState {
    input: String,
    focused: bool,
    engine: FilterEngine,
}

struct FocusState {
    hard_focused_block_id: uuid::Uuid,
    soft_focused_block_id: Option<uuid::Uuid>,
    prev_hard_focused_block_id: uuid::Uuid,
}

struct SelectionState {
    selected_log_uuid: Option<uuid::Uuid>,
    prev_selected_log_id: Option<uuid::Uuid>,
}
```

Then in App:
```rust
struct App {
    state: AppState,
    filter: FilterState,
    focus: FocusState,
    selection: SelectionState,
    // ... other fields
}
```

**Benefit**: Reduces App struct size and improves field organization.

## 5. Simplify Duplicated Rendering Pattern
The render methods for details/debug follow nearly identical patterns. Extract common logic:

```rust
fn render_scrollable_block<F>(
    &mut self,
    area: Rect,
    buf: &mut Buffer,
    block: &mut AppBlock,
    content_fn: F,
) -> Result<()>
where
    F: FnOnce(&Self, Rect, bool) -> (Vec<Line>, usize),
{
    // Common rendering logic here
}
```

**Benefit**: Reduces code duplication in rendering methods.
