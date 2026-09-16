# tpt-certus

**Formally Verified 3D Spatial Foundation — TPT Solutions**

Status: **Draft v3 / Phase 1 Initiation** · License: **MIT OR Apache-2.0**
Authoritative spec: [`spec.txt`](spec.txt) · Task tracker: [`todo.md`](todo.md)

---

`tpt-certus` is a pure-Rust, domain-agnostic, formally verified 3D spatial library
suite. Unlike traditional rendering engines that prioritize visual fidelity or
game-like features, `tpt-certus` prioritizes **mathematical certainty, deterministic
behavior, and auditability**.

Leveraging the `tpt-telos` agentic compiler and its self-contained Fourier-Motzkin
SMT-style solver, `tpt-certus` proves spatial relationships, collision detection,
and state transitions correct against a two-layer specification:

1. **Ideal model (QF_LRA, exact rationals)** — proven with Fourier-Motzkin
   elimination.
2. **Sound f64 realization** — every guarantee is paired with a machine-checked
   bound on the arithmetic error the CPU actually performs, with `ε` auto-derived
   from IEEE-754 rounding analysis.

This is a stronger claim than "proven over the reals" alone: every guarantee ships
with a Proof Certificate stating exactly what was proven, over which number system,
and within what bound — usable as genuine evidence in DO-178C, FDA, and
financial-model-risk review processes.

To our knowledge, `tpt-certus` is among a very small set of 3D spatial libraries
where every geometric guarantee comes with a machine-checked certificate. The
load-bearing distinctiveness is the two-layer form of that certificate — exactly
what was proven, over which number system (ideal reals *and* IEEE-754 f64
realization with derived error bound), and within what bound. The closest
known work is sharp: verified 3D intersection algorithms exist in exact
arithmetic (theorem provers, e.g. Lean 4), and production Rust spatial indexes
exist but are unverified; none surveyed publishes a per-build two-layer
certificate over the executing artifact. Positioning confirmed by a competitive
survey, Phase 1.7 of spec Section 7 (findings recorded in `todo.md`).

## Target industries

| Industry | The gap | The `tpt-certus` guarantee |
| :--- | :--- | :--- |
| Medical imaging & surgical planning | Float clipping / bounding-volume errors near critical anatomy | Verified voxel & mesh bounds with sub-micron bounded containment error |
| AV / robotics simulation | Rendering/physics bugs poison training data | Deterministic, cryptographically hashed spatial-state replay |
| Aerospace & avionics digital twins | Modern engines too opaque to certify | Certifiable projection math with explicit float error bounds and DO-330 audit logs |
| Financial/high-stakes spatial data | Rendering artifacts mislead analysts | Lossless, bijective data projection with certified maximum distortion |

See spec Section 2 for the full table.

## Architecture

Six crates compose the workspace:

| Crate | Purpose |
| :--- | :--- |
| `tpt-certus-math` | Core linear algebra, bounded floats, verified vector/matrix ops (on `tpt-math`) |
| `tpt-certus-geometry` | 3D primitives: `Vector3`, `AABB`, planes, voxels, basic meshes (on `tpt-engineering`) |
| `tpt-certus-spatial` | **Core engine.** Spatial partitioning (BVH/Octree), broad/narrow-phase collision, raycasting |
| `tpt-certus-state` | Deterministic state management, cryptographic hashing, temporal replay/audit |
| `tpt-certus-render` | Minimal headless WebGPU (`wgpu`) visualizer — a pure function of verified state |
| `tpt-certus-proof` | Proof-certificate assembly, error-bound bookkeeping, DO-330 audit-log export |

## Verification strategy (two-layer soundness)

Every verified function carries two linked contracts:

- an **ideal contract**, proven over QF_LRA / exact rationals, and
- a **realization contract**, stating the f64 result differs from the ideal result
  by no more than `ε`, where `ε` is **derived** (interval arithmetic over the
  function's operation graph), not asserted by hand.

`tpt-telos` auto-derives the realization-layer `ε` from the ideal contract plus the
operation graph, so authors write one specification and get both proofs. Until
Phase 2's DO-330 tool-qualification analysis completes, the ε-autoderivation
mechanism is treated as a documented **trust assumption**; the per-function ideal
and realization contracts it produces are still individually checked by the FM
solver.

Composition is handled by proving construction/mutation *functions* at build time
(`insert`, `rebalance`, `split_node`) with per-node-type invariants and one fixed
composition proof by induction — no runtime proof cost, no per-node re-proofing.

## Proof certificates

`tpt-certus-proof` assembles, per build, a Proof Certificate containing the ideal
and realization contract (with derived `ε`) for every verified function reachable
from the build, the composition lemmas used, and a machine-readable manifest
cross-referencing certificate entries to source locations and regulatory objectives
(DO-178C Table A-5). **If any proof obligation fails, the build fails — no partial
or best-effort certificate is ever emitted.**

## Repository layout

```
tpt-certus/
├── Cargo.toml                 # workspace root (6 member crates)
├── crates/
│   ├── tpt-certus-math/       # bounded floats, linalg wrappers
│   │   ├── Cargo.toml
│   │   ├── telos/             # .telos sources (Phase 1+)
│   │   └── src/               # hand-written + src/generated/ (never committed)
│   ├── tpt-certus-geometry/   # Vector3, AABB, Ray, etc.
│   ├── tpt-certus-spatial/    # verified spatial algorithms (ray casting, BVH, ...)
│   ├── tpt-certus-state/      # deterministic state, hashing, temporal replay
│   ├── tpt-certus-render/     # headless wgpu visualizer
│   └── tpt-certus-proof/      # proof-certificate assembly
├── spec.txt                   # authoritative specification (v3)
├── todo.md                    # phased task tracker
└── .github/workflows/ci.yml   # build + test + clippy + telos verify gate
```

**`.telos` source vs generated `.rs` policy:**
`.telos` sources live per-crate in `telos/` (e.g. `crates/tpt-certus-spatial/telos/ray_aabb.telos`).
`tpt-telos build` generates the corresponding Rust into `src/generated/` (e.g. `crates/tpt-certus-spatial/src/generated/ray_aabb.rs`).
Generated files are **never committed** — they are rebuilt on every CI run and local build, ensuring the Proof Certificate always reflects the current `.telos` source.
A `build.rs` (added in Phase 1) will trigger regeneration automatically; CI enforces the invariant with `telos verify` over all `**/*.telos` sources plus a post-generation drift check.

## Status / roadmap

Phase 0 (repository & workspace bootstrap) is in progress. See [`spec.txt`](spec.txt)
Section 7 and [`todo.md`](todo.md) for the full phased roadmap (DO-330 tool
qualification, independent audit, reference submission package, financial
model-risk track).

We do **not** claim "regulatory submission ready" before the independent audit
(~Phase 3) completes, and the financial model-risk pathway is not established until
its dedicated phase (~Phase 5) completes.

## License

`tpt-certus` is dual-licensed under **MIT OR Apache-2.0**:

- [MIT](LICENSE-MIT)
- [Apache-2.0](LICENSE-APACHE)

Copyright (c) 2026 TPT Solutions.