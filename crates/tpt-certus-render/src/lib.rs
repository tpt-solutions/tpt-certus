//! Minimal, headless WebGPU visualizer — a deterministic pure function of
//! verified spatial state.
//!
//! [`tpt-certus-render`] provides a minimal rendering pipeline that is
//! **a pure function of the verified spatial state**: given identical input
//! state, it produces identical pixel output on any platform (deterministic
//! rasterization).  The render pipeline is not itself formally verified — it
//! is *derived from* verified state, and its sole verification claim is that
//! it does not mutate or fabricate spatial data.
//!
//! # Two-layer role
//!
//! The render crate does not carry its own `tpt-telos` contracts.  Its
//! verification scope is limited to proving (at the architectural level, not
//! per-pixel) that the rendering function is referentially transparent over
//! [`tpt-certus-spatial`] state.
//!
//! # Status
//!
//! Phase 0 scaffold only — wgpu integration and the headless renderer will
//! be added when rendering is implemented.

#![no_std]

/// A minimal colour representation for the headless visualizer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub const TRANSPARENT: Self = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };
    pub const WHITE: Self = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        debug_assert!(
            (0.0..=1.0).contains(&r)
                && (0.0..=1.0).contains(&g)
                && (0.0..=1.0).contains(&b)
                && (0.0..=1.0).contains(&a),
            "RGBA components must be in [0.0, 1.0]"
        );
        Rgba { r, g, b, a }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_constants() {
        let t = Rgba::TRANSPARENT;
        assert_eq!(t.a, 0.0);
        let w = Rgba::WHITE;
        assert_eq!(w.r, 1.0);
    }
}
