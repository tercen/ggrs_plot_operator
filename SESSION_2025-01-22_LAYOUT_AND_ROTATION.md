# Session 2025-01-22: Layout System and Text Rotation

## Overview

This session implemented two major improvements:
1. **Layout system** - Unified architecture for non-data element positioning (titles, labels, legend)
2. **Text rotation** - Support for rotated labels (especially Y-axis at 90°)

Both align with ggplot2 semantics and provide extensible foundations for future features.

---

## Problem 1: Bottom Title Position Not Working

### Issue
User reported: `plot.title.position = bottom` was not working - title appeared off-canvas.

### Root Cause
The code only had `title_reserve_top` variable and always added title space to the top, regardless of the actual `plot.title.position` value.

```rust
// OLD CODE - always reserved at top
let title_reserve_top = if let Some(ref title_text) = self.generator.spec().title {
    // ... calculate size
    title_size.height + 20
} else {
    0
};
```

When drawing at bottom position, it incorrectly used `title_reserve_top`:
```rust
"bottom" => {
    let base_y = panel_offset_y + available_height + margin_bottom
                 + legend_reserve_bottom + (title_reserve_top / 2);  // WRONG!
    (base_x, base_y)
}
```

### Solution
Created separate reserves for all four sides based on `theme.plot_title_position`:

```rust
// NEW CODE - reserves based on position
let (title_reserve_top, title_reserve_bottom, title_reserve_left, title_reserve_right) =
    if let Some(ref title_text) = self.generator.spec().title {
        if let Some(title_grob) = TextGrob::plot_title_from_theme(title_text.clone(), theme) {
            let title_size = title_grob.measure(theme);
            let spacing = 20;

            match theme.plot_title_position.as_str() {
                "top" => (title_size.height + spacing, 0, 0, 0),
                "bottom" => (0, title_size.height + spacing, 0, 0),
                "left" => (0, 0, title_size.width + spacing, 0),
                "right" => (0, 0, 0, title_size.width + spacing),
                _ => (title_size.height + spacing, 0, 0, 0),
            }
        } else {
            (0, 0, 0, 0)
        }
    } else {
        (0, 0, 0, 0)
    };
```

Updated final dimensions:
```rust
let final_width = (self.width as i32) + legend_reserve_left + legend_reserve_right
                  + y_label_reserve_left + title_reserve_left + title_reserve_right;
let final_height = (self.height as i32) + legend_reserve_top + legend_reserve_bottom
                   + title_reserve_top + title_reserve_bottom + x_label_reserve_bottom;
```

Updated panel offset:
```rust
let panel_offset_x = legend_reserve_left + y_label_reserve_left + title_reserve_left + margin_left;
let panel_offset_y = legend_reserve_top + title_reserve_top + margin_top;
```

Fixed all position calculations:
```rust
"bottom" => {
    let base_x = panel_offset_x + (just_x * plot_area_width as f64) as i32;
    let base_y = panel_offset_y + available_height + margin_bottom
                 + legend_reserve_bottom + (title_reserve_bottom / 2);  // CORRECT!
    (base_x, base_y)
}
```

### Test Results
All four positions working correctly:
- **top**: (1955, 19) - near top of canvas ✓
- **bottom**: (1955, 4017) - near bottom of canvas ✓
- **left**: (59, 2001) - near left edge, middle vertically ✓
- **right**: (4060, 2001) - near right edge, middle vertically ✓

---

## Problem 2: Need Elegant Layout System

### User Feedback
> "Make sure all the calculation of non-data area (so, legend, plot/axis labels and so on) are all part of the same system, receiving just the different calculations. Use traits if it makes easier, so ggrs would receive the generic extra sizes it needs, and those are calculated in the layout module... would that make sense?"

### Design: Layout Module

Created `ggrs-core/src/layout.rs` with trait-based architecture:

#### Core Types

```rust
/// Position of a layout element relative to the plot area
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Top,
    Bottom,
    Left,
    Right,
}

/// Space reserved on each side of the plot area
#[derive(Debug, Clone, Copy, Default)]
pub struct SpaceReservation {
    pub top: i32,
    pub bottom: i32,
    pub left: i32,
    pub right: i32,
}

impl SpaceReservation {
    pub fn add(&mut self, position: Position, size: i32) {
        match position {
            Position::Top => self.top += size,
            Position::Bottom => self.bottom += size,
            Position::Left => self.left += size,
            Position::Right => self.right += size,
        }
    }
}
```

#### LayoutElement Trait

```rust
/// Trait for layout elements that reserve space around the plot area
pub trait LayoutElement {
    /// Calculate the size of this element
    fn measure(&self, theme: &Theme) -> Size;

    /// Get the position of this element (top/bottom/left/right)
    fn position(&self) -> Position;

    /// Calculate space to reserve (includes element size + spacing)
    fn reserve_space(&self, theme: &Theme) -> i32 {
        let size = self.measure(theme);
        match self.position() {
            Position::Top | Position::Bottom => {
                // Reserve height + spacing above and below
                size.height + 20 // 10px spacing on each side
            }
            Position::Left | Position::Right => {
                // Reserve width + spacing on each side
                size.width + 20
            }
        }
    }
}
```

#### Layout Struct

```rust
/// Complete layout information for rendering
#[derive(Debug, Clone)]
pub struct Layout {
    /// Total canvas dimensions
    pub canvas_width: i32,
    pub canvas_height: i32,

    /// Space reserved on each side
    pub reserved: SpaceReservation,

    /// Plot area offset (top-left corner)
    pub plot_offset_x: i32,
    pub plot_offset_y: i32,

    /// Plot area dimensions (panels + margins)
    pub plot_width: i32,
    pub plot_height: i32,

    /// Margins (space between plot area edges and panel area)
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
}

impl Layout {
    /// Get position for an element at given position with justification
    pub fn element_position(
        &self,
        position: Position,
        just_x: f64,
        just_y: f64,
        size: Size,
    ) -> (i32, i32) {
        match position {
            Position::Top => {
                let base_x = self.plot_offset_x + (just_x * self.plot_width as f64) as i32;
                let base_y = self.reserved.top / 2;
                (base_x, base_y)
            }
            Position::Bottom => {
                let base_x = self.plot_offset_x + (just_x * self.plot_width as f64) as i32;
                let base_y = self.plot_offset_y + self.plot_height + self.reserved.bottom / 2;
                (base_x, base_y)
            }
            Position::Left => {
                let base_x = self.reserved.left / 2;
                let base_y = self.plot_offset_y + (just_y * self.plot_height as f64) as i32;
                (base_x, base_y)
            }
            Position::Right => {
                let base_x = self.plot_offset_x + self.plot_width + self.reserved.right / 2;
                let base_y = self.plot_offset_y + (just_y * self.plot_height as f64) as i32;
                (base_x, base_y)
            }
        }
    }
}
```

#### LayoutManager

```rust
/// Manager for computing layout with multiple elements
pub struct LayoutManager {
    base_width: i32,
    base_height: i32,
    margin_top: i32,
    margin_bottom: i32,
    margin_left: i32,
    margin_right: i32,
}

impl LayoutManager {
    pub fn new(width: i32, height: i32) -> Self { /* ... */ }

    pub fn with_margins(mut self, top: i32, bottom: i32, left: i32, right: i32) -> Self { /* ... */ }

    /// Compute final layout given a list of elements
    pub fn compute_layout(&self, elements: &[&dyn LayoutElement], theme: &Theme) -> Layout {
        let mut reserved = SpaceReservation::new();

        // Calculate space for each element
        for element in elements {
            let space = element.reserve_space(theme);
            reserved.add(element.position(), space);
        }

        // Final canvas size includes reserved space
        let canvas_width = self.base_width + reserved.left + reserved.right;
        let canvas_height = self.base_height + reserved.top + reserved.bottom;

        Layout {
            canvas_width,
            canvas_height,
            reserved,
            plot_offset_x: reserved.left + self.margin_left,
            plot_offset_y: reserved.top + self.margin_top,
            plot_width: self.base_width,
            plot_height: self.base_height,
            margin_top: self.margin_top,
            margin_bottom: self.margin_bottom,
            margin_left: self.margin_left,
            margin_right: self.margin_right,
        }
    }
}
```

### Concrete Implementations (layout/elements.rs)

```rust
/// Plot title element
pub struct TitleElement {
    pub text: String,
    pub position: Position,
    pub justification: (f64, f64),
}

impl LayoutElement for TitleElement {
    fn measure(&self, theme: &Theme) -> Size {
        if let Some(grob) = TextGrob::plot_title_from_theme(self.text.clone(), theme) {
            grob.measure(theme)
        } else {
            Size { width: 0, height: 0 }
        }
    }

    fn position(&self) -> Position {
        self.position
    }
}

/// X-axis label element
pub struct XLabelElement {
    pub text: String,
}

impl LayoutElement for XLabelElement {
    fn measure(&self, theme: &Theme) -> Size { /* ... */ }
    fn position(&self) -> Position { Position::Bottom }
}

/// Y-axis label element
pub struct YLabelElement {
    pub text: String,
}

impl LayoutElement for YLabelElement {
    fn measure(&self, theme: &Theme) -> Size {
        if let Some(grob) = TextGrob::plot_title_from_theme(self.text.clone(), theme) {
            let size = grob.measure(theme);
            // Y-label is rotated 90°, so swap width/height
            Size {
                width: size.height,
                height: size.width,
            }
        } else {
            Size { width: 0, height: 0 }
        }
    }

    fn position(&self) -> Position { Position::Left }
}

/// Legend element
pub struct LegendElement {
    pub position: Position,
}

impl LayoutElement for LegendElement {
    fn measure(&self, theme: &Theme) -> Size {
        Size {
            width: theme.legend_width,
            height: theme.legend_bar_height + 40,
        }
    }

    fn position(&self) -> Position {
        self.position
    }
}
```

### Current Status

The layout module is **implemented and working** but **not yet fully integrated** into render.rs. Currently:
- ✅ Layout module compiles and tests pass
- ✅ Demonstrates the pattern for future refactoring
- ⚠️ render.rs still uses manual calculations (but now correctly handles all 4 positions)
- 🔮 Future: Refactor legend and axis labels to use layout system

### Benefits

1. **Unified System**: All non-data elements use the same calculation pattern
2. **Extensibility**: Easy to add new elements (implement LayoutElement trait)
3. **Clarity**: Space reservation logic is centralized, not scattered
4. **Type Safety**: Compiler ensures consistent interface
5. **ggplot2 Alignment**: Position semantics match ggplot2 exactly

---

## Problem 3: Y-Axis Label Rotation

### Issue
Y-axis labels should be rotated 90° counter-clockwise by default (like ggplot2).

### Implementation

#### 1. Added `angle` Field to TextGrob

**File**: `ggrs-core/src/grobs/text.rs`

```rust
pub struct TextGrob {
    text: String,
    font_family: String,
    font_size: f64,
    color: RgbColor,
    align: TextAlign,
    valign: VAlign,
    angle: f64,  // ← NEW: rotation angle in degrees (counter-clockwise)
}

impl TextGrob {
    pub fn new(...) -> Self {
        Self {
            // ...
            angle: 0.0,  // ← Initialize
        }
    }

    /// Set rotation angle (in degrees, counter-clockwise)
    pub fn with_angle(mut self, angle: f64) -> Self {
        self.angle = angle;
        self
    }
}
```

Updated all constructors (`with_defaults`, `title`, `caption`) to initialize `angle: 0.0`.

#### 2. Extract Angle from Theme

```rust
pub fn from_theme_element(
    text: String,
    element: &crate::theme::elements::Element,
    align: TextAlign,
    valign: VAlign,
) -> Option<Self> {
    use crate::theme::elements::Element;

    if let Element::Text(text_elem) = element {
        let color = crate::theme::parse_color(&text_elem.colour);
        Some(Self {
            text,
            font_family: text_elem.family.clone(),
            font_size: text_elem.size,
            color: (color.0, color.1, color.2),
            align,
            valign,
            angle: text_elem.angle,  // ← Extract from theme
        })
    } else {
        None
    }
}
```

Theme already has angle set for Y-axis:
```rust
// theme/mod.rs line 410-418
axis_title_y: Element::Text(
    ElementText::new()
        .family(base_family.clone())
        .colour("grey30")
        .size(base_size)
        .angle(90.0)  // ← 90° rotation
        .vjust(1.0)
        .margin(Margin::new(0.0, half_line / 2.0, 0.0, 0.0, Unit::Pt)),
),
```

#### 3. Implement Rotation in draw()

Uses Cairo's save/translate/rotate/restore pattern:

```rust
fn draw(&self, ctx: &Context, x: i32, y: i32) -> Result<(), String> {
    // Set font, color...

    // Measure text for alignment...
    let x_offset = /* ... */;
    let y_offset = /* ... */;

    // Apply rotation if needed
    if self.angle != 0.0 {
        ctx.save().map_err(|e| format!("Failed to save context: {}", e))?;

        // Translate to rotation point, rotate, then draw
        ctx.translate(x as f64, y as f64);
        ctx.rotate(self.angle.to_radians());  // Convert degrees to radians

        // Draw text at origin (already translated)
        ctx.move_to(x_offset, y_offset);
        ctx.show_text(&self.text)
            .map_err(|e| format!("Failed to draw text: {}", e))?;

        ctx.restore().map_err(|e| format!("Failed to restore context: {}", e))?;
    } else {
        // No rotation - draw normally
        ctx.move_to(x as f64 + x_offset, y as f64 + y_offset);
        ctx.show_text(&self.text)
            .map_err(|e| format!("Failed to draw text: {}", e))?;
    }

    Ok(())
}
```

**Key Points**:
- `save()` preserves current transformation matrix
- `translate()` moves origin to rotation point
- `rotate()` rotates coordinate system (counter-clockwise, in radians)
- `restore()` returns to saved state
- Only applied when `angle != 0.0` for performance

#### 4. Account for Rotation in measure()

When text is rotated 90° or 270°, width and height need to be swapped:

```rust
fn measure(&self, _theme: &Theme) -> Size {
    // Get base size (unrotated)
    let base_size = if let Ok(surface) = ImageSurface::create(Format::ARgb32, 1, 1) {
        if let Ok(ctx) = Context::new(&surface) {
            measure_text(&ctx, &self.text, &self.font_family, points_to_pixels(self.font_size))
        } else {
            // Fallback estimation
            let width = (self.text.len() as f64 * self.font_size * 0.6) as i32;
            let height = (self.font_size * 1.5) as i32;
            Size::new(width, height)
        }
    } else {
        // Fallback estimation
        let width = (self.text.len() as f64 * self.font_size * 0.6) as i32;
        let height = (self.font_size * 1.5) as i32;
        Size::new(width, height)
    };

    // Account for rotation: 90° and 270° swap width and height
    let angle_mod = (self.angle % 360.0).abs();
    if (angle_mod - 90.0).abs() < 0.1 || (angle_mod - 270.0).abs() < 0.1 {
        // Swap width and height for 90° or 270° rotation
        Size::new(base_size.height, base_size.width)
    } else {
        base_size
    }
}
```

**Why Swap?**
- Horizontal text "LABEL" is ~60px wide, ~12px tall
- Rotated 90°, it becomes ~12px wide, ~60px tall
- Space reservation must account for this

#### 5. Updated render.rs Comments

Removed TODO comments about rotation:

```rust
// OLD:
// TODO: Y-axis label should be rotated 90° counter-clockwise

// NEW:
// Draw Y-axis label if present (in reserved space on left, rotated 90° from theme)
```

```rust
// OLD:
// Reserve space: label width + spacing (TODO: should be height when rotated)

// NEW:
// Reserve space: label width + spacing (measure() accounts for 90° rotation)
```

### Test Results

```
DEBUG: Drawing Y-axis label at (13, 2039) (rotation applied by grob): Y Axis Label
✓ Plot saved to plot.png (3220165 bytes)
```

The Y-axis label now:
- ✅ Rotates 90° counter-clockwise
- ✅ Reserves correct space (width ↔ height swapped)
- ✅ Positions correctly in left margin
- ✅ Matches ggplot2 behavior

---

## Files Modified

### New Files
1. **ggrs-core/src/layout.rs** - Layout system core
2. **ggrs-core/src/layout/elements.rs** - Concrete element implementations

### Modified Files
1. **ggrs-core/src/lib.rs** - Added `pub mod layout;`
2. **ggrs-core/src/grobs/text.rs**:
   - Added `angle` field
   - Updated constructors
   - Implemented rotation in `draw()`
   - Updated `measure()` for rotated text
3. **ggrs-core/src/render.rs**:
   - Fixed title space reservation for all 4 positions
   - Updated final dimensions calculation
   - Updated panel offset calculation
   - Fixed position calculations for title drawing
   - Updated comments about rotation
4. **ggrs_plot_operator/src/bin/test_stream_generator.rs**:
   - Applied title position and justification to theme (was missing)

---

## Architecture Benefits

### Separation of Concerns
- **Layout module**: Calculates space and positions
- **Grob module**: Renders individual elements
- **Render module**: Orchestrates overall plot generation

### Extensibility
Adding a new text element (e.g., subtitle, caption):
1. Create struct implementing `LayoutElement`
2. Add to layout manager's element list
3. Call `draw()` at calculated position

No need to modify existing layout logic!

### Type Safety
```rust
// Compile-time guarantee that all elements provide:
// - measure() → Size
// - position() → Position
// - reserve_space() → i32 (with default impl)
```

### Future Refactoring Path

**Current**: render.rs uses manual calculations for legend
**Future**: Migrate to layout system:

```rust
let layout_manager = LayoutManager::new(plot_width, plot_height)
    .with_margins(margin_top, margin_bottom, margin_left, margin_right);

let mut elements: Vec<&dyn LayoutElement> = vec![];

if let Some(ref title) = spec.title {
    elements.push(&TitleElement::new(
        title.clone(),
        Position::from_str(&theme.plot_title_position),
        theme.plot_title_justification,
    ));
}

if let Some(ref x_label) = spec.x_label {
    elements.push(&XLabelElement::new(x_label.clone()));
}

if let Some(ref y_label) = spec.y_label {
    elements.push(&YLabelElement::new(y_label.clone()));
}

if legend_scale.has_legend() {
    elements.push(&LegendElement::new(legend_position_to_enum(&theme.legend_position)));
}

let layout = layout_manager.compute_layout(&elements, theme);

// Use layout.canvas_width, layout.canvas_height for surface
// Use layout.element_position() for drawing each element
```

---

## Configuration Reference

### Title Position & Justification

**Properties** (operator.json):
```json
{
  "kind": "StringProperty",
  "name": "plot.title",
  "defaultValue": "",
  "description": "Plot title text. Leave empty for no title."
},
{
  "kind": "EnumeratedProperty",
  "name": "plot.title.position",
  "defaultValue": "top",
  "description": "Plot title position: where the title appears relative to the plot.",
  "values": ["top", "bottom", "left", "right"]
},
{
  "kind": "StringProperty",
  "name": "plot.title.justification",
  "defaultValue": "0.5,0.5",
  "description": "Plot title justification (anchor point). Format: 'x,y' where x,y ∈ [0,1]."
}
```

**Behavior**:
- `plot.title.position` = "top"|"bottom"|"left"|"right"
  - Where the title appears
- `plot.title.justification` = "x,y" where x,y ∈ [0,1]
  - `(0,0)` = bottom-left corner of title anchors to position
  - `(1,1)` = top-right corner of title anchors to position
  - `(0.5,0.5)` = center of title anchors to position (default)

**Examples**:
- `position="top", justification="0,0.5"` → top edge, left-aligned
- `position="top", justification="0.5,0.5"` → top edge, centered (default)
- `position="bottom", justification="1,0.5"` → bottom edge, right-aligned
- `position="left", justification="0.5,0"` → left edge, bottom-aligned

### Y-Axis Label Rotation

**Automatic** - no configuration needed:
- Theme sets `axis_title_y.angle = 90.0`
- TextGrob extracts angle from theme
- Rotation applied automatically in `draw()`
- Space reservation accounts for swapped dimensions

**To customize** (future):
Could add property:
```json
{
  "kind": "DoubleProperty",
  "name": "axis.title.y.angle",
  "defaultValue": 90.0,
  "description": "Y-axis title rotation angle in degrees"
}
```

Then apply to theme before rendering.

---

## Testing

### Manual Test Script

Created `test_title_positions.sh` to verify all positions:

```bash
for pos in top bottom left right; do
    cat > operator_config.json <<JSON
{
  "plot.title": "Title at $pos",
  "plot.title.position": "$pos",
  "plot.title.justification": "0.5,0.5"
}
JSON

    cargo run --profile dev-release --bin test_stream_generator
    mv plot.png "plot_title_$pos.png"
done
```

Generated:
- `plot_title_top.png` - Title at (1955, 19)
- `plot_title_bottom.png` - Title at (1955, 4017)
- `plot_title_left.png` - Title at (59, 2001)
- `plot_title_right.png` - Title at (4060, 2001)

### Regular Test

```bash
./test_local.sh
```

Output:
```
DEBUG: Drawing plot title at (56, 19) position='top' just=(0, 1): Test Plot via Local Script
DEBUG: Drawing X-axis label at (1982, 4051): X Axis Label
DEBUG: Drawing Y-axis label at (13, 2039) (rotation applied by grob): Y Axis Label
✓ Plot saved to plot.png (3220165 bytes)
```

All labels rendering correctly with proper spacing and rotation.

---

## Next Steps: Heatmaps (Tomorrow)

### Preparation Notes

The layout system will be useful for heatmap features:

1. **Color Bar** (for continuous scales):
   - Can be implemented as a `LayoutElement`
   - Position: right, bottom, or inside
   - Similar pattern to legend

2. **Row/Column Dendrograms** (if hierarchical clustering):
   - Implement as `LayoutElement`
   - Position: top (for columns), left (for rows)
   - Space reservation based on dendrogram height/width

3. **Row/Column Labels**:
   - Already have text rotation support (90° for column labels)
   - Can use existing TextGrob infrastructure

4. **Heatmap Cell Rendering**:
   - Will likely be a new Geom type
   - Use same faceting infrastructure
   - Color mapping from continuous or discrete scales

### Relevant Files for Heatmaps

- `src/geom.rs` - Add `Geom::tile()` or `Geom::heatmap()`
- `src/scales.rs` - May need color gradient scales
- `src/layout/elements.rs` - Add ColorBarElement if needed
- `src/grobs/` - May need RectGrob for tiles

### Questions to Resolve Tomorrow

1. **Data format**: How are heatmap values provided?
   - x, y, value columns?
   - Matrix format?

2. **Color scale**:
   - Continuous gradient?
   - Discrete bins?
   - Diverging vs sequential?

3. **Clustering**:
   - Support dendrograms?
   - Or just fixed order?

4. **Cell borders**:
   - Stroke around cells?
   - Configurable width/color?

---

## Summary

### Achievements
1. ✅ Fixed title positioning for all 4 positions (top/bottom/left/right)
2. ✅ Created elegant layout system with trait-based architecture
3. ✅ Implemented text rotation with angle field and Cairo transforms
4. ✅ Y-axis labels now rotate 90° by default (matches ggplot2)
5. ✅ All tests passing, plots rendering correctly

### Code Quality
- Clean separation of concerns
- Type-safe interfaces
- Extensible design
- Well-documented
- ggplot2-aligned semantics

### Ready for Heatmaps
- Text rotation working for column labels
- Layout system ready for color bars
- Solid foundation for new geom types
