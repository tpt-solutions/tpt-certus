# FM-elimination scalability benchmark (Phase 1.5, spec Section 5.2 / exit criterion 2).
#
# Generates a synthetic BVH module at each nesting depth.  The module holds the
# three construction/mutation functions the spec names -- `insert`,
# `rebalance` and `split_node` -- each with `d` sequential guards (2^d branches).
# Runs `telos verify` on the module and records wall-clock time, peak working
# set and the verifier's RESULT line.  Writes a CSV next to this script.
#
# Usage (from the repo root):
#   powershell -ExecutionPolicy Bypass -File crates/tpt-certus-spatial/telos/bench/run_fm_bench.ps1
#   ... -Depths '4,8' -TimeoutSec 900
#
# Peak memory is sampled by polling the child's PeakWorkingSet64 every 50 ms,
# so it is a lower bound on the true peak for very short runs.

param(
    # Comma-separated string, so `powershell -File` does not collapse "4,8" into 48.
    [string]$Depths = '4,8,12,16',
    [int]$TimeoutSec = 3600
)

$ErrorActionPreference = 'Stop'
$bench = Split-Path -Parent $MyInvocation.MyCommand.Path
$telos = (Get-Command telos).Source
$telosVersion = (& telos --version) -join ' '
$results = @()

# One function body: `depth` guards.  Each guard tests a different expression so
# the three functions have distinct branch structure, and every arm keeps the
# `lo <= hi` invariant.
function New-FunctionLines([string]$name, [string]$params, [string]$guardFormat, [int]$depth) {
    $lines = @(
        "    func $name($params)",
        "        requires out.lo <= out.hi",
        "        ensures out.lo <= out.hi",
        "    {"
    )
    for ($i = 1; $i -le $depth; $i++) {
        $guard = $guardFormat -f $i
        $lines += "        if $guard {"
        $lines += "            mutate state { out.lo = out.lo + 1; out.hi = out.hi + 1 }"
        $lines += "        } else {"
        $lines += "            mutate state { out.hi = out.hi + 1 }"
        $lines += "        }"
    }
    $lines += "    }"
    return , $lines
}

foreach ($depth in ($Depths -split ',' | ForEach-Object { [int]$_.Trim() })) {
    $file = Join-Path $bench ("bvh_d{0}.telos" -f $depth)

    $lines = @(
        "module BvhBench$depth {",
        "    struct Node {",
        "        lo: Int,",
        "        hi: Int,",
        "        flag: Int,",
        "    }",
        ""
    )
    $lines += New-FunctionLines 'insert' 'key: Int, mut out: Node' 'key >= out.lo + {0}' $depth
    $lines += ""
    $lines += New-FunctionLines 'rebalance' 'mut out: Node' 'out.hi >= out.lo + {0}' $depth
    $lines += ""
    $lines += New-FunctionLines 'split_node' 'key: Int, mut out: Node' 'key <= out.hi - {0}' $depth
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
        functions    = 3
        branches_per_function = [math]::Pow(2, $depth)
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
