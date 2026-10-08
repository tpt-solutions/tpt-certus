//! Internal verifier for tpt-certus.
//!
//! This crate replaces the external `tpt-telos` dependency.  It is built from
//! three layers, deliberately kept apart so that only the smallest one needs
//! to be trusted:
//!
//! * [`rational`] and [`poly`] — exact arithmetic (`i128` rationals and sparse
//!   polynomials), fail-closed on overflow.
//! * [`refute`] — the **trusted checker**.  It re-verifies a Positivstellensatz
//!   refutation certificate with exact arithmetic and never searches.
//!
//! Certificate *generation* is untrusted and lives outside the checker's
//! trust boundary; a certificate counts only once [`refute::check`] accepts it.
//!
//! # Status
//!
//! Milestone 0 (spike) is complete.  [`search`] is an untrusted degree-2
//! certificate generator: it proposes refutations via exact simplex and every
//! proposal is re-checked by [`refute::check`] before being returned.  It
//! rediscovers the single-axis slab lemma automatically (`tests/slab_lemma.rs`).
//! The IR, verification-condition generation and the `ray_intersects_aabb`
//! contract are later milestones.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod ir;
pub mod poly;
pub mod ray_aabb;
pub mod rational;
pub mod refute;
pub mod search;

