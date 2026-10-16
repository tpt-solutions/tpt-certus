# FM-elimination scalability benchmark — results (Phase 1.5)

Evidence for spec Section 5.2 / Phase 1 exit criterion 2: proof time and memory
of `telos verify` against a synthetic BVH-shaped function at nesting depths
4, 8, 12 and 16.

Reproduce:

```
powershell -NoProfile -ExecutionPolicy Bypass -File crates/tpt-certus-spatial/telos/bench/run_fm_bench.ps1
```

## Setup

- Verifier: `telos 0.2.0` (the pinned CI version), default solver (FM).
- Workload: `insert(key: Int, mut out: Node)` with `d` sequential `if key >= out.lo + i`
  guards, each arm mutating `out`. The function has `2^d` verification branches.
  Both arms preserve the `requires`/`ensures` invariant `out.lo <= out.hi`.
  This is a synthetic stand-in for the BVH insert path, not a faithful BVH.
- Machine: Windows 11 development workstation (single run per depth).
- Memory: peak working set of the `telos` process, sampled every 50 ms. This can
  under-report very short peaks.

## Results

| depth | branches | wall time (s) | peak memory (MiB) | result |
|------:|---------:|--------------:|------------------:|--------|
| 4     | 16       | 0.386         | 6.0               | all constraints satisfied |
| 8     | 256      | 0.495         | 8.9               | all constraints satisfied |
| 12    | 4096     | 18.426        | 71.7              | all constraints satisfied |
| 16    | 65536    | 838.876       | 1307.9            | all constraints satisfied |

Raw data: `fm_bench_results.csv`. Depth 16 took about 14 minutes.

## Reading

- Every depth verified; no solver-limit failures were observed on this workload.
- Branch count grows 16× from depth 12 to 16, but wall time grows about 45× and
  peak memory about 18×. Cost is super-linear in branch count at these depths.
- Depths 4–8 are dominated by process startup, so the small-depth timings say
  little about scaling.
- A single depth-16 run takes 14 minutes. A build-time gate on this
  workload would need a time budget or caching; the numbers alone do not
  settle that.

## Not yet covered

- The spec names `insert`, `rebalance` and `split_node`. Only `insert` is built.
  `rebalance` and `split_node` still need `.telos` sources.
- The workload is synthetic. Results on real BVH contracts may differ.
- One run per depth, no variance estimate.

## Decision

This is evidence, not a verdict. Whether it supports committing to "self-contained
solver, no external SMT dependency" as a permanent principle is a judgment for
the project owner. The `--solver z3` path in `telos verify` was not measured here
and would be a useful comparison.
