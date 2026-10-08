//! Parity: the straight-line IR of `ray_intersects_aabb` must agree with the
//! shipped artifact.  Hit/miss must match on every input, and on a hit the
//! `t_near` and `t_far` values must be bit-identical `f64`s.
//!
//! The grid deliberately includes zero direction components (both in and out
//! of the slab), origins on box faces, and a degenerate box, since those are
//! the branches the port has to reproduce.

use tpt_certus_geometry::{Ray, Vector3, AABB};
use tpt_certus_spatial::ray_aabb::ray_intersects_aabb;
use tpt_certus_verify::ray_aabb::ray_aabb_graph;

const ORIGINS: [f64; 7] = [-3.0, -1.0, -0.5, 0.0, 0.5, 1.0, 3.0];
const DIRS: [f64; 6] = [-1.0, -0.5, 0.0, 0.25, 1.0, 2.0];
const T_MAXES: [f64; 3] = [0.5, 5.0, 100.0];

/// (min, max) per box: a unit box, an offset box, and a degenerate slab.
const BOXES: [([f64; 3], [f64; 3]); 3] = [
    ([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]),
    ([0.0, -2.0, 0.5], [3.0, 0.0, 2.0]),
    ([-1.0, 0.0, -1.0], [1.0, 0.0, 1.0]),
];

#[test]
fn graph_matches_the_artifact_bit_for_bit() {
    let g = ray_aabb_graph();
    let mut hits = 0usize;
    let mut misses = 0usize;
    let mut checked = 0usize;

    for (bmin, bmax) in BOXES {
        let aabb = AABB::new(
            Vector3::new(bmin[0], bmin[1], bmin[2]),
            Vector3::new(bmax[0], bmax[1], bmax[2]),
        );
        for ox in ORIGINS {
            for oy in ORIGINS {
                for oz in ORIGINS {
                    for dx in DIRS {
                        for dy in DIRS {
                            for dz in DIRS {
                                for t_max in T_MAXES {
                                    let inputs = [
                                        ox, oy, oz, dx, dy, dz, bmin[0], bmin[1], bmin[2], bmax[0],
                                        bmax[1], bmax[2], t_max,
                                    ];
                                    let values = g
                                        .graph
                                        .eval_f64(&inputs)
                                        .expect("inputs match the declared arity");
                                    let ray = Ray::new(
                                        Vector3::new(ox, oy, oz),
                                        Vector3::new(dx, dy, dz),
                                    );
                                    let reference = ray_intersects_aabb(ray, aabb, t_max);
                                    checked += 1;

                                    let ir_hit = values[g.hit] == 1.0;
                                    assert_eq!(
                                        ir_hit,
                                        reference.is_some(),
                                        "hit mismatch at origin ({ox},{oy},{oz}) dir \
                                         ({dx},{dy},{dz}) box {bmin:?}..{bmax:?} t_max {t_max}"
                                    );

                                    match reference {
                                        Some(hit) => {
                                            hits += 1;
                                            assert_eq!(
                                                values[g.t_near].to_bits(),
                                                hit.t_near.to_bits(),
                                                "t_near differs at origin ({ox},{oy},{oz}) dir \
                                                 ({dx},{dy},{dz})"
                                            );
                                            assert_eq!(
                                                values[g.t_far].to_bits(),
                                                hit.t_far.to_bits(),
                                                "t_far differs at origin ({ox},{oy},{oz}) dir \
                                                 ({dx},{dy},{dz})"
                                            );
                                        }
                                        None => misses += 1,
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(checked > 100_000, "grid is large enough");
    assert!(hits > 0 && misses > 0, "grid exercises both outcomes");
}

#[test]
fn graph_rejects_wrong_arity() {
    let g = ray_aabb_graph();
    assert!(g.graph.eval_f64(&[0.0; 3]).is_none());
}
