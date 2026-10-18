//! User settings: everything the panel can change, persisted as JSON
//! (`config.rs`). New fields must have a serde default so old files load;
//! removed or renamed fields must be migrated in `config::migrate`, never
//! dropped.
//!
//! Model: a **visual** is a renderer (scope, ribbons, starfield); a **layer**
//! is one instance of a visual with its own parameters, opacity and audio
//! band; a **scene** is the composition of up to `MAX_LAYERS` layers plus the
//! post settings, saved by name (`scene_store.rs`).

use serde::{Deserialize, Serialize};

pub const MAX_LAYERS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualKind {
    Scope,
    Ribbons,
    Starfield,
    Kaleido,
    Sigil,
    Waves,
    Crystal,
}

impl VisualKind {
    pub const ALL: [VisualKind; 7] = [VisualKind::Scope, VisualKind::Ribbons, VisualKind::Starfield, VisualKind::Kaleido, VisualKind::Sigil, VisualKind::Waves, VisualKind::Crystal];
    pub fn name(self) -> &'static str {
        match self {
            VisualKind::Scope => "Phosphor scope",
            VisualKind::Ribbons => "Flow-field ribbons",
            VisualKind::Starfield => "Black hole",
            VisualKind::Kaleido => "Kaleidoscope",
            VisualKind::Sigil => "Sigil",
            VisualKind::Waves => "Waves",
            VisualKind::Crystal => "Crystal",
        }
    }
    /// Short display name (bar pips, stack rows, breadcrumb).
    pub fn short(self) -> &'static str {
        match self {
            VisualKind::Scope => "Scope",
            VisualKind::Ribbons => "Ribbons",
            VisualKind::Starfield => "Black hole",
            VisualKind::Kaleido => "Kaleidoscope",
            VisualKind::Sigil => "Sigil",
            VisualKind::Waves => "Waves",
            VisualKind::Crystal => "Crystal",
        }
    }
}

/// Mirror geometry of a kaleidoscope fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldMode {
    /// Two mirrors: N-fold symmetry around the apex.
    Radial,
    /// Three mirrors at 60 degrees: a triangular tiling of one cell.
    Tiled,
    /// Rotation only, no mirror: spinning blades.
    Pinwheel,
}

/// Kaleidoscope fold modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fold {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub mode: FoldMode,
    pub segments: u32,
    /// Rotation, radians per second at average energy.
    pub speed: f32,
    pub zoom: f32,
    /// Apex offset from the source's centre (image half-heights).
    pub offset: f32,
    /// How fast the apex wanders.
    pub wander: f32,
    /// 0 = fixed segments; N = step to a new count every N beats.
    pub step_beats: u32,
    /// How much energy, onsets and bass move it.
    pub react: f32,
}

impl Default for Fold {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, mode: FoldMode::Radial, segments: 8, speed: 0.15, zoom: 1.0, offset: 0.25, wander: 1.0, step_beats: 0, react: 1.0 }
    }
}

/// The pre-chain `fold` field (one build, 2026-10-17); read for migration only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyFold {
    pub enabled: bool,
    pub mode: FoldMode,
    pub segments: u32,
    pub speed: f32,
    pub zoom: f32,
    pub react: f32,
}

impl Default for LegacyFold {
    fn default() -> Self {
        Self { enabled: false, mode: FoldMode::Radial, segments: 8, speed: 0.15, zoom: 1.0, react: 1.0 }
    }
}

/// Feedback / echo modifier: last frame's result zoomed, spun and faded under this one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Feedback {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub decay: f32,
    /// Zoom rate per second (positive = outward).
    pub zoom: f32,
    /// Spin, radians per second.
    pub spin: f32,
    pub react: f32,
}

impl Default for Feedback {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, decay: 0.9, zoom: 0.25, spin: 0.1, react: 1.0 }
    }
}

/// Polar wrap modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Polar {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    /// false = wrap (x becomes angle), true = unwrap (the inverse).
    pub unwrap: bool,
    pub radius: f32,
    pub spin: f32,
    pub react: f32,
}

impl Default for Polar {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, unwrap: false, radius: 1.0, spin: 0.1, react: 1.0 }
    }
}

/// Ripple modifier: rings on beats, plus a standing sine warp.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ripple {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub amount: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub sine: f32,
    pub react: f32,
}

impl Default for Ripple {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, amount: 0.04, wavelength: 0.15, speed: 0.8, sine: 0.0, react: 1.0 }
    }
}

/// What colours an edge outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EdgeColour {
    /// The picture's own hue under the edge.
    #[default]
    Picture,
    /// The layer tint.
    Tint,
    /// The layer's palette, by the edge's direction; the palette's cycle turns it.
    Palette,
}

impl EdgeColour {
    pub const ALL: [EdgeColour; 3] = [EdgeColour::Picture, EdgeColour::Tint, EdgeColour::Palette];
    pub fn name(self) -> &'static str {
        match self {
            EdgeColour::Picture => "Picture",
            EdgeColour::Tint => "Tint",
            EdgeColour::Palette => "Palette",
        }
    }
}

/// Edge (Sobel outline) modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edge {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub strength: f32,
    /// Share of the original kept underneath.
    pub keep: f32,
    pub colour: EdgeColour,
    /// Pre-`colour` flag (one build, 2026-10-17); read for migration only.
    #[serde(skip_serializing)]
    pub tinted: bool,
    pub react: f32,
}

impl Default for Edge {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, strength: 1.5, keep: 0.0, colour: EdgeColour::Picture, tinted: false, react: 1.0 }
    }
}

/// Chromatic split modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChromaSplit {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub amount: f32,
    pub radial: bool,
    pub react: f32,
}

impl Default for ChromaSplit {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, amount: 0.012, radial: true, react: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskShape {
    Circle,
    Band,
    Hex,
}

/// Mask modifier: keep the picture inside (or outside) a soft shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mask {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub shape: MaskShape,
    pub radius: f32,
    pub soft: f32,
    pub invert: bool,
    pub react: f32,
}

impl Default for Mask {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, shape: MaskShape::Circle, radius: 0.7, soft: 0.1, invert: false, react: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FractalMode {
    /// Kaleidoscopic IFS: fold, rotate, scale, shift; the cell recurs at every scale.
    Kifs,
    /// z^2 + c warp: the picture pulled into Julia-set filaments.
    Julia,
}

/// Fractal modifier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fractal {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub mode: FractalMode,
    pub iterations: u32,
    /// KIFS scale per step (>1 shrinks the cell each level).
    pub scale: f32,
    /// KIFS rotation per step, radians; Julia: speed of c's orbit.
    pub angle: f32,
    /// KIFS shift per step; Julia: radius of c's orbit.
    pub offset: f32,
    pub zoom: f32,
    /// Weight of each deeper level relative to the one above.
    pub falloff: f32,
    pub react: f32,
}

impl Default for Fractal {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, mode: FractalMode::Kifs, iterations: 4, scale: 1.9, angle: 0.35, offset: 0.8, zoom: 1.0, falloff: 0.8, react: 1.0 }
    }
}

/// Luminescence modifier: fresh glow, hue-shifted afterglow, UV saturation, sparkle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Luminescence {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    /// Fast pool release, seconds.
    pub fresh_s: f32,
    /// Slow pool release, seconds (the afterglow).
    pub afterglow_s: f32,
    pub fresh_gain: f32,
    pub afterglow_gain: f32,
    /// Afterglow hue, degrees away from the glow colour.
    pub hue_shift: f32,
    /// Glow in the picture's own colours (so a palette carries through)
    /// instead of the layer tint.
    pub own_colour: bool,
    /// Extra excitation on onsets.
    pub excite: f32,
    /// Fluorescent saturation of the excited picture.
    pub uv: f32,
    pub sparkle: f32,
    pub react: f32,
}

impl Default for Luminescence {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, fresh_s: 0.35, afterglow_s: 4.0, fresh_gain: 0.6, afterglow_gain: 0.5, hue_shift: -140.0, own_colour: true, excite: 1.5, uv: 0.6, sparkle: 1.0, react: 1.0 }
    }
}

/// Palette grade modifier: recolour the picture through the layer's palette.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    /// false = by brightness (gradient map), true = by the picture's hue.
    pub by_hue: bool,
    pub amount: f32,
    pub react: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, by_hue: false, amount: 1.0, react: 1.0 }
    }
}

/// A layer's palette: a preset by name, or its own stops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteSel {
    /// "none" = the layer colours itself as before; a preset name; or "custom".
    pub name: String,
    pub custom: Vec<crate::color::Stop>,
    /// How far visuals lean on the palette instead of their own colouring.
    pub amount: f32,
    /// Cycles per second the palette shifts along itself (0 = still).
    pub cycle: f32,
    pub cyclic: bool,
    pub react: f32,
}

impl Default for PaletteSel {
    fn default() -> Self {
        Self { name: "none".into(), custom: crate::color::default_custom(), amount: 1.0, cycle: 0.0, cyclic: true, react: 1.0 }
    }
}

impl PaletteSel {
    pub fn active(&self) -> bool {
        self.name != "none"
    }
    /// The stops this selection means (a preset's, or the custom list).
    pub fn stops(&self) -> Vec<crate::color::Stop> {
        if self.name == "custom" {
            self.custom.clone()
        } else {
            crate::color::preset(&self.name).map(|p| p.stops.to_vec()).unwrap_or_else(|| crate::color::preset("phosphor").unwrap().stops.to_vec())
        }
    }
}

/// A modifier: one full-screen pass over a layer's picture before the mix.
/// A layer runs an ordered chain of up to `MAX_MODS`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Modifier {
    Fold(Fold),
    Feedback(Feedback),
    Polar(Polar),
    Ripple(Ripple),
    Edge(Edge),
    Chroma(ChromaSplit),
    Mask(Mask),
    Fractal(Fractal),
    Lumi(Luminescence),
    Grade(Grade),
    Halftone(Halftone),
    Warp(Warp),
}

pub const MAX_MODS: usize = 6;

impl Modifier {
    /// Every kind with a constructor, in the order the panel offers them.
    pub const KINDS: [(&'static str, fn() -> Modifier); 12] = [
        ("Palette", || Modifier::Grade(Grade::default())),
        ("Kaleidoscope", || Modifier::Fold(Fold::default())),
        ("Fractal", || Modifier::Fractal(Fractal::default())),
        ("Luminescence", || Modifier::Lumi(Luminescence::default())),
        ("Feedback", || Modifier::Feedback(Feedback::default())),
        ("Polar", || Modifier::Polar(Polar::default())),
        ("Ripple", || Modifier::Ripple(Ripple::default())),
        ("Edge", || Modifier::Edge(Edge::default())),
        ("Chroma split", || Modifier::Chroma(ChromaSplit::default())),
        ("Mask", || Modifier::Mask(Mask::default())),
        ("Halftone", || Modifier::Halftone(Halftone::default())),
        ("Warp", || Modifier::Warp(Warp::default())),
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Modifier::Fold(_) => "Kaleidoscope",
            Modifier::Feedback(_) => "Feedback",
            Modifier::Polar(_) => "Polar",
            Modifier::Ripple(_) => "Ripple",
            Modifier::Edge(_) => "Edge",
            Modifier::Chroma(_) => "Chroma split",
            Modifier::Mask(_) => "Mask",
            Modifier::Fractal(_) => "Fractal",
            Modifier::Lumi(_) => "Luminescence",
            Modifier::Grade(_) => "Palette",
            Modifier::Halftone(_) => "Halftone",
            Modifier::Warp(_) => "Warp",
        }
    }

    pub fn enabled(&self) -> bool {
        *self.enabled_ref()
    }

    pub fn mix(&self) -> f32 {
        match self {
            Modifier::Fold(k) => k.mix,
            Modifier::Feedback(k) => k.mix,
            Modifier::Polar(k) => k.mix,
            Modifier::Ripple(k) => k.mix,
            Modifier::Edge(k) => k.mix,
            Modifier::Chroma(k) => k.mix,
            Modifier::Mask(k) => k.mix,
            Modifier::Fractal(k) => k.mix,
            Modifier::Lumi(k) => k.mix,
            Modifier::Grade(k) => k.mix,
            Modifier::Halftone(k) => k.mix,
            Modifier::Warp(k) => k.mix,
        }
    }

    /// The band this modifier follows (`None` = the layer's own).
    pub fn band(&self) -> Option<AudioBand> {
        match self {
            Modifier::Fold(k) => k.band,
            Modifier::Feedback(k) => k.band,
            Modifier::Polar(k) => k.band,
            Modifier::Ripple(k) => k.band,
            Modifier::Edge(k) => k.band,
            Modifier::Chroma(k) => k.band,
            Modifier::Mask(k) => k.band,
            Modifier::Fractal(k) => k.band,
            Modifier::Lumi(k) => k.band,
            Modifier::Grade(k) => k.band,
            Modifier::Halftone(k) => k.band,
            Modifier::Warp(k) => k.band,
        }
    }

    pub fn band_mut(&mut self) -> &mut Option<AudioBand> {
        match self {
            Modifier::Fold(k) => &mut k.band,
            Modifier::Feedback(k) => &mut k.band,
            Modifier::Polar(k) => &mut k.band,
            Modifier::Ripple(k) => &mut k.band,
            Modifier::Edge(k) => &mut k.band,
            Modifier::Chroma(k) => &mut k.band,
            Modifier::Mask(k) => &mut k.band,
            Modifier::Fractal(k) => &mut k.band,
            Modifier::Lumi(k) => &mut k.band,
            Modifier::Grade(k) => &mut k.band,
            Modifier::Halftone(k) => &mut k.band,
            Modifier::Warp(k) => &mut k.band,
        }
    }

    /// Bring a loaded modifier up to date (old fields into their successors).
    pub fn normalize(&mut self) {
        if let Modifier::Edge(k) = self {
            if k.tinted {
                k.colour = EdgeColour::Tint;
                k.tinted = false;
            }
        }
    }

    pub fn mix_mut(&mut self) -> &mut f32 {
        match self {
            Modifier::Fold(k) => &mut k.mix,
            Modifier::Feedback(k) => &mut k.mix,
            Modifier::Polar(k) => &mut k.mix,
            Modifier::Ripple(k) => &mut k.mix,
            Modifier::Edge(k) => &mut k.mix,
            Modifier::Chroma(k) => &mut k.mix,
            Modifier::Mask(k) => &mut k.mix,
            Modifier::Fractal(k) => &mut k.mix,
            Modifier::Lumi(k) => &mut k.mix,
            Modifier::Grade(k) => &mut k.mix,
            Modifier::Halftone(k) => &mut k.mix,
            Modifier::Warp(k) => &mut k.mix,
        }
    }

    /// Fresh defaults of the same kind.
    pub fn reset(&mut self) {
        let enabled = self.enabled();
        *self = match self {
            Modifier::Fold(_) => Modifier::Fold(Fold::default()),
            Modifier::Feedback(_) => Modifier::Feedback(Feedback::default()),
            Modifier::Polar(_) => Modifier::Polar(Polar::default()),
            Modifier::Ripple(_) => Modifier::Ripple(Ripple::default()),
            Modifier::Edge(_) => Modifier::Edge(Edge::default()),
            Modifier::Chroma(_) => Modifier::Chroma(ChromaSplit::default()),
            Modifier::Mask(_) => Modifier::Mask(Mask::default()),
            Modifier::Fractal(_) => Modifier::Fractal(Fractal::default()),
            Modifier::Lumi(_) => Modifier::Lumi(Luminescence::default()),
            Modifier::Grade(_) => Modifier::Grade(Grade::default()),
            Modifier::Halftone(_) => Modifier::Halftone(Halftone::default()),
            Modifier::Warp(_) => Modifier::Warp(Warp::default()),
        };
        *self.enabled_mut() = enabled;
    }

    fn enabled_ref(&self) -> &bool {
        match self {
            Modifier::Fold(m) => &m.enabled,
            Modifier::Feedback(m) => &m.enabled,
            Modifier::Polar(m) => &m.enabled,
            Modifier::Ripple(m) => &m.enabled,
            Modifier::Edge(m) => &m.enabled,
            Modifier::Chroma(m) => &m.enabled,
            Modifier::Mask(m) => &m.enabled,
            Modifier::Fractal(m) => &m.enabled,
            Modifier::Lumi(m) => &m.enabled,
            Modifier::Grade(m) => &m.enabled,
            Modifier::Halftone(m) => &m.enabled,
            Modifier::Warp(m) => &m.enabled,
        }
    }

    pub fn enabled_mut(&mut self) -> &mut bool {
        match self {
            Modifier::Fold(m) => &mut m.enabled,
            Modifier::Feedback(m) => &mut m.enabled,
            Modifier::Polar(m) => &mut m.enabled,
            Modifier::Ripple(m) => &mut m.enabled,
            Modifier::Edge(m) => &mut m.enabled,
            Modifier::Chroma(m) => &mut m.enabled,
            Modifier::Mask(m) => &mut m.enabled,
            Modifier::Fractal(m) => &mut m.enabled,
            Modifier::Lumi(m) => &mut m.enabled,
            Modifier::Grade(m) => &mut m.enabled,
            Modifier::Halftone(m) => &mut m.enabled,
            Modifier::Warp(m) => &mut m.enabled,
        }
    }
}

/// Mark shape of the halftone screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HalftoneShape {
    #[default]
    Dot,
    Line,
    Cross,
}

impl HalftoneShape {
    pub const ALL: [HalftoneShape; 3] = [HalftoneShape::Dot, HalftoneShape::Line, HalftoneShape::Cross];
    pub fn name(self) -> &'static str {
        match self {
            HalftoneShape::Dot => "Dots",
            HalftoneShape::Line => "Lines",
            HalftoneShape::Cross => "Crosses",
        }
    }
}

/// Halftone modifier: the picture re-printed as a screen of marks on a
/// rotated grid, in its own colours or one ink, drifting between the two.
/// Inspired by the halftone pass in Felix Turner's Uberviz.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Halftone {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    pub shape: HalftoneShape,
    /// Screen cell size, pixels.
    pub cell: f32,
    /// Screen angle, degrees.
    pub angle: f32,
    /// 0 = the picture's colours, 1 = one ink (the tint, or the palette by brightness).
    pub mono: f32,
    /// How far `mono` wanders on its own and with the music.
    pub drift: f32,
    /// Brightness curve into the mark size.
    pub gamma: f32,
    /// Use the layer palette as the ink.
    pub palette: bool,
    pub invert: bool,
    /// Cells breathe with the bass; onsets flash toward one ink.
    pub react: f32,
}

impl Default for Halftone {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, shape: HalftoneShape::Dot, cell: 9.0, angle: 15.0, mono: 0.5, drift: 0.6, gamma: 1.4, palette: true, invert: false, react: 1.0 }
    }
}

/// Warp modifier: the picture pulled through a flowing field of smooth noise.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Warp {
    pub enabled: bool,
    /// Wet/dry: 1 = the pass's full effect, 0 = the picture untouched.
    pub mix: f32,
    /// Audio band this effect follows; none = the layer's own.
    pub band: Option<AudioBand>,
    /// Displacement at rest, pixels.
    pub amount: f32,
    /// Field detail: noise cycles across the screen.
    pub scale: f32,
    /// How fast the field flows.
    pub speed: f32,
    /// How much the field turns on itself.
    pub twist: f32,
    /// Noise octaves (1 = soft swells, 4 = churn).
    pub octaves: u32,
    /// Bass swells the displacement; onsets kick the flow.
    pub react: f32,
}

impl Default for Warp {
    fn default() -> Self {
        Self { enabled: true, mix: 1.0, band: None, amount: 24.0, scale: 3.0, speed: 0.25, twist: 0.5, octaves: 2, react: 1.0 }
    }
}

/// Where the kaleidoscope's object cell takes its picture from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageSource {
    Art,
    Photos,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KaleidoParams {
    pub source: ImageSource,
    /// Photos: seconds each picture stays before the crossfade.
    pub dwell_s: f32,
    /// Fraction of the picture's height the window shows.
    pub zoom: f32,
    pub drift: f32,
    /// Spin kick on onsets.
    pub tumble: f32,
    /// 0 = the picture's own colours, 1 = fully pulled to the layer tint.
    pub tint_mix: f32,
}

impl Default for KaleidoParams {
    fn default() -> Self {
        Self { source: ImageSource::Art, dwell_s: 15.0, zoom: 0.6, drift: 1.0, tumble: 1.0, tint_mix: 0.0 }
    }
}

/// What picks the scope beam's palette position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScopeColour {
    /// The track's key sets the colour; fast beam travel moves along the palette.
    #[default]
    Key,
    /// Each part of the trace takes the colour of its own pitch (zero-crossing estimate).
    Frequency,
}

impl ScopeColour {
    pub const ALL: [ScopeColour; 2] = [ScopeColour::Key, ScopeColour::Frequency];
    pub fn name(self) -> &'static str {
        match self {
            ScopeColour::Key => "Key and speed",
            ScopeColour::Frequency => "Frequency",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeParams {
    /// With a palette: what picks the beam's colour.
    pub colour: ScopeColour,
    pub gain: f32,
    pub auto_gain: bool,
    pub rotation_deg: f32,
    pub beam_width: f32,
    pub beam_intensity: f32,
    /// Fraction of brightness remaining after one second.
    pub persistence: f32,
    /// How much the beat clock pulses the beam (0 = off).
    pub beat_pulse: f32,
}

impl Default for ScopeParams {
    fn default() -> Self {
        Self { colour: ScopeColour::Key, gain: 0.85, auto_gain: true, rotation_deg: 45.0, beam_width: 1.2, beam_intensity: 1.0, persistence: 0.05, beat_pulse: 0.3 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RibbonsParams {
    pub speed: f32,
    pub scale: f32,
    pub curl: f32,
    pub vortex: f32,
    pub width: f32,
    pub intensity: f32,
    pub persistence: f32,
    pub saturation: f32,
    pub count: u32,
    /// With a palette: how far a chord spreads around the current position
    /// (0 = one colour at a time, 1 = the whole palette at once).
    pub spread: f32,
}

impl Default for RibbonsParams {
    fn default() -> Self {
        Self { speed: 0.3, scale: 1.5, curl: 1.0, vortex: 1.0, width: 1.0, intensity: 1.0, persistence: 0.3, saturation: 0.45, count: 12_000, spread: 0.3 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StarfieldParams {
    /// Lens mass: scales the shadow and the disk together.
    pub mass: f32,
    pub density: f32,
    pub twinkle: f32,
    pub drift: f32,
    /// Accretion disk brightness.
    pub disk: f32,
    /// Doppler beaming strength (approaching side brighter and bluer).
    pub doppler: f32,
    /// Camera elevation above the disk plane, degrees.
    pub tilt: f32,
    /// How much the hole's size breathes with the bass (0 = steady).
    pub breathe: f32,
    /// Nebula haze and the Milky Way band behind the stars.
    pub nebula: f32,
    /// Infall: tendrils at the inner edge and streams spiralling in from outside.
    pub streams: f32,
}

impl Default for StarfieldParams {
    fn default() -> Self {
        Self { mass: 1.0, density: 1.0, twinkle: 0.6, drift: 1.0, disk: 1.0, doppler: 1.0, tilt: 5.0, breathe: 0.0, nebula: 1.0, streams: 1.0 }
    }
}

/// Sigil: a neon sacred-geometry glyph (`scenes/sigil.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SigilParams {
    /// Glyph size as a fraction of the short side.
    pub scale: f32,
    /// Coloured halo around every tube.
    pub glow: f32,
    pub rings: f32,
    pub rays: f32,
    pub orbs: f32,
    /// The centre star polygon, cardinal triangles and diamonds.
    pub shapes: f32,
    /// The sigil trace and its pen.
    pub trace: f32,
    /// Paint drips from bright elements.
    pub drips: f32,
    /// The innermost ring and the centre flare.
    pub core: f32,
    /// The painterly ground and its splatter (0 = none; the glyph stands alone).
    pub ground: f32,
    /// Ray afterimage life, seconds.
    pub persistence: f32,
    /// Rotation of the glyph and the orbit.
    pub spin: f32,
    /// How much bands, onsets and beats move it.
    pub react: f32,
    /// Extinguish and relight a new arrangement on a track change.
    pub rebuild: bool,
    /// Radial spacing of everything outside the inner ring (1 = as drawn).
    pub spread: f32,
    /// How far drops push the spread outward.
    pub explode: f32,
    /// Tube width multiplier.
    pub line_width: f32,
    /// 0 = lines in their own colour, 1 = white-hot cores.
    pub hot: f32,
    /// Overrides of the track seed; 0 = by track.
    pub sides: u32,
    pub nest: u32,
    pub symmetry: u32,
}

impl Default for SigilParams {
    fn default() -> Self {
        Self {
            scale: 0.8,
            glow: 1.0,
            rings: 1.0,
            rays: 0.7,
            orbs: 1.0,
            shapes: 1.0,
            trace: 1.0,
            drips: 1.0,
            core: 1.0,
            ground: 0.0,
            persistence: 0.35,
            spin: 1.0,
            react: 1.0,
            rebuild: true,
            spread: 1.0,
            explode: 0.5,
            line_width: 1.0,
            hot: 0.3,
            sides: 0,
            nest: 0,
            symmetry: 0,
        }
    }
}

/// What a Waves row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WavesSource {
    #[default]
    Waveform,
    Spectrum,
}

/// What picks a Waves row's palette position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WavesColour {
    /// Front to back through the palette.
    #[default]
    Age,
    /// The row's loudness.
    Level,
}

/// Waves: a landscape of signal rows in perspective (`scenes/waves.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WavesParams {
    pub source: WavesSource,
    pub colour: WavesColour,
    /// Rows on screen (the depth of the landscape).
    pub rows: u32,
    /// New rows per second.
    pub rate: f32,
    /// Amplitude of the near row, in screen halves.
    pub height: f32,
    /// Half width of the near row, in screen halves.
    pub spread: f32,
    /// 0 = a flat stack, 1 = strong foreshortening toward the horizon.
    pub perspective: f32,
    /// Baseline of the farthest row, 0 bottom .. 1 top.
    pub horizon: f32,
    /// Baseline of the nearest row.
    pub near: f32,
    /// Line width, pixels.
    pub width: f32,
    pub gain: f32,
    /// Box-smoothing passes over each row.
    pub smooth: u32,
    /// How dim the far rows get.
    pub fade: f32,
}

impl Default for WavesParams {
    fn default() -> Self {
        Self { source: WavesSource::Waveform, colour: WavesColour::Age, rows: 48, rate: 24.0, height: 0.35, spread: 0.7, perspective: 0.6, horizon: 0.72, near: 0.2, width: 1.5, gain: 1.0, smooth: 2, fade: 0.7 }
    }
}

/// Crystal: tumbling glass shards (`scenes/crystal.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrystalParams {
    pub count: u32,
    pub size: f32,
    /// Shards are stretched along their own x.
    pub stretch: f32,
    /// Tumble speed of the cluster.
    pub spin: f32,
    /// How far the shards fly apart over each bar (0 = a tight cluster).
    pub scatter: f32,
    /// A shard takes a new pose every this many beats.
    pub every: u32,
    /// Facet opacity gain.
    pub facets: f32,
    /// Edge brightness gain.
    pub edges: f32,
    /// Edge line width, pixels.
    pub width: f32,
    pub react: f32,
}

impl Default for CrystalParams {
    fn default() -> Self {
        Self { count: 5, size: 1.0, stretch: 2.0, spin: 1.0, scatter: 0.0, every: 1, facets: 1.0, edges: 1.0, width: 1.5, react: 1.0 }
    }
}

/// Where a layer takes its colour from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TintMode {
    /// The scene tint (which itself may follow the album art).
    Scene,
    /// The album art's dominant colour, whatever the scene tint does.
    Art,
    /// This layer's own colour.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayerTint {
    pub mode: TintMode,
    pub color: [f32; 3],
}

impl Default for LayerTint {
    fn default() -> Self {
        Self { mode: TintMode::Scene, color: [0.40, 1.0, 0.55] }
    }
}

/// Layer blend mode. Add is order-free; the others let a higher layer
/// cover, lighten or mask what is beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Add,
    /// Bright parts cover what is below (brightness is the coverage).
    Over,
    /// Lightens what is below without blowing out.
    Screen,
    /// Masks what is below: dark parts of this layer darken it.
    Multiply,
}

impl Blend {
    pub const ALL: [Blend; 4] = [Blend::Add, Blend::Over, Blend::Screen, Blend::Multiply];
    pub fn name(self) -> &'static str {
        match self {
            Blend::Add => "Add",
            Blend::Over => "Over",
            Blend::Screen => "Screen",
            Blend::Multiply => "Multiply",
        }
    }
}

/// Which part of the spectrum a layer reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioBand {
    /// false = the full mix (what every layer did before bands existed).
    pub enabled: bool,
    pub lo_hz: f32,
    pub hi_hz: f32,
}

impl Default for AudioBand {
    fn default() -> Self {
        Self { enabled: false, lo_hz: 20.0, hi_hz: 400.0 }
    }
}

impl AudioBand {
    /// Named presets: (label, lo, hi).
    pub const PRESETS: [(&'static str, f32, f32); 6] = [("Sub", 20.0, 60.0), ("Bass", 60.0, 250.0), ("Low mid", 250.0, 500.0), ("Mid", 500.0, 2000.0), ("Presence", 2000.0, 4000.0), ("Air", 4000.0, 16000.0)];
    pub fn label(&self) -> String {
        if !self.enabled {
            return "Full mix".into();
        }
        for (name, lo, hi) in Self::PRESETS {
            if (self.lo_hz - lo).abs() < 1.0 && (self.hi_hz - hi).abs() < 1.0 {
                return name.into();
            }
        }
        format!("{:.0}-{:.0} Hz", self.lo_hz, self.hi_hz)
    }
}

/// One instance of a visual in the scene. All three parameter sets are kept
/// so switching the visual back and forth loses nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub visual: VisualKind,
    pub enabled: bool,
    /// 0 = off, 1 = as rendered, up to 2 = boosted.
    pub opacity: f32,
    /// How this layer meets the ones below it (layer 1 is on top).
    pub blend: Blend,
    pub band: AudioBand,
    pub tint: LayerTint,
    /// Colour palette the visual and the palette modifier draw from.
    pub palette: PaletteSel,
    /// Ordered chain of modifiers run over the visual's output.
    pub modifiers: Vec<Modifier>,
    /// Pre-chain field, migrated into `modifiers` on load; never written.
    #[serde(skip_serializing)]
    pub fold: LegacyFold,
    pub scope: ScopeParams,
    pub ribbons: RibbonsParams,
    pub starfield: StarfieldParams,
    pub kaleido: KaleidoParams,
    pub sigil: SigilParams,
    pub waves: WavesParams,
    pub crystal: CrystalParams,
}

impl Default for Layer {
    fn default() -> Self {
        Self { visual: VisualKind::Scope, enabled: false, opacity: 1.0, blend: Blend::Add, band: AudioBand::default(), tint: LayerTint::default(), palette: PaletteSel::default(), modifiers: Vec::new(), fold: LegacyFold::default(), scope: ScopeParams::default(), ribbons: RibbonsParams::default(), starfield: StarfieldParams::default(), kaleido: KaleidoParams::default(), sigil: SigilParams::default(), waves: WavesParams::default(), crystal: CrystalParams::default() }
    }
}

impl Layer {
    pub fn new(visual: VisualKind, enabled: bool, opacity: f32) -> Self {
        Self { visual, enabled, opacity, ..Self::default() }
    }

    /// Bring a loaded layer up to date: the one-build `fold` field becomes the
    /// first modifier; chains are capped.
    pub fn normalize(&mut self) {
        if self.fold.enabled {
            let f = self.fold;
            self.modifiers.insert(0, Modifier::Fold(Fold { enabled: true, mode: f.mode, segments: f.segments, speed: f.speed, zoom: f.zoom, react: f.react, ..Fold::default() }));
            self.fold.enabled = false;
        }
        self.modifiers.truncate(MAX_MODS);
        for m in &mut self.modifiers {
            m.normalize();
        }
    }

    pub fn has_fold(&self) -> bool {
        self.modifiers.iter().any(|m| matches!(m, Modifier::Fold(_)))
    }
}

/// Post-stack settings for the whole picture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostSettings {
    pub exposure: f32,
    pub auto_exposure: bool,
    pub bloom: f32,
    pub grain: f32,
    /// Tonemapper: film (AgX, bright colours roll to white) or neon (hue kept,
    /// only brightness compresses, so saturated colours stay saturated).
    pub look: Look,
    /// Colour saturation before the tonemapper (1 = as rendered).
    pub saturation: f32,
}

impl Default for PostSettings {
    fn default() -> Self {
        Self { exposure: 1.0, auto_exposure: true, bloom: 0.3, grain: 0.15, look: Look::Film, saturation: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    #[default]
    Film,
    Neon,
}

/// Panel sections, in rail order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Section {
    Scenes,
    Layers,
    /// The whole picture: post stack and scene tint (was "Post").
    #[serde(alias = "Post")]
    Picture,
    Sync,
    Display,
    Meters,
    About,
}

impl Section {
    pub const ALL: [Section; 7] = [Section::Scenes, Section::Layers, Section::Picture, Section::Sync, Section::Display, Section::Meters, Section::About];
    pub fn name(self) -> &'static str {
        match self {
            Section::Scenes => "Scenes",
            Section::Layers => "Layers",
            Section::Picture => "Picture",
            Section::Sync => "Sync",
            Section::Display => "Display",
            Section::Meters => "Meters",
            Section::About => "About",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // ---- the scene (saved by scene_store) ----
    pub layers: Vec<Layer>,
    /// Which layer the panel and keys 1-3 edit.
    pub editing: usize,
    /// Solo: render only this layer while tuning it (never saved).
    #[serde(skip)]
    pub solo: Option<usize>,
    /// Name of the active scene ("" = unsaved).
    pub scene_name: String,
    pub post: PostSettings,
    /// Base colour for every visual.
    pub tint: [f32; 3],
    /// Scene accent follows the album art's dominant colour.
    pub tint_from_art: bool,
    /// Show what is playing: toast and track identity on the strip.
    pub show_now_playing: bool,
    /// Folder of photos for the kaleidoscope's "my photos" picture source.
    pub photos_folder: String,
    // ---- director ----
    pub auto_scenes: bool,
    // ---- accessibility ----
    pub reduce_flashing: bool,
    pub flash_rise: f32,
    // ---- audio ----
    pub audio_delay_ms: f32,
    /// What the visualizer listens to: a player on this PC or the room.
    pub source: AudioSourceKind,
    pub target_process: String,
    // ---- display ----
    pub fps_cap: u32,
    pub monitor: usize,
    pub start_fullscreen: bool,
    pub low_power: bool,
    pub toast: bool,
    pub chrome_autohide: bool,
    // ---- panel ----
    pub show_ui: bool,
    pub section: Section,
    /// Which tab of the layer inspector is open.
    pub layer_tab: LayerTab,
    /// Open state of collapsible control groups, by key ("sigil.look").
    pub ui_open: std::collections::BTreeMap<String, bool>,
    pub developer: bool,
    pub onboarded: bool,
}

/// What the visualizer listens to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioSourceKind {
    /// A player on this PC (`target_process`), by loopback.
    #[default]
    Player,
    /// The room, through the default microphone.
    Microphone,
}

impl AudioSourceKind {
    pub const ALL: [AudioSourceKind; 2] = [AudioSourceKind::Player, AudioSourceKind::Microphone];
    pub fn name(self) -> &'static str {
        match self {
            AudioSourceKind::Player => "A player on this PC",
            AudioSourceKind::Microphone => "The room",
        }
    }
}

/// Tabs of the layer inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayerTab {
    #[default]
    Visual,
    Colour,
    Effects,
}

impl LayerTab {
    pub const ALL: [LayerTab; 3] = [LayerTab::Visual, LayerTab::Colour, LayerTab::Effects];
    pub fn name(self) -> &'static str {
        match self {
            LayerTab::Visual => "Visual",
            LayerTab::Colour => "Colour",
            LayerTab::Effects => "Effects",
        }
    }
}

impl Settings {
    pub fn layer(&self, i: usize) -> Layer {
        self.layers.get(i).cloned().unwrap_or_default()
    }
    pub fn layer_mut(&mut self, i: usize) -> &mut Layer {
        while self.layers.len() <= i {
            self.layers.push(Layer::default());
        }
        &mut self.layers[i]
    }
    /// Post-load fix-ups for every layer (see `Layer::normalize`).
    pub fn normalize(&mut self) {
        for l in &mut self.layers {
            l.normalize();
        }
    }
    pub fn reset_post(&mut self) {
        self.post = PostSettings::default();
    }

    /// Take the scene part of `other` (layers, post, tint, overlay) and keep
    /// everything about this machine and window.
    pub fn apply_scene(&mut self, other: &Settings) {
        self.layers = other.layers.clone();
        self.editing = other.editing.min(MAX_LAYERS - 1);
        self.post = other.post;
        self.tint = other.tint;
        self.tint_from_art = other.tint_from_art;
        self.scene_name = other.scene_name.clone();
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            layers: vec![Layer::new(VisualKind::Scope, true, 1.0), Layer::new(VisualKind::Ribbons, false, 0.5), Layer::new(VisualKind::Starfield, false, 0.5)],
            solo: None,
            editing: 0,
            scene_name: String::new(),
            post: PostSettings::default(),
            tint: [0.40, 1.0, 0.55],
            tint_from_art: false,
            show_now_playing: false,
            photos_folder: String::new(),
            auto_scenes: false,
            reduce_flashing: true,
            flash_rise: 2.0,
            audio_delay_ms: 0.0,
            source: AudioSourceKind::Player,
            target_process: "Spotify.exe".into(),
            fps_cap: 60,
            monitor: 0,
            start_fullscreen: false,
            low_power: true,
            toast: true,
            chrome_autohide: true,
            show_ui: false,
            section: Section::Scenes,
            layer_tab: LayerTab::default(),
            ui_open: Default::default(),
            developer: false,
            onboarded: false,
        }
    }
}
