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
> binary against the workspace, plus source review of the v0.2.0 crates —
> `parser`, `ir`, `codegen`, `verify-manifest`):**
> 1. **No `==>` / symbolic `let ideal = ...`:** the grammar has no implication
>    operator and no symbolic-evaluation binding.  Realization-layer clauses
>    must be re-expressed (as DNF double-negation/disjunction) until then.
> 2. **`if`/`else` *is* supported in function bodies, but with a single
>    comparison guard only:** all six relations (`==`, `!=`, `<`, `<=`, `>`,
>    `>=`) and **field operands** work (`tpt-telos-ir` `process_body_stmts`)
>    — so the slab gates (`t_max_slab >= t_min`, `t_near <= t_max`) ARE
>    expressible as single-comparison guards.  Only **compound** (`&&`/`||`)
>    guards are rejected (`if`-guard must be a single comparison).  `match`
>    with mutating arms is rejected; non-mutating `let`/`return` parse but are
>    **no-ops in the IR body analysis**, so return-value post-conditions
>    (`ensures result == ...`) are not verifiable yet.
> 3. **Mutation model only — but `=` parses in if-arms:** `mutate state { ... }`
>    is the designed mutation site and plain `=`/`+=`/`-=` assignments parse
>    inside if/else arms (parser + IR both handle `Stmt::Assign`).  No `Option`
>    post-conditions (only `Result<T,E>`); no verified return values.
> 4. **Codegen float handling — `transpile`/`build` demotes floats and drops
>    invariant-only fields (empirically re-confirmed v0.2.0):** `telos build`
>    on the draft compiles the generated Rust and **fails** (E0609 unknown
>    fields `max_y`/`min_z`): the emitted `Box` drops every field referenced
>    *only* by the `invariant` (not by any param/`requires`/`mutate`), while
>    `satisfies_invariants()` still references them; `Float64` params (`px`,
>    `py`, `pz`) render as `i64`.  The main `render_type` maps `Float64 → f64`
>    (`codegen/src/lib.rs`) and the **`project`/`eject` FFI path rejects floats
>    outright** (`codegen/src/ffi.rs` "only integer types"), but the default
>    agentic `transpile` output observed in practice demotes floats and drops
>    invariant-only fields — so `telos build` output is still not contract- or
>    type-faithful for our draft.
> 5. Float32/Float64 parse and codegen, but the **IR has no float semantics**:
>    the verifier is QF_LRA over integer/real atoms; interval arithmetic exists
>    only for bounding nonlinear *integer* products (`used_interval_bounding`
>    per-function flag in `telos-proof.json`), not IEEE-754 rounding bounds.
>    ε auto-derivation remains absent (Phase 1.3).
>
> Net: the Section 6 contract is the *authored* ideal-layer contract (recorded
> verbatim in `crates/tpt-certus-spatial/telos/ray_aabb.telos`).  A meaningful
> slice can be **machine verified** today: invariant maintenance, straight-line
> equality/`mutate` slices, and single-comparison `if`/`else` traversal with
> mutating arms.  Genuinely blocked: compound guards, mutating `match`,
> verified return values / `Option`, and the entire f64 realization layer.

Draft authored and passing `telos verify` (CI-safe): `crates/tpt-certus-spatial/telos/ray_aabb.telos`
- [x] `requires`: `aabb.min <= aabb.max` on all axes — machine-checked as `invariant Box` + `requires`
- [x] Explicit NaN/finiteness out-of-scope note — recorded in-draft and in `tpt-certus-geometry`/`tpt-certus-spatial` scope notes

> **Phase 1.2 empirical re-verification (v0.2.0 binary, scratch `.telos` in
> temp):** the earlier "relational/field `if` guards fail to parse" record was
> **wrong**.  Confirmed with the installed binary:
> - `if b >= out.value { mutate state { out.flag = 1; out.value = b } } else { ... }`
>   → **both branches verified PASS** (relational guard + field access + `=`
>   assignments in arms all work).
> - Compound guard `if b >= 3 && out.flag == 0 { ... }` → rejected with the
>   exact message from `tpt-telos-ir` `process_body_stmts`.
> - `match` with a state-mutating arm → rejected with the exact message from
>   `process_body_stmts`.
> - `let t = b;` then `out.value = t` with `ensures out.value == b` → **FAIL**
>   (counterexample `out.value'=0, b=1`): `let` is a no-op in the IR body
>   analysis (grammar's "in IR this becomes `x == e`" is not implemented).
> - `telos build` on the draft → generated Rust **does not compile** (E0609
>   unknown fields `max_y`/`min_z`); `transpile` emits `Hit.t_near: f64` but
>   `px: i64`, and drops invariant-only `Box` fields.  Floats are not
>   type-faithful on the default agentic path.
- [ ] `requires`: `t_max > 0.0` (explicit bounded domain) — expressible in `requires`; pending full function that uses it
- [ ] `requires`: ray direction non-zero on at least one axis — expressible via `!=` disjunction (DNF-supported); pending full function
- [ ] `ensures`: valid intersection bounds (`0 <= t_near <= t_far`, `t_near <= t_max`, `t_far` intentionally unclipped) — **partial**: `slab_hit_decided` + `domain_overlap_detected` machine-verify the relational hit-gate shape (`t_max_slab >= t_min`, `t_min >= 0`) with `=` arms (confirmed v0.2.0); blocked on verified return values (`ensures result == ...`) to state the actual bounds
- [ ] `ensures`: origin strictly inside AABB ⟹ hit at `t_near == 0.0` — blocked on `==>`/return-value post-conditions
- [ ] `ensures`: origin exactly on a boundary face ⟹ treated as inside (closes the boundary gap from Appendix A) — partially advanced: `boundary_min_x` slice verifies equality gate + `mutate` assignment (`out.flag` 0→1); full field/relational form blocked on return-value semantics
- [ ] `telos parse` / `telos verify` passes on the ideal-layer contract — **passes for the current draft** (4 funcs, all branches PASS, incl. relational/field guards, nested `if`, `=`/`+=` arms, DNF `||` ensures); full contract blocked on `==>` + return-value verification
- [ ] `telos build` generates `ray_aabb.rs` into `tpt-certus-spatial/src/` — **blocked (empirically re-confirmed v0.2.0)**: generated crate does not compile. Auto-inferred `Box` drops invariant-only fields (`max_y`, `min_z`) while `satisfies_invariants()` references them (E0609); `Float64` params render as `i64` on the default agentic path. (Verified: `telos build` → cargo error 101.)
- [ ] Confirm generated implementation has no `TANGENT_THRESHOLD`-style magnitude shortcut — only an exact-zero direction check gates the parallel/containment branch (per Appendix A) — pending a compiling generated implementation
- [ ] Confirm each realization-layer `ensures` clause independently re-binds `let ideal = ...` (no shared-scope assumption across clauses) — moot until `==>`/symbolic bindings land

### 1.3 Realization layer / ε auto-derivation

> **Phase 0 finding (tpt-telos v0.2.0, source-confirmed):** Float32/Float64 are
> parsed and codegen'd, but the IR is QF_LRA over integer/real atoms — there is
> **no float semantics in the IR** and no IEEE-754 rounding-error derivation.
> The verifier does have *interval arithmetic*, but only for bounding nonlinear
> **integer** products (per-function `used_interval_bounding` flag in the tool's
> own `telos-proof.json`); it is not a rounding-error bound over f64 operations.
> The realization-layer `ε` auto-derivation from Section 5.1 therefore does
> **not** exist in v0.2.0 — it must be filed as upstream `tpt-telos` work (or
> implemented as our own layer) before Phase 1 can close.  See below.

- [ ] Verify whether `tpt-telos` v0.2.0 actually implements auto-derivation of realization-layer `ε` from the ideal contract + operation graph (interval arithmetic over IEEE-754 rounding) — this is load-bearing for Section 5.1's claim and may not exist yet
  - [ ] **Confirmed missing in v0.2.0 (source review):** IR has no float semantics (QF_LRA over integer/real atoms); interval arithmetic exists only for nonlinear *integer* products; no IEEE-754 interval arithmetic or ε derivation. File upstream `tpt-telos` work (or implement as our own layer) before Phase 1 can close
- [ ] `ensures`: bounded numerical error vs. ideal result (`|f64 result − ideal result| <= EPSILON_T`)
- [ ] `ensures`: no false negatives within the bounded domain (`ideal.is_some() ⟹ result.is_some()`)
- [ ] Confirm `EPSILON_T` is published per-build in the Proof Certificate, not hand-asserted

### 1.4 Proof Certificate (`tpt-certus-proof`)

- [x] **Machine-readable manifest** — first draft implemented in `src/lib.rs`: `MANIFEST_VERSION` (v1), structured `SourceLocation` (file + validated 1-based line span, rejects inverted/zero spans), `RegulatoryObjective` enum (DO-178C Table A-5 requirements-verification / formal-methods, DO-330 tool-qualification, FDA design-controls) with stable serde identifiers, `composition_lemmas`, pinned `telos_version` field, and deterministic `to_json()`/`to_json_pretty()` export (field order = declaration order → byte-identical for identical content)
- [x] **Native `telos-proof.json` bridge** — `src/telos_manifest.rs` parses the compiler's own hash-sealed manifest (source SHA-256, per-function `verified`/`conclusions_checked`/`conclusions_passed`/`used_interval_bounding`) and `ProofCertificate::is_supported_by` cross-checks that every certificate entry maps to a natively-verified function (matching by trailing `::` segment; rejects any native manifest listing an unverified function). Empirically confirmed against real `telos build` output + `telos verify-manifest` round-trip.
- [ ] Implement per-build Proof Certificate assembly: ideal + realization contracts for every verified function reachable from the build — partially blocked: native-outcome capture works (1.4 bullet above); realization-layer contract capture blocked on ε-derivation (Phase 1.3) and codegen
- [x] Include composition lemmas used (n/a for Phase 1's single function, but wire the field) — `CertificateEntry::composition_lemmas` field wired and serialized
- [ ] Cross-reference manifest entries to DO-178C Table A-5 objective rows — enum identifiers in place; full row-level mapping with the DO-330 objective mapping reviewed in Phase 2
- [ ] Wire hard CI gate: any failed proof obligation fails the build; no partial/best-effort certificate is ever emitted — `is_complete()` hard-gate semantics defined (`!empty && all source spans valid`); `verify-manifest` native-integrity step added to CI for all buildable `.telos` (currently smoke contract; `ray_aabb.telos` rejoins once codegen is faithful)
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
- [ ] Confirm `tpt-telos`'s ε-auto-derivation is actually implemented (not just described in `tpt-telos`'s own docs/roadmap) before Phase 1 depends on it → see Phase 1.3, exit criterion for Phase 1 closure. **No.** v0.2.0 IR has no float semantics (QF_LRA over integer/real atoms); interval arithmetic exists only for nonlinear integer products (`used_interval_bounding`); no IEEE-754 ε derivation (see Phase 1.2/1.3 findings).
- [ ] Upstream `tpt-telos` work required before Phase 1 ideal contract can fully close: (a) verified return values / return-value post-conditions (`let`/`return` currently no-ops in IR body analysis) + `==>` implication and/or `ensures result == ...`; (b) compound (`&&`/`||`) `if` guards and mutating-`match` support; (c) contract- and type-faithful `telos build`/`transpile` codegen (currently drops invariant-only struct fields → non-compiling Rust for float-carrying contracts; demotes `Float64` params to `i64`); (d) IEEE-754 interval arithmetic + auto-derived `ε`.
- [ ] **Float codegen nuance (Step 5, empirically re-verified):** main `render_type` maps `Float64 → f64` (`codegen/src/lib.rs`), and FFI/eject paths reject floats outright (`codegen/src/ffi.rs`); but the default agentic `telos build`/`transpile` output renders `Float64` params as `i64` and drops invariant-only fields, so observed generated Rust for float contracts does not compile. Realization layer per spec §5.1 must treat the executing f64 artifact as our own, not rely on `telos build` output yet.
- [ ] Do not claim "regulatory submission ready" before Phase 3 completes; do not claim an established financial model-risk pathway before Phase 5 completes (per spec Section 7 closing note) → ongoing constraint.
