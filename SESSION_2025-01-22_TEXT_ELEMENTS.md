# Session 2025-01-22: Text Elements Implementation

## Session Overview

**Goal:** Add configurable text elements (plot title, X-axis label, Y-axis label) to plots using the GGRS DecorationLayout system.

**Approach:** Option A - Use DecorationLayout (proper architecture, requires migration from inline legend rendering)

**Status:** Phase 1 Complete (Theme Integration) - Ready for Phase 2 (DecorationLayout Migration)

---

## What Was Completed

### ✅ Phase 1: Theme Integration (GGRS)

All changes in `/home/thiago/workspaces/tercen/main/ggrs/`

#### 1.1 Theme Helper Methods

**File:** `crates/ggrs-core/src/theme/mod.rs`

Added color extraction methods:

```rust
/// Get plot title text color
pub fn plot_title_color(&self) -> RGBColor {
    if let Element::Text(text) = &self.plot_title {
        parse_color(&text.colour)
    } else {
        RGBColor(0, 0, 0) // Default black
    }
}

/// Get axis title text color
pub fn axis_title_color(&self) -> RGBColor {
    if let Element::Text(text) = &self.axis_title {
        parse_color(&text.colour)
    } else {
        RGBColor(0, 0, 0) // Default black
    }
}
```

**Note:** `title_color()` already existed (line 562) - aliases to `plot_title_color()`

#### 1.2 TextGrob Theme Factory Methods

**File:** `crates/ggrs-core/src/grobs/text.rs`

Added theme-aware factory methods:

```rust
/// Create text grob from theme Element::Text
pub fn from_theme_element(
    text: String,
    element: &crate::theme::elements::Element,
    align: TextAlign,
    valign: VAlign,
) -> Option<Self>

/// Create plot title from theme
pub fn plot_title_from_theme(text: String, theme: &crate::theme::Theme) -> Option<Self>

/// Create X-axis label from theme
pub fn x_label_from_theme(text: String, theme: &crate::theme::Theme) -> Option<Self>

/// Create Y-axis label from theme
pub fn y_label_from_theme(text: String, theme: &crate::theme::Theme) -> Option<Self>
```

These methods:
- Read font family, size, and color from theme Elements
- Use `parse_color()` to handle named colors and hex codes
- Return `Option<TextGrob>` (None if Element is blank)
- Set appropriate alignment (Center for title, Center for X-axis)

#### 1.3 Unit Tests

**File:** `crates/ggrs-core/src/grobs/text.rs`

Added tests (note: existing GGRS test infrastructure has unrelated DataFrame API issues):

```rust
#[test]
fn test_text_grob_from_theme() { ... }

#[test]
fn test_x_label_from_theme() { ... }

#[test]
fn test_y_label_from_theme() { ... }
```

**Build Status:** ✅ Library builds successfully (`cargo build --lib --release`)

---

## Architecture Analysis

### Current State

**DecorationLayout exists but is NOT used:**

- **File:** `ggrs/crates/ggrs-core/src/decoration_layout.rs`
- **Status:** Fully implemented, well-designed
- **Capabilities:**
  - Measures decorations (title, caption, legend) using `Grob::measure()`
  - Reserves space dynamically
  - Composites panels + decorations on final surface
  - Already supports `title`, `caption`, `legend`

**Current Rendering (render.rs):**

- Legend is drawn **inline** on final surface (lines 740-850)
- Does NOT use DecorationLayout
- Does NOT use grob system
- Hardcoded positioning with some theme integration

### Why Migration is Needed

**Current Problems:**
1. ❌ Legend rendering bypasses layout system
2. ❌ No support for plot title, axis labels
3. ❌ Hardcoded spacing (10px, 15px, 30px)
4. ❌ No clean separation between panels and decorations

**After Migration:**
1. ✅ Clean grob-based architecture
2. ✅ Proper space reservation
3. ✅ Support for all text elements
4. ✅ Easier to test and maintain

---

## Next Phase: DecorationLayout Migration

### Current Challenge

**render.rs line 740-850** draws legend inline:

```rust
// Draw legend directly on the same surface (no intermediate surface!)
if legend_scale.has_legend() {
    eprintln!("DEBUG: Drawing legend directly on final surface");

    // Calculate legend position... (lines 745-779)
    // Draw legend using draw_continuous_legend() or draw_discrete_legend()
}
```

**This needs to be converted to:**

```rust
// Build legend grob
let legend_grob = if legend_scale.has_legend() {
    Some(LegendGrob::from_scale(legend_scale, theme))
} else {
    None
};

// Build text grobs
let title_grob = spec.title.as_ref()
    .and_then(|t| TextGrob::plot_title_from_theme(t.clone(), theme));

let x_label_grob = spec.x_label.as_ref()
    .and_then(|l| TextGrob::x_label_from_theme(l.clone(), theme));

let y_label_grob = spec.y_label.as_ref()
    .and_then(|l| TextGrob::y_label_from_theme(l.clone(), theme)
        .map(|g| g.with_rotation(90.0)));

// Use DecorationLayout
let final_surface = DecorationLayout::add_decorations(
    panel_surface,
    panel_area,
    theme,
    legend_grob.as_ref(),
    &theme.legend_position,
    title_grob.as_ref(),
    x_label_grob.as_ref(),
    y_label_grob.as_ref(),
    None, // caption
)?;
```

### Required Changes

#### Step 1: Create LegendGrob (Missing!)

Currently `draw_continuous_legend()` and `draw_discrete_legend()` are functions, not grobs.

**Need to create:** `ggrs/crates/ggrs-core/src/grobs/legend.rs`

```rust
pub struct LegendGrob {
    scale: LegendScale,
    position: LegendPosition,
    justification: Option<(f64, f64)>,
}

impl LegendGrob {
    pub fn from_scale(
        scale: LegendScale,
        position: LegendPosition,
        justification: Option<(f64, f64)>,
    ) -> Self { ... }
}

impl Grob for LegendGrob {
    fn measure(&self, theme: &Theme) -> Size { ... }
    fn draw(&self, ctx: &Context, x: i32, y: i32) -> Result<(), String> {
        // Call draw_continuous_legend or draw_discrete_legend
        // But with context, not root surface
    }
}
```

**Problem:** Current `draw_continuous_legend()` signature:

```rust
fn draw_continuous_legend<F>(
    root: &DrawingArea<BitMapBackend, Shift>,  // ❌ Plotters-specific
    min: f64,
    max: f64,
    title: &str,
    theme: &Theme,
    legend_x: i32,
    legend_y: i32,
    get_color: F,
) -> Result<()>
```

This uses **Plotters** `DrawingArea`, but `Grob::draw()` uses **Cairo** `Context`.

**Solution:** Need to either:
- A) Rewrite legend drawing to use Cairo (complex)
- B) Keep dual rendering paths temporarily (quick)

#### Step 2: Add Rotation Support to TextGrob

**File:** `ggrs/crates/ggrs-core/src/grobs/text.rs`

```rust
pub struct TextGrob {
    // ... existing fields ...
    rotation: f64,  // NEW: rotation in degrees
}

impl TextGrob {
    pub fn with_rotation(mut self, degrees: f64) -> Self {
        self.rotation = degrees;
        self
    }
}

// Update draw() to apply rotation
fn draw(&self, ctx: &Context, x: i32, y: i32) -> Result<(), String> {
    ctx.save()?;

    if self.rotation != 0.0 {
        ctx.translate(x as f64, y as f64);
        ctx.rotate(self.rotation * std::f64::consts::PI / 180.0);
        // ... draw at (0, 0) after rotation
    } else {
        // ... existing drawing code
    }

    ctx.restore()?;
    Ok(())
}
```

#### Step 3: Update DecorationLayout Signature

**File:** `ggrs/crates/ggrs-core/src/decoration_layout.rs`

Already has this signature:

```rust
pub fn add_decorations(
    panel_surface: ImageSurface,
    panel_area: Rect,
    theme: &Theme,
    legend: Option<&LegendGrob>,
    legend_position: &crate::theme::LegendPosition,
    title: Option<&TextGrob>,
    caption: Option<&TextGrob>,
) -> Result<ImageSurface>
```

**Need to add:**

```rust
pub fn add_decorations(
    panel_surface: ImageSurface,
    panel_area: Rect,
    theme: &Theme,
    legend: Option<&LegendGrob>,
    legend_position: &crate::theme::LegendPosition,
    title: Option<&TextGrob>,
    x_label: Option<&TextGrob>,  // NEW
    y_label: Option<&TextGrob>,  // NEW
    caption: Option<&TextGrob>,
) -> Result<ImageSurface>
```

**Measure and reserve space:**

```rust
let x_label_size = x_label.map(|xl| xl.measure(theme)).unwrap_or(Size::zero());
let y_label_size = y_label.map(|yl| yl.measure(theme)).unwrap_or(Size::zero());

let x_label_height = if x_label_size.height > 0 {
    x_label_size.height + spacing
} else {
    0
};

// Y-label is rotated 90°, so height becomes width!
let y_label_width = if y_label_size.height > 0 {
    y_label_size.height + spacing
} else {
    0
};

let total_width = y_label_width + surface_width + legend_width_reserve;
let total_height = title_height + surface_height + x_label_height + ...;
```

**Render labels:**

```rust
// X-axis label (bottom, centered)
if let Some(x_label_grob) = x_label {
    let x_label_x = (total_width / 2) as i32;
    let x_label_y = (title_height + surface_height) as i32;
    x_label_grob.draw(&ctx, x_label_x, x_label_y)?;
}

// Y-axis label (left, rotated 90°, centered vertically)
if let Some(y_label_grob) = y_label {
    let y_label_x = 10; // Left margin
    let y_label_y = title_height + (surface_height / 2);
    y_label_grob.draw(&ctx, y_label_x as i32, y_label_y as i32)?;
}
```

#### Step 4: Migrate render.rs to Use DecorationLayout

**File:** `ggrs/crates/ggrs-core/src/render.rs`

**Current flow (simplified):**

```rust
// 1. Create final surface with reserved space for legend
let final_surface = create_surface_with_legend_reserve(...);

// 2. Render all panels to surface
render_panels(&final_surface, ...);

// 3. Draw legend inline (lines 740-850)
draw_legend_inline(&final_surface, ...);

// 4. Save PNG
save_png(final_surface);
```

**Target flow:**

```rust
// 1. Create panel surface (no legend reserve)
let panel_surface = create_panel_surface(...);

// 2. Render all panels to panel surface
render_panels(&panel_surface, ...);

// 3. Build grobs
let legend_grob = build_legend_grob(...);
let title_grob = TextGrob::plot_title_from_theme(...);
let x_label_grob = TextGrob::x_label_from_theme(...);
let y_label_grob = TextGrob::y_label_from_theme(...)
    .map(|g| g.with_rotation(90.0));

// 4. Composite using DecorationLayout
let final_surface = DecorationLayout::add_decorations(
    panel_surface,
    panel_area,
    theme,
    legend_grob.as_ref(),
    &theme.legend_position,
    title_grob.as_ref(),
    x_label_grob.as_ref(),
    y_label_grob.as_ref(),
    None,
)?;

// 5. Save PNG
save_png(final_surface);
```

**This is a significant refactor** because:
- Current code calculates legend reserve upfront
- Current code draws legend with Plotters
- Need to separate panel rendering from decoration rendering

---

## Operator Integration (After GGRS Migration)

Once DecorationLayout is working in GGRS, operator changes are simple:

### Step 1: Add Properties

**File:** `operator.json`

```json
{
  "kind": "StringProperty",
  "name": "plot.title",
  "defaultValue": "",
  "description": "Plot title"
},
{
  "kind": "StringProperty",
  "name": "axis.x.label",
  "defaultValue": "",
  "description": "X-axis label"
},
{
  "kind": "StringProperty",
  "name": "axis.y.label",
  "defaultValue": "",
  "description": "Y-axis label"
}
```

### Step 2: Config Parsing

**File:** `src/config.rs`

```rust
pub struct PlotConfig {
    // ... existing ...
    pub plot_title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
}

impl PlotConfig {
    pub fn from_properties(props: &PropertyReader) -> Self {
        let plot_title = props.get_string("plot.title", "");
        let plot_title = if plot_title.is_empty() { None } else { Some(plot_title) };

        // Same for x_label, y_label
    }
}
```

### Step 3: Wire to PlotSpec

**Files:** `src/bin/test_stream_generator.rs`, `src/main.rs`

```rust
let mut plot_spec = EnginePlotSpec::new()
    .add_layer(Geom::point_sized(config.point_size as f64))
    .theme(theme);

if let Some(title) = &config.plot_title {
    plot_spec = plot_spec.title(title);
}
if let Some(x_label) = &config.x_label {
    plot_spec = plot_spec.x_label(x_label);
}
if let Some(y_label) = &config.y_label {
    plot_spec = plot_spec.y_label(y_label);
}
```

**Note:** `EnginePlotSpec` (from `engine.rs`) already has these fields:

```rust
pub struct PlotSpec {
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    // ...
}
```

So no changes needed to PlotSpec itself!

---

## Known Issues

### 1. Hardcoded Values in GGRS

Found during analysis:

```rust
// render.rs
let margin_left = max_y_label_width + 15;  // Hardcoded 15px
let margin_top = if n_col_levels > 0 { 10 + ... };  // Hardcoded 10px
let cell_spacing = 10;  // Hardcoded 10px

// decoration_layout.rs
let spacing = 10;  // TODO: Get from theme

// grobs/text.rs
TextGrob::title() { font_size: 16.0, ... }  // Hardcoded (not used after our changes)
```

**Recommendation:** Add to theme in a follow-up:
- `theme.spacing` or `theme.panel_spacing_x/y`
- `theme.margin.left/right/top/bottom`

### 2. Pre-existing Test Failures

GGRS test suite has DataFrame API incompatibilities (polars version mismatch):

```
error[E0599]: no function or associated item named `from_polars` found
error[E0308]: mismatched types - expected `Column`, found `Series`
```

These are **NOT caused by our changes**. Library builds fine with `cargo build --lib`.

### 3. Legend Drawing Uses Plotters, Not Cairo

`draw_continuous_legend()` uses:
```rust
root: &DrawingArea<BitMapBackend, Shift>  // Plotters
```

But `Grob::draw()` uses:
```rust
ctx: &Context  // Cairo
```

**Options:**
- A) Keep dual rendering (Plotters for legend, Cairo for text) - Quick
- B) Rewrite legend with Cairo - Proper but complex

---

## Recommendation for Fresh Session

**YES - Recommend a clean session** for the DecorationLayout migration because:

1. **Complexity:** This is a significant architectural refactor
2. **Scope:** Touches multiple files (render.rs, decoration_layout.rs, grobs/legend.rs)
3. **Risk:** Changes core rendering pipeline
4. **Testing:** Needs careful validation at each step

### Fresh Session Plan

**Session Goal:** Migrate from inline legend rendering to DecorationLayout with text element support

**Approach:** Bottom-up with incremental testing

**Phases:**

1. **Phase 1:** Add rotation to TextGrob (15 min)
   - Add `rotation` field
   - Update `draw()` with Cairo rotation
   - Test with simple rotated text

2. **Phase 2:** Create LegendGrob wrapper (30 min)
   - Create thin wrapper around existing legend drawing
   - Implement `Grob::measure()` and `Grob::draw()`
   - Keep Plotters rendering temporarily

3. **Phase 3:** Update DecorationLayout for X/Y labels (20 min)
   - Add x_label, y_label parameters
   - Measure and reserve space
   - Render text grobs

4. **Phase 4:** Refactor render.rs panel/decoration separation (60 min)
   - Separate panel surface creation
   - Remove inline legend drawing
   - Call DecorationLayout::add_decorations()
   - Test with existing plots

5. **Phase 5:** Operator integration (20 min)
   - Add properties
   - Wire config to PlotSpec
   - End-to-end test

**Total:** ~2.5 hours

**First Checkpoint:** After Phase 1 - can render rotated text
**Second Checkpoint:** After Phase 3 - DecorationLayout works with text
**Third Checkpoint:** After Phase 4 - Full plot with all decorations
**Final Checkpoint:** After Phase 5 - Operator integration complete

---

## Files Modified in This Session

### GGRS (ggrs/crates/ggrs-core/src/)

1. ✅ `theme/mod.rs` - Added `plot_title_color()`, `axis_title_color()`
2. ✅ `grobs/text.rs` - Added `from_theme_element()`, `plot_title_from_theme()`, `x_label_from_theme()`, `y_label_from_theme()`, unit tests

### Operator

1. ✅ `Cargo.toml` - Using local ggrs path (for development)
2. ✅ Built successfully with new GGRS

---

## Context for Next Session

**What's Ready:**
- ✅ Theme integration complete
- ✅ TextGrob can be created from theme
- ✅ Operator builds with new GGRS
- ✅ DecorationLayout exists and is well-designed

**What's Needed:**
- ❌ Rotation support in TextGrob
- ❌ LegendGrob wrapper
- ❌ DecorationLayout extended for X/Y labels
- ❌ render.rs migration from inline to DecorationLayout
- ❌ Operator properties and config

**Key Decision Made:**
- Going with **Option A: Use DecorationLayout** (proper architecture)
- This requires refactoring current inline legend rendering
- Worth it for clean, maintainable, extensible architecture

**Critical Files to Review:**
- `ggrs/crates/ggrs-core/src/decoration_layout.rs` - The layout system
- `ggrs/crates/ggrs-core/src/render.rs` lines 740-850 - Current inline legend code
- `ggrs/crates/ggrs-core/src/render.rs` lines 1881-2150 - `draw_continuous_legend()` and `draw_discrete_legend()`

**Estimated Time to Complete:** 2-3 hours with testing

Good luck with the migration! The architecture is solid, and the incremental testing approach should make it manageable.
