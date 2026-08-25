# tpt-certus — Project TODO

Formally verified 3D spatial foundation. TPT Solutions. Dual-licensed MIT OR
Apache-2.0. Tracks spec.txt v3 end to end, phase by phase.

---

## Phase 0 — Repository & Workspace Bootstrap

- [ ] `git init` the `tpt-certus` repo
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` (dual license: MIT OR Apache-2.0)
- [ ] Add `README.md` (positioning summary from spec Section 1, TPT Solutions authorship)
- [ ] Add `.gitignore` (Rust/Cargo + `.telos` build artifacts)
- [ ] Create Cargo workspace `Cargo.toml` with the 6 Section-4 crates as members:
  - [ ] `tpt-certus-math`
  - [ ] `tpt-certus-geometry`
  - [ ] `tpt-certus-spatial`
  - [ ] `tpt-certus-state`
  - [ ] `tpt-certus-render`
  - [ ] `tpt-certus-proof`
- [ ] Confirm exact crates.io package names/versions for `tpt-math` (incl. `tpt-math-geometry`, `tpt-math-linalg`) and `tpt-engineering` (incl. `tpt-eng-geometry`, `tpt-eng-mesh`); add as dependencies
- [ ] Add `tpt-formal` (incl. `tpt-for-model-check`, `tpt-for-vcgen`) as a git dependency pointing at the correct GitHub URL/branch
- [ ] Add `tpt-telos` as a path or git dependency (build-time tool, `../tpt-telos` locally) for the `.telos` → Rust compile step
- [ ] CI skeleton: `cargo build`/`cargo test` for the workspace, plus a `telos verify`/`telos build` gate over all `.telos` sources
- [ ] Decide and document repo layout for `.telos` source files vs. generated `.rs` (e.g. `telos/` source dir, `src/generated/` output, regenerate-on-build vs. commit-generated-output policy)

---

## Phase 1 — Proof of Concept: Verified Ray-AABB Intersection

Spec Sections 4, 6, 8. Goal: fully close the real/float gap and boundary-case
gaps for `ray_intersects_aabb`, as the template all later crates follow.

### 1.1 Core crate scaffolding for the PoC

- [ ] Scaffold `tpt-certus-math`: core linear algebra, bounded floats, verified vector/matrix ops (on top of `tpt-math`)
- [ ] Scaffold `tpt-certus-geometry`: `Vector3`, `AABB`, and other 3D primitives (on top of `tpt-engineering`)
- [ ] Scaffold `tpt-certus-spatial`: home of `ray_aabb.rs` / spatial partitioning
- [ ] Scaffold `tpt-certus-proof`: proof-certificate assembly crate (on top of `tpt-formal`)

### 1.2 `.telos` source authoring for `ray_intersects_aabb`

- [ ] Write `ray_intersects_aabb.telos` with the ideal-layer (QF_LRA) contract:
  - [ ] `requires`: `aabb.min <= aabb.max` on all axes
  - [ ] `requires`: `t_max > 0.0` (explicit bounded domain)
  - [ ] `requires`: ray direction non-zero on at least one axis
  - [ ] `ensures`: valid intersection bounds (`0 <= t_near <= t_far`, `t_near <= t_max`, `t_far` intentionally unclipped)
  - [ ] `ensures`: origin strictly inside AABB ⟹ hit at `t_near == 0.0`
  - [ ] `ensures`: origin exactly on a boundary face ⟹ treated as inside (closes the boundary gap from Appendix A)
  - [ ] Explicit NaN/finiteness out-of-scope note (malformed inputs rejected upstream by `tpt-certus-geometry`)
- [ ] `telos parse` / `telos verify` passes on the ideal-layer contract
- [ ] `telos build` generates `ray_aabb.rs` into `tpt-certus-spatial/src/`
- [ ] Confirm generated implementation has no `TANGENT_THRESHOLD`-style magnitude shortcut — only an exact-zero direction check gates the parallel/containment branch (per Appendix A)
- [ ] Confirm each realization-layer `ensures` clause independently re-binds `let ideal = ...` (no shared-scope assumption across clauses)

### 1.3 Realization layer / ε auto-derivation

- [ ] Verify whether `tpt-telos` v0.2.0 actually implements auto-derivation of realization-layer `ε` from the ideal contract + operation graph (interval arithmetic over IEEE-754 rounding) — this is load-bearing for Section 5.1's claim and may not exist yet
  - [ ] If missing: file/track as upstream `tpt-telos` work before Phase 1 can close
- [ ] `ensures`: bounded numerical error vs. ideal result (`|f64 result − ideal result| <= EPSILON_T`)
- [ ] `ensures`: no false negatives within the bounded domain (`ideal.is_some() ⟹ result.is_some()`)
- [ ] Confirm `EPSILON_T` is published per-build in the Proof Certificate, not hand-asserted

### 1.4 Proof Certificate (`tpt-certus-proof`)

- [ ] Implement per-build Proof Certificate assembly: ideal + realization contracts for every verified function reachable from the build
- [ ] Include composition lemmas used (n/a for Phase 1's single function, but wire the field)
- [ ] Machine-readable manifest cross-referencing certificate entries to source locations and regulatory objectives (e.g. DO-178C Table A-5)
- [ ] Wire hard CI gate: any failed proof obligation fails the build; no partial/best-effort certificate is ever emitted
- [ ] First-draft Proof Certificate generated for the Phase 1 crate and reviewed against the DO-330 objective mapping (Section 7, Phase 1)

### 1.5 FM-elimination scalability benchmark (Section 5.2 / exit criterion 2)

- [ ] Build synthetic BVH construction/mutation functions (`insert`, `rebalance`, `split_node`) in `.telos`
- [ ] Benchmark FM elimination proof time + memory at build time, at depths 4, 8, 12, and 16
- [ ] Record results as the evidence gate for committing to "self-contained solver, no external SMT dependency" as a permanent architectural principle

### 1.6 ε-derivation generalization check (exit criterion 4)

- [ ] Demonstrate the ε auto-derivation mechanism on at least 2 functions beyond `ray_intersects_aabb`

### 1.7 Competitive survey (Section 7 Phase 1 / exit criterion 5)

- [ ] Lightweight survey of formally-verified spatial/rendering libraries
- [ ] Confirm or revise Section 1's "among a very small set... to our knowledge" positioning claim based on survey results

### 1.8 Remaining crates — high-level placeholders only

- [ ] `tpt-certus-state`: scaffold crate; define deterministic state management, cryptographic hashing, and temporal replay/audit goals (on top of `tpt-formal`)
- [ ] `tpt-certus-render`: scaffold crate; define minimal headless `wgpu` visualizer goals (pure function of verified spatial state)

---

## Phase 2 — `tpt-telos` DO-330 Tool Qualification

- [ ] TQL (Tool Qualification Level) assessment of `tpt-telos` under DO-330
- [ ] Explicitly cover the ε-auto-derivation mechanism (Section 5.1) in the assessment
- [ ] Produces qualified-tool evidence for `tpt-telos` output, not just supplementary documentation

---

## Phase 3 — Independent Audit

- [ ] Third-party audit (DO-178C/FDA design-control experience) of the FM-elimination soundness argument
- [ ] Third-party audit of the realization-layer ε-derivation
- [ ] Confirms the two-layer soundness claim (Section 5.1) before it appears in customer-facing certification packages

---

## Phase 4 — Reference Submission Package

- [ ] Build a redacted reference submission package against the Phase 1 ray-AABB primitive
- [ ] Package serves as a template for customers' own regulatory submissions

---

## Phase 5 — Financial Model-Risk Track

- [ ] Map Proof Certificate contents to a model-risk-management framework (e.g. SR 11-7-style effective-challenge and documentation expectations)
- [ ] Conduct in consultation with a financial model-risk practitioner
- [ ] Gives the Section 2 financial-industry row a staged path equivalent to aerospace/medical (Phases 1–4)

---

## Open risks / follow-ups

- [ ] Confirm exact `tpt-formal` GitHub repository URL and branch, and that it exposes `tpt-for-model-check` / `tpt-for-vcgen`
- [ ] Confirm exact crates.io package names/versions for `tpt-math` and `tpt-engineering` sub-crates
- [ ] Confirm `tpt-telos`'s ε-auto-derivation is actually implemented (not just described in `tpt-telos`'s own docs/roadmap) before Phase 1 depends on it
- [ ] Do not claim "regulatory submission ready" before Phase 3 completes; do not claim an established financial model-risk pathway before Phase 5 completes (per spec Section 7 closing note)
