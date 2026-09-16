# tpt-certus — Project TODO

Formally verified 3D spatial foundation. TPT Solutions. Dual-licensed MIT OR
Apache-2.0. Tracks spec.txt v3 end to end, phase by phase.

---

## Phase 0 — Repository & Workspace Bootstrap

> **Dependency note (Phase 0 resolution):** `tpt-math` and `tpt-engineering`
> sub-crates are consumed from crates.io.  `tpt-formal` (`tpt-for-model-check`
> and `tpt-for-vcgen`) is consumed from GitHub (`master` branch) since it is
> not published to crates.io.  `tpt-telos` (the CLI) is a binary-only crate
> consumed via `cargo install tpt-telos --version 0.2.0 --locked` in CI and
> locally; the programmatic orchestration API (`tpt-telos-sdk` 0.2.0) is a
> crates.io dependency used by `tpt-certus-proof`.  The `../tpt-telos` local
> path was unavailable; the crates.io pinned release is used instead, which
> also ensures reproducible builds for DO-330 reproducibility goals.

- [x] `git init` the `tpt-certus` repo
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (dual license: MIT OR Apache-2.0)
- [x] Add `README.md` (positioning summary from spec Section 1, TPT Solutions authorship)
- [x] Add `.gitignore` (Rust/Cargo + `.telos` build artifacts)
- [x] Create Cargo workspace `Cargo.toml` with the 6 Section-4 crates as members:
  - [x] `tpt-certus-math`
  - [x] `tpt-certus-geometry`
  - [x] `tpt-certus-spatial`
  - [x] `tpt-certus-state`
  - [x] `tpt-certus-render`
  - [x] `tpt-certus-proof`
- [x] Confirm exact crates.io package names/versions for `tpt-math` (incl. `tpt-math-geometry`, `tpt-math-linalg`) and `tpt-engineering` (incl. `tpt-eng-geometry`, `tpt-eng-mesh`); add as dependencies
- [x] Add `tpt-formal` (incl. `tpt-for-model-check`, `tpt-for-vcgen`) as a git dependency pointing at the correct GitHub URL/branch
- [x] Add `tpt-telos` as a path or git dependency (build-time tool, `../tpt-telos` locally) for the `.telos` → Rust compile step
- [x] CI skeleton: `cargo build`/`cargo test` for the workspace, plus a `telos verify`/`telos build` gate over all `.telos` sources
- [x] Decide and document repo layout for `.telos` source files vs. generated `.rs` (e.g. `telos/` source dir, `src/generated/` output, regenerate-on-build vs. commit-generated-output policy)

---

## Phase 1 — Proof of Concept: Verified Ray-AABB Intersection

Spec Sections 4, 6, 8. Goal: fully close the real/float gap and boundary-case
gaps for `ray_intersects_aabb`, as the template all later crates follow.

### 1.1 Core crate scaffolding for the PoC

- [ ] Scaffold `tpt-certus-math`: core linear algebra, bounded floats, verified vector/matrix ops (on top of `tpt-math`)
- [ ] Scaffold `tpt-certus-geometry`: `Vector3`, `AABB`, and other 3D primitives (on top of `tpt-engineering`)
- [x] Scaffold `tpt-certus-spatial`: home of `ray_aabb.rs` / spatial partitioning (Phase 0 scaffold + reference slab impl in `src/lib.rs`)
- [x] Scaffold `tpt-certus-proof`: proof-certificate assembly crate (on top of `tpt-formal`)
- [x] Scaffold `tpt-certus-math` / `tpt-certus-geometry` (Phase 0; `Vector3`, `AABB`, `Ray` primitives + bounded-float placeholder)

### 1.2 `.telos` source authoring for `ray_intersects_aabb`

> **Phase 0/1 drafting findings (tpt-telos v0.2.0, verified with the installed
> binary against the workspace):**
> 1. **No `==>` / symbolic `let ideal = ...`:** the grammar has no implication
>    operator and no symbolic-evaluation binding.  Realization-layer clauses
>    must be re-expressed once upstream adds these.
> 2. **`if` conditions are severely restricted:** single comparison only, and
>    only `==`/`!=` over non-field operands.  Relational (`<`, `>=`) and
>    field-access guards fail to parse, so the slab method's relational gates
>    (`t_max_slab >= t_min`, `t_near <= t_max`) are **not expressible as control
>    flow** in v0.2.0.
> 3. **Mutation model only:** `mutate state { ... }` is the sole mutation site;
>    `=` inside if-arms fails to parse (only `+=`/`-=` parse there, and only in
>    single-level ifs).  No `ensures result == ...` for pure return values and
>    no `Option` post-conditions.
> 4. **`telos build` codegen is lossy:** for the draft `ray_aabb.telos` it
>    emitted a `Box` struct **missing contract-only fields** (`max_y`, `min_z`)
>    and typed `Float64` as `i64`, so the generated Rust did not compile.
> 5. Float32/Float64 are tracked as integer constraints; IEEE-754 interval
>    arithmetic + rounding-error bounds are documented as future work.
>
> Net: the Section 6 contract is the *authored* ideal-layer contract (recorded
> verbatim in `crates/tpt-certus-spatial/telos/ray_aabb.telos`), but only the
> invariant-maintenance and straight-line equality slice can be **machine
> verified** today.  The rest is blocked on upstream `tpt-telos` statement-level
> control flow + float semantics (same upstream work as 1.3).

Draft authored and passing `telos verify` (CI-safe): `crates/tpt-certus-spatial/telos/ray_aabb.telos`
- [x] `requires`: `aabb.min <= aabb.max` on all axes — machine-checked as `invariant Box` + `requires`
- [x] Explicit NaN/finiteness out-of-scope note — recorded in-draft and in `tpt-certus-geometry`/`tpt-certus-spatial` scope notes
- [ ] `requires`: `t_max > 0.0` (explicit bounded domain) — expressible in `requires`; pending full function that uses it
- [ ] `requires`: ray direction non-zero on at least one axis — expressible via `!=` disjunction (DNF-supported); pending full function
- [ ] `ensures`: valid intersection bounds (`0 <= t_near <= t_far`, `t_near <= t_max`, `t_far` intentionally unclipped) — blocked: relational control flow unparseable
- [ ] `ensures`: origin strictly inside AABB ⟹ hit at `t_near == 0.0` — blocked as above
- [ ] `ensures`: origin exactly on a boundary face ⟹ treated as inside (closes the boundary gap from Appendix A) — partially advanced: `boundary_min_x` slice verifies equality gate + `mutate` assignment (`out.flag` 0→1); full field/relational form blocked
- [ ] `telos parse` / `telos verify` passes on the ideal-layer contract — passes for the authored slice; full contract blocked on upstream
- [ ] `telos build` generates `ray_aabb.rs` into `tpt-certus-spatial/src/` — **blocked**: codegen drops contract-only fields and lossily demotes floats to integers; generated Rust did not compile
- [ ] Confirm generated implementation has no `TANGENT_THRESHOLD`-style magnitude shortcut — only an exact-zero direction check gates the parallel/containment branch (per Appendix A) — pending a compiling generated implementation
- [ ] Confirm each realization-layer `ensures` clause independently re-binds `let ideal = ...` (no shared-scope assumption across clauses) — moot until `==>`/symbolic bindings land

### 1.3 Realization layer / ε auto-derivation

> **Phase 0 finding (tpt-telos v0.2.0):** toolchain inspected via
> `docs/LANGUAGE.md` and `examples/float.telos`.  Float32/Float64 are parsed and
> codegen'd, but the IR tracks them as *integer constraints* (QF_LRA) and the
> docs themselves mark "IEEE 754 interval arithmetic and rounding error bounds"
> as **future work**.  The realization-layer `ε` auto-derivation from Section 5.1
> therefore does **not** exist in v0.2.0 — it must be filed as upstream `tpt-telos`
> work (or implemented in-repo) before Phase 1 can close.  See below.

- [ ] Verify whether `tpt-telos` v0.2.0 actually implements auto-derivation of realization-layer `ε` from the ideal contract + operation graph (interval arithmetic over IEEE-754 rounding) — this is load-bearing for Section 5.1's claim and may not exist yet
  - [ ] **Likely missing:** v0.2.0 docs/examples describe float contracts as approximate (QF_LRA integer tracking, no IEEE-754 interval arithmetic yet) → file upstream `tpt-telos` work before Phase 1 can close
- [ ] `ensures`: bounded numerical error vs. ideal result (`|f64 result − ideal result| <= EPSILON_T`)
- [ ] `ensures`: no false negatives within the bounded domain (`ideal.is_some() ⟹ result.is_some()`)
- [ ] Confirm `EPSILON_T` is published per-build in the Proof Certificate, not hand-asserted

### 1.4 Proof Certificate (`tpt-certus-proof`)

- [x] **Machine-readable manifest** — first draft implemented in `src/lib.rs`: `MANIFEST_VERSION` (v1), structured `SourceLocation` (file + validated 1-based line span, rejects inverted/zero spans), `RegulatoryObjective` enum (DO-178C Table A-5 requirements-verification / formal-methods, DO-330 tool-qualification, FDA design-controls) with stable serde identifiers, `composition_lemmas`, pinned `telos_version` field, and deterministic `to_json()`/`to_json_pretty()` export (field order = declaration order → byte-identical for identical content)
- [ ] Implement per-build Proof Certificate assembly: ideal + realization contracts for every verified function reachable from the build — blocked: assembly consumes `tpt-telos build` artifacts, whose codegen is contract-lossy (Phase 1.2 finding)
- [x] Include composition lemmas used (n/a for Phase 1's single function, but wire the field) — `CertificateEntry::composition_lemmas` field wired and serialized
- [ ] Cross-reference manifest entries to DO-178C Table A-5 objective rows — enum identifiers in place; full row-level mapping with the DO-330 objective mapping reviewed in Phase 2
- [ ] Wire hard CI gate: any failed proof obligation fails the build; no partial/best-effort certificate is ever emitted — `is_complete()` hard-gate semantics defined (`!empty && all source spans valid`); CI wiring lands with `telos build` integration
- [ ] First-draft Proof Certificate generated for the Phase 1 crate and reviewed against the DO-330 objective mapping (Section 7, Phase 1) — blocked on `telos build` (Phase 1.2 finding)

### 1.5 FM-elimination scalability benchmark (Section 5.2 / exit criterion 2)

- [ ] Build synthetic BVH construction/mutation functions (`insert`, `rebalance`, `split_node`) in `.telos`
- [ ] Benchmark FM elimination proof time + memory at build time, at depths 4, 8, 12, and 16
- [ ] Record results as the evidence gate for committing to "self-contained solver, no external SMT dependency" as a permanent architectural principle

### 1.6 ε-derivation generalization check (exit criterion 4)

- [ ] Demonstrate the ε auto-derivation mechanism on at least 2 functions beyond `ray_intersects_aabb`

### 1.7 Competitive survey (Section 7 Phase 1 / exit criterion 5)

- [x] Lightweight survey of formally-verified spatial/rendering libraries
- [x] Confirm or revise Section 1's "among a very small set... to our knowledge" positioning claim based on survey results — **confirmed (substantiated), no revision required; prior-art names recorded below**

> **Phase 1.7 survey record (completed Sep 2026):**
> Web-searched formally verified spatial/rendering software (Lean 4, Coq, Verus,
> SPARK, Rust verification crates).  Findings:
>
> - **`schildep/verified-3d-mesh-intersection`** (Lean 4, Jul 2026) — "first formally
>   verified 3D CSG mesh intersection."  93-line trusted spec; 1,000+ LOC AI-written
>   implementation + 60,000 LOC AI-written Lean proofs, all machine-checked; only
>   trusted axioms `[propext, Classical.choice, Quot.sound]`.  Verifies exact
>   real/rational geometry (surface set-equality, well-formedness of triangulation),
>   handles ray/edge/vertex-coplanar boundary cases by *inequality design*, not
>   by an explicit f64 rounding bound.  24 s for two 70k-triangle bunnies —
>   correctness prioritized over performance.  Predecessor: `verified-polygon-
>   intersection` (Lean 4, 2D), which cites PVS work on polygon clipping.
> - **`kmill/lean4-raytracer`** (Lean 4) — a full raytracer written in Lean (Float),
>   an executable artifact, but *not* a correctness proof product (no verified
>   intersection-contract goal).
> - **`packed_spatial_index`** (Rust, crates.io) — fast SIMD Hilbert R-tree; superb
>   memory-safety hardening + adversarial fuzzing (SAFETY.md), but no formal
>   verification of geometry or search semantics.
> - **`geo-index`** (Rust/Python) — immutable packed R-tree / k-d tree (flatbush/
>   kdbush-compatible, 2D only); no formal verification.
> - **`ruvector-verified`** (Rust + lean-agentic) — proof-carrying *type-level*
>   dimension checks for vectors/HNSW (sub-µs, attestation witnesses); niche and
>   not spatial-geometry correctness.
> - **`semi-persistent-containers-verus`** (Verus) — 1,703 machine-checked facts on
>   semi-persistent containers incl. a verified B+Tree; shows Verus as a viable
>   in-Rust alternativespace, but no spatial primitives.
> - **SPARK Ada raytrace teapot** (AdaCore blog) — educational SPARK contracts /
>   pre-post on a raytracer; DO-178C-adjacent tooling, not a proof-certificate product.
>
> **Assessment:** every near-match either (a) verifies *algorithms* in a theorem
> prover over exact arithmetic (Lean/Coq/PVS), not a two-layer real+f64-realization
> certificate over the *executing* artifact; or (b) is a Rust spatial library that
> is safety-hardened but unverified.  None surveyed provides "every geometric
> guarantee → machine-checked certificate stating what was proven, over which
> number system, and within what bound" for an executable Rust spatial
> *library*.  Section 1's claim therefore stands as written; the differentiating
> load-bearing phrase is *"over which number system, and within what bound"*.
> Honest limits: algorithmic-novelty claims must not be implied — theorem-prover
> verified 3D intersection exists (schildep); `tpt-certus`'s contribution is the
> per-build two-layer certificate + DO-330/regulatory-oriented packaging over
> executable Rust, plus our existential-exclusion caveat remains in scope
> ("to our knowledge", "very small set").

### 1.8 Remaining crates — high-level placeholders only

- [x] `tpt-certus-state`: scaffold crate; define deterministic state management, cryptographic hashing, and temporal replay/audit goals (on top of `tpt-formal`) — `src/lib.rs` doc: canonical serialization, SHA-256 per-timestep hash, ordered replay/audit with hash verification
- [x] `tpt-certus-render`: scaffold crate; define minimal headless `wgpu` visualizer goals (pure function of verified spatial state) — `src/lib.rs` doc: deterministic pure-function rasterization from verified spatial state; no mutation/fabrication of spatial data; not itself formally verified

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

- [x] Confirm the Section 1 "among a very small set" positioning claim is still tenable after the Phase 1.7 survey → **confirmed**. Closest prior art (`schildep` Lean 4 verified 3D CSG mesh intersection) verifies a 3D spatial *algorithm* over exact arithmetic; it does not publish a two-layer f64-realization certificate. Survey record appended under Phase 1.7.
- [x] Confirm exact `tpt-formal` GitHub repository URL and branch, and that it exposes `tpt-for-model-check` / `tpt-for-vcgen` → `https://github.com/tpt-solutions/tpt-formal`, `master` branch. Both sub-crates are published at v0.1.0.
- [x] Confirm exact crates.io package names/versions for `tpt-math` and `tpt-engineering` sub-crates → `tpt-math-geometry` 0.1.1, `tpt-math-linalg` 0.1.0, `tpt-eng-geometry` 0.1.0, `tpt-eng-mesh` 0.1.0. All on crates.io.
- [ ] Confirm `tpt-telos`'s ε-auto-derivation is actually implemented (not just described in `tpt-telos`'s own docs/roadmap) before Phase 1 depends on it → see Phase 1.3, exit criterion for Phase 1 closure. **No.** v0.2.0 tracks floats as integer constraints and marks IEEE-754 interval arithmetic / rounding-error bounds as future work; `telos build` codegen additionally drops contract-only struct fields and demotes floats to integers (see Phase 1.2 drafting findings).
- [ ] Upstream `tpt-telos` work required before Phase 1 ideal contract can fully close: (a) statement-level `if` with relational (`<`, `<=`, `>`, `>=`) and field-access guards; (b) `==>` implication and/or `ensures result == ...` for pure functions; (c) contract-faithful `telos build` codegen (preserve all referenced fields; correct float typing); (d) IEEE-754 interval arithmetic + auto-derived `ε`.
- [ ] Do not claim "regulatory submission ready" before Phase 3 completes; do not claim an established financial model-risk pathway before Phase 5 completes (per spec Section 7 closing note) → ongoing constraint.
