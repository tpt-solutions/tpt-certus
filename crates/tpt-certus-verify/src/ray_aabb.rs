//! The `ray_intersects_aabb` slab method as a straight-line [`Graph`].
//!
//! This is a port of `tpt_certus_spatial::ray_aabb::ray_intersects_aabb`.  Each
//! `if` in the artifact becomes a [`Op::Select`](crate::ir::Op::Select), so the
//! graph has no control flow.  Operations are performed in the same order as
//! the artifact, so the `f64` evaluation is expected to agree bit-for-bit.
//! `tests/ir_parity.rs` checks that over a grid that includes zero direction
//! components.
//!
//! Inputs, in order: origin `x,y,z`; direction `x,y,z`; box min `x,y,z`;
//! box max `x,y,z`; `t_max`.
//!
//! Outputs: `hit` (1.0 for a hit, 0.0 for a miss), `t_near`, `t_far`.  The
//! `t_near`/`t_far` values are only meaningful when `hit == 1.0`, matching the
//! artifact's `Option<Intersection>`.

use alloc::format;

use crate::ir::{Cmp, Cond, Graph, NodeId};
use crate::rational::Rational;

/// The graph together with its named outputs.
pub struct RayAabbGraph {
    /// The straight-line graph.
    pub graph: Graph,
    /// 1.0 for a hit, 0.0 for a miss.
    pub hit: NodeId,
    /// Entry distance, `max(t_min, 0)`.  Meaningful only on a hit.
    pub t_near: NodeId,
    /// Unclipped exit distance, `t_max_slab`.  Meaningful only on a hit.
    pub t_far: NodeId,
}

/// Build the graph.
pub fn ray_aabb_graph() -> RayAabbGraph {
    let mut g = Graph::new();

    let o = [g.input("origin.x"), g.input("origin.y"), g.input("origin.z")];
    let d = [g.input("dir.x"), g.input("dir.y"), g.input("dir.z")];
    let lo = [g.input("min.x"), g.input("min.y"), g.input("min.z")];
    let hi = [g.input("max.x"), g.input("max.y"), g.input("max.z")];
    let t_max = g.input("t_max");

    let zero = g.constant("0", Rational::ZERO);
    let one = g.constant("1", Rational::ONE);
    let pos_inf = g.infinity("+inf", true);
    let neg_inf = g.infinity("-inf", false);

    const AXES: [&str; 3] = ["x", "y", "z"];
    let mut t_min = neg_inf;
    let mut t_max_slab = pos_inf;
    let mut miss = zero;

    for a in 0..3 {
        let ax = AXES[a];
        let dir_nonzero = Cond {
            op: Cmp::Ne,
            a: d[a],
            b: zero,
        };
        let dir_zero = Cond {
            op: Cmp::Eq,
            a: d[a],
            b: zero,
        };

        // tx1 = (min − origin) / dir, tx2 = (max − origin) / dir.
        let num1 = g.sub(&format!("min.{ax} - origin.{ax}"), lo[a], o[a]);
        let num2 = g.sub(&format!("max.{ax} - origin.{ax}"), hi[a], o[a]);
        let t1 = g.div(&format!("t1.{ax}"), num1, d[a]);
        let t2 = g.div(&format!("t2.{ax}"), num2, d[a]);
        let near_a = g.min(&format!("near.{ax}"), t1, t2);
        let far_a = g.max(&format!("far.{ax}"), t1, t2);

        // A zero direction component contributes no slab constraint.
        let near_gated = g.select(&format!("near.{ax} gated"), dir_nonzero, near_a, neg_inf);
        let far_gated = g.select(&format!("far.{ax} gated"), dir_nonzero, far_a, pos_inf);
        t_min = g.max(&format!("t_min after {ax}"), t_min, near_gated);
        t_max_slab = g.min(&format!("t_max_slab after {ax}"), t_max_slab, far_gated);

        // Parallel ray outside the slab ⇒ miss.  Flag is 1.0 or 0.0.
        let below = Cond {
            op: Cmp::Lt,
            a: o[a],
            b: lo[a],
        };
        let above = Cond {
            op: Cmp::Lt,
            a: hi[a],
            b: o[a],
        };
        let outside = g.select(&format!("outside {ax}"), above, one, zero);
        let outside_or_below = g.select(&format!("outside-or-below {ax}"), below, one, outside);
        let miss_axis = g.select(
            &format!("miss {ax}"),
            dir_zero,
            outside_or_below,
            zero,
        );
        miss = g.max(&format!("miss after {ax}"), miss, miss_axis);
    }

    // Artifact: if t_max_slab >= t_min && t_max_slab >= 0, then
    //   t_near = max(t_min, 0); hit iff t_near <= t_max.
    let t_near = g.max("t_near", t_min, zero);
    let within_tmax = g.select(
        "t_near <= t_max",
        Cond {
            op: Cmp::Le,
            a: t_near,
            b: t_max,
        },
        one,
        zero,
    );
    let exit_nonneg = g.select(
        "t_max_slab >= 0",
        Cond {
            op: Cmp::Lt,
            a: t_max_slab,
            b: zero,
        },
        zero,
        within_tmax,
    );
    let slab_ordered = g.select(
        "t_max_slab >= t_min",
        Cond {
            op: Cmp::Lt,
            a: t_max_slab,
            b: t_min,
        },
        zero,
        exit_nonneg,
    );
    let hit = g.select(
        "missed",
        Cond {
            op: Cmp::Eq,
            a: miss,
            b: one,
        },
        zero,
        slab_ordered,
    );

    RayAabbGraph {
        graph: g,
        hit,
        t_near,
        t_far: t_max_slab,
    }
}
