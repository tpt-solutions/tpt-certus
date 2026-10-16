# FM-elimination scalability benchmark (Phase 1.5, spec Section 5.2 / exit criterion 2).
#
# Generates a synthetic BVH `insert` function at each nesting depth, runs
# `telos verify` on it, and records wall-clock time, peak working set and the
# verifier's RESULT line.  Writes a CSV next to this script.
#
# Usage (from the repo root):
#   powershell -ExecutionPolicy Bypass -File crates/tpt-certus-spatial/telos/bench/run_fm_bench.ps1
#   ... -Depths 4,8 -TimeoutSec 900
#
# Peak memory is sampled by polling the child's PeakWorkingSet64 every 50 ms,
# so it is a lower bound on the true peak for very short runs.

param(
    [int[]]$Depths = @(4, 8, 12, 16),
    [int]$TimeoutSec = 1800
)

$ErrorActionPreference = 'Stop'
$bench = Split-Path -Parent $MyInvocation.MyCommand.Path
$telos = (Get-Command telos).Source
$telosVersion = (& telos --version) -join ' '
$results = @()

foreach ($depth in $Depths) {
    $file = Join-Path $bench ("bvh_d{0}.telos" -f $depth)

    $lines = @(
        "module BvhBench$depth {",
        "    struct Node {",
        "        lo: Int,",
        "        hi: Int,",
        "        flag: Int,",
        "    }",
        "",
        "    func insert(key: Int, mut out: Node)",
        "        requires out.lo <= out.hi",
        "        ensures out.lo <= out.hi",
        "    {"
    )
    for ($i = 1; $i -le $depth; $i++) {
        $lines += "        if key >= out.lo + $i {"
        $lines += "            mutate state { out.lo = out.lo + 1; out.hi = out.hi + 1 }"
        $lines += "        } else {"
        $lines += "            mutate state { out.hi = out.hi + 1 }"
        $lines += "        }"
    }
    $lines += "    }"
    $lines += "}"
    Set-Content -Path $file -Value $lines -Encoding ascii

    $stdout = Join-Path $bench ("bvh_d{0}.stdout.txt" -f $depth)
    $stderr = Join-Path $bench ("bvh_d{0}.stderr.txt" -f $depth)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $telos -ArgumentList @('verify', $file) `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr `
        -NoNewWindow -PassThru

    $peak = [long]0
    $status = 'UNKNOWN'
    while (-not $p.HasExited) {
        try {
            $p.Refresh()
            if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 }
        } catch { }
        if ($sw.Elapsed.TotalSeconds -gt $TimeoutSec) {
            $p.Kill()
            $status = 'TIMEOUT'
            break
        }
        Start-Sleep -Milliseconds 50
    }
    $p.WaitForExit()
    $sw.Stop()

    if ($status -ne 'TIMEOUT') {
        $resultLine = Select-String -Path $stdout -Pattern '^RESULT:' | Select-Object -Last 1
        if ($resultLine) {
            $status = ($resultLine.Line -replace '^RESULT:\s*', '')
        } else {
            $status = "NO RESULT (exit $($p.ExitCode))"
        }
    }

    $results += [pscustomobject]@{
        depth        = $depth
        branches     = [math]::Pow(2, $depth)
        seconds      = [math]::Round($sw.Elapsed.TotalSeconds, 3)
        peak_mib     = [math]::Round($peak / 1MB, 1)
        result       = $status
        telos        = $telosVersion
    }
    Write-Host ("depth {0,2}: {1,10} s  peak {2,8} MiB  {3}" -f $depth, $sw.Elapsed.TotalSeconds.ToString('F3'), [math]::Round($peak / 1MB, 1), $status)
}

$csv = Join-Path $bench 'fm_bench_results.csv'
$results | Export-Csv -Path $csv -NoTypeInformation -Encoding ascii
Write-Host "wrote $csv"
