# FM-elimination scalability benchmark — results (Phase 1.5)

Evidence for spec Section 5.2 / Phase 1 exit criterion 2: proof time and memory
of `telos verify` against synthetic BVH-shaped functions at nesting depths
4, 8, 12 and 16.

Reproduce:

```
powershell -NoProfile -ExecutionPolicy Bypass -File crates/tpt-certus-spatial/telos/bench/run_fm_bench.ps1 -TimeoutSec 5400
```

## Setup

- Verifier: `telos 0.2.0` (the pinned CI version), default solver (FM).
- Workload: a module with three functions named in the spec — `insert`,
  `rebalance`, `split_node` — each with `d` sequential `if` guards (2^d branches
  per function). Each arm mutates `out` and keeps the invariant `out.lo <= out.hi`.
  Guards differ per function (`key >= out.lo + i`, `out.hi >= out.lo + i`,
  `key <= out.hi - i`). This is a synthetic stand-in for BVH mutation code, not a
  faithful BVH.
- Machine: Windows 11 development workstation, one run per depth.
- Memory: peak working set of the `telos` process, sampled every 50 ms. This can
  under-report very short peaks.
- Timeout: 5400 s per depth.

## Results — three functions (`insert` + `rebalance` + `split_node`)

| depth | branches / function | wall time (s) | peak memory (MiB) | result |
|------:|--------------------:|--------------:|------------------:|--------|
| 4     | 16                  | 0.140         | 6.2               | all constraints satisfied |
| 8     | 256                 | 1.930         | 17.9              | all constraints satisfied |
| 12    | 4096                | 269.4         | 137.4             | all constraints satisfied |
| 16    | 65536               | > 5400        | > 2040 (at timeout) | **TIMEOUT** — not completed |

Raw data: `fm_bench_results.csv`. The depth-16 run was killed by the harness at
5400 s; no verdict was produced.

## Results — `insert` alone (earlier run, superseded)

Recorded for comparison. This run used a single function per module and is
preserved in git history at commit `be1ee1c`.

| depth | branches | wall time (s) | peak memory (MiB) | result |
|------:|---------:|--------------:|------------------:|--------|
| 4     | 16       | 0.386         | 6.0               | pass |
| 8     | 256      | 0.495         | 8.9               | pass |
| 12    | 4096     | 18.426        | 71.7              | pass |
| 16    | 65536    | 838.876       | 1307.9            | pass |

## Reading

- Adding `rebalance` and `split_node` changed the cost substantially. At depth 12
  the three-function module takes about 15× the time of `insert` alone, not the
  ~3× that three independent functions would suggest. The guard shapes in the
  new functions appear to be much more expensive for the FM solver. We have not
  isolated which one.
- At depth 16 the verifier did not finish within 90 minutes on this machine.
  Single-function depth 16 completed in 14 minutes, so the three-function
  workload is not simply 3× that.
- The verifier is not at a usable scale for depth 16 on these shapes. This is the
  opposite of what the depth-16 insert-only result suggested.
- Depths 4–8 are dominated by process startup, so small-depth timings say little
  about scaling.

## Not yet covered

- Per-function timing: the module is verified as one unit, so cost cannot be
  attributed to `insert`, `rebalance` or `split_node` separately.
- The workload is synthetic. Results on real BVH contracts may differ.
- One run per depth, no variance estimate.
- The `--solver z3` path was not measured.

## Decision

This is evidence, not a verdict. It does not support a claim that FM elimination
scales to BVH-sized contracts at depth 16 with this verifier. Whether it supports
committing to "self-contained solver, no external SMT dependency" as a permanent
principle is a judgment for the project owner. The z3 comparison, and per-function
timing, would be the next measurements.

## Per-function timing at depth 12 (single-function modules)

Each function was verified in its own module (one run each, run sequentially,
same machine as above):

| function     | wall time (s) | result |
|--------------|--------------:|--------|
| insert       | 13            | pass   |
| split_node   | 28            | pass   |
| rebalance    | 475           | pass   |

`rebalance` dominates. Its guard `out.hi >= out.lo + i` is the expensive shape.
The three single-function times sum to about 516 s, but the combined
three-function module took 269 s at the same depth. The two measurements do not
agree, so cost is not simply additive across functions. Possible causes include
how `telos` batches functions within a module, or timing noise from a single
run each. This has not been investigated further.

## Reproducibility of the combined depth-12 module

Re-run twice on the same machine to check the earlier 269 s figure:

| run | wall time (s) | result |
|----:|--------------:|--------|
| 1   | 431           | pass   |
| 2   | 357           | pass   |
| earlier | 269       | pass   |

The 269 s run was low. The combined module sits at about 360–430 s, which is
close to the 516 s single-function sum. Most of the earlier gap was run-to-run
variance, not a real difference between one module and three.

## z3 solver comparison — not measured

- z3 4.x was installed via scoop (`z3` 5.3.1 package, `libz3.lib` present).
- telos 0.2.0 was built with `--features z3` into a separate install root.
  The build failed: `tpt-telos-verifier` 0.2.0's z3 backend does not compile
  against the locked `z3` crate 0.12.1 (missing operator and `_eq` impls on
  `z3::ast::Int`). This is an upstream incompatibility. The shipped
  `telos verify --solver z3` flag in the default build is silently accepted and
  is not a z3 measurement.
- Patching the registry crate to get it to build was not done, because that
  would measure a modified solver, not the shipped one. The comparison needs
  an upstream fix or a compatible `z3` crate version.
