// skp-uv/src/units.rs
// Single source of truth for every scale factor and axis convention in the pipeline.

use std::ops::{Add, Mul, Sub};

/// SketchUp stores all lengths internally in inches; an Unreal Unit is one centimetre.
pub const INCHES_TO_UU: f64 = 2.54;

/// Area scales with the square of the linear factor. Named so it is never re-derived by hand.
pub const SQ_INCHES_TO_SQ_UU: f64 = INCHES_TO_UU * INCHES_TO_UU;

/// SketchUp is right-handed Z-up, Unreal is left-handed Z-up. Negating Y converts, and flips winding.
pub const UNREAL_NEGATES_Y: bool = true;

// ---------- length ----------

/// A length as returned by the SketchUp C API.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Inches(pub f64);

/// A length in Unreal Units (centimetres). All geometry downstream of skp-io is in these.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Uu(pub f64);

impl From<Inches> for Uu {
    fn from(v: Inches) -> Self {
        Uu(v.0 * INCHES_TO_UU)
    }
}

impl From<Uu> for Inches {
    fn from(v: Uu) -> Self {
        Inches(v.0 / INCHES_TO_UU)
    }
}

impl Uu {
    /// Narrows to f32 at the export boundary only; keep f64 for all internal maths.
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

impl Add for Uu {
    type Output = Uu;
    fn add(self, r: Uu) -> Uu {
        Uu(self.0 + r.0)
    }
}

impl Sub for Uu {
    type Output = Uu;
    fn sub(self, r: Uu) -> Uu {
        Uu(self.0 - r.0)
    }
}

impl Mul<f64> for Uu {
    type Output = Uu;
    fn mul(self, k: f64) -> Uu {
        Uu(self.0 * k)
    }
}

// ---------- area ----------

/// A surface area in square inches, as reported by the SketchUp C API.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct SqInches(pub f64);

/// A surface area in square Unreal Units. Deliberately distinct from Uu so the linear factor cannot be applied.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct SqUu(pub f64);

impl From<SqInches> for SqUu {
    fn from(v: SqInches) -> Self {
        SqUu(v.0 * SQ_INCHES_TO_SQ_UU)
    }
}

impl Add for SqUu {
    type Output = SqUu;
    fn add(self, r: SqUu) -> SqUu {
        SqUu(self.0 + r.0)
    }
}

impl std::iter::Sum for SqUu {
    fn sum<I: Iterator<Item = SqUu>>(iter: I) -> SqUu {
        iter.fold(SqUu(0.0), |a, b| a + b)
    }
}

// ---------- texel density ----------

/// Texels per Unreal Unit, i.e. per centimetre. Linear, not areal.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TexelDensity(f64);

impl TexelDensity {
    /// 1024 px per metre. The common Unreal convention for props and hero assets.
    pub const UE_PROP: Self = TexelDensity(10.24);

    /// 512 px per metre. Suits large architectural surfaces, which is most SketchUp input.
    pub const UE_ARCHITECTURE: Self = TexelDensity(5.12);

    /// 128 px per metre. Terrain and distant background geometry.
    pub const UE_BACKGROUND: Self = TexelDensity(1.28);

    pub fn from_px_per_metre(px: f64) -> Self {
        TexelDensity(px / 100.0)
    }

    pub fn from_px_per_uu(px: f64) -> Self {
        TexelDensity(px)
    }

    /// Feeds straight into xatlas PackOptions::texelsPerUnit, which expects mesh-space units.
    pub fn per_uu(self) -> f64 {
        self.0
    }

    /// Texel count over an area is density squared. Encoded here so callers cannot forget the square.
    pub fn texels_for_area(self, area: SqUu) -> f64 {
        self.0 * self.0 * area.0
    }

    /// Inverse of texels_for_area: the density that fits a texel budget over an area.
    pub fn for_budget(texels: f64, area: SqUu) -> Option<Self> {
        if area.0 <= 0.0 || texels <= 0.0 {
            return None;
        }
        Some(TexelDensity((texels / area.0).sqrt()))
    }
}

// ---------- uv conventions ----------

/// SketchUp and OpenGL put the V origin bottom-left; Unreal and DirectX put it top-left.
pub const UNREAL_FLIPS_V: bool = true;

/// Apply exactly once, and only on export paths that do not already flip (raw writers, not FBX or glTF).
pub fn flip_v(v: f64) -> f64 {
    1.0 - v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inch_uu_roundtrip() {
        let a = Inches(96.0);
        let back: Inches = Uu::from(a).into();
        assert!((back.0 - a.0).abs() < 1e-12);
    }

    #[test]
    fn one_foot_is_thirty_point_four_eight_uu() {
        assert!((Uu::from(Inches(12.0)).0 - 30.48).abs() < 1e-12);
    }

    #[test]
    fn area_factor_is_not_the_linear_factor() {
        let a = SqUu::from(SqInches(1.0));
        assert!((a.0 - 6.4516).abs() < 1e-12);
        assert!((a.0 - INCHES_TO_UU).abs() > 1.0);
    }

    #[test]
    fn texel_count_scales_quadratically() {
        let d = TexelDensity::from_px_per_uu(2.0);
        assert!((d.texels_for_area(SqUu(100.0)) - 400.0).abs() < 1e-12);
    }

    #[test]
    fn budget_inverts_texel_count() {
        let area = SqUu(2500.0);
        let d = TexelDensity::for_budget(10_000.0, area).unwrap();
        assert!((d.texels_for_area(area) - 10_000.0).abs() < 1e-9);
    }

    #[test]
    fn budget_rejects_degenerate_input() {
        assert!(TexelDensity::for_budget(1024.0, SqUu(0.0)).is_none());
        assert!(TexelDensity::for_budget(0.0, SqUu(10.0)).is_none());
    }
}
