#!/usr/bin/env pwsh
# Regenerates praxis/shadow/shadow_<name>.px from pares-umbra/data/<name>.px
#
# Each generated file = a loadable, manual-trigger pares-radix procedure (shadow_<name>)
# + the verbatim evolved source embedded as //-prefixed provenance.
#
# WHY //-prefixed (not /* */): pluresdb-px's grammar supports only `#` and `//` line
# comments; block comments are a hard parse error. WHY a wrapper proc (not the umbra body
# verbatim as the live body): umbra's brace-block dialect (`procedure x { facts {..} let .. }`)
# does not parse under pluresdb-px, so a verbatim body would fail to load. The wrapper routes
# through the umbra-backed `evaluate_shadow_classifier` action; numeric net eval stays in umbra
# (C-PLURES). See praxis/shadow/README.md.
#
# Usage:  pwsh ./praxis/shadow/_generate_shadow.ps1
$ErrorActionPreference = "Stop"
$umbra = Join-Path $PSScriptRoot "..\..\..\pares-umbra\data"
$out   = $PSScriptRoot
if (-not (Test-Path $umbra)) {
  # Fallback to the canonical checkout location.
  $umbra = "C:\Projects\pares-umbra\data"
}
New-Item -ItemType Directory -Force -Path $out | Out-Null

$specs = @(
  @{ name="route_message";   acc="Accuracy: 97.5%";           desc="Umbra-evolved router candidate (97.5%). Shadow-only; never serves live output." },
  @{ name="score_priority";  acc="Fitness: 0.2960 (inv-MSE)"; desc="Umbra-evolved priority scorer (inv-MSE 0.2960). Shadow-only; never serves live output." },
  @{ name="classify_intent"; acc="Accuracy: 95.8%";           desc="Umbra-evolved intent classifier (95.8%). Shadow-only; never serves live output." }
)

$sep = "// =============================================================================================="

foreach ($s in $specs) {
  $name = $s.name
  $src  = Get-Content (Join-Path $umbra "$name.px") -Raw
  # //-prefix every line of the evolved source for the provenance block.
  $provenance = ($src -split "`r?`n" | ForEach-Object { if ($_ -eq "") { "//" } else { "// $_" } }) -join "`n"

  $sb = New-Object System.Text.StringBuilder
  [void]$sb.AppendLine("// Evolved by pares-umbra")
  [void]$sb.AppendLine("// $($s.acc)")
  [void]$sb.AppendLine("//")
  [void]$sb.AppendLine("// SHADOW CANDIDATE - deployed INERT (trigger: manual) so it ships to praxisbot via the normal")
  [void]$sb.AppendLine("// praxis/ sync but NEVER serves live traffic. Loaded into the shadow holder (ShadowProcedures)")
  [void]$sb.AppendLine("// at startup, separate from the live ReactiveRegistry. Numeric evaluation of the evolved network")
  [void]$sb.AppendLine("// lives in pares-umbra (the arena), per C-PLURES - pares-radix only LOADS this candidate and")
  [void]$sb.AppendLine("// (later) evaluates/promotes it once it consistently beats the live classifier.")
  [void]$sb.AppendLine("// See praxis/shadow/README.md.")
  [void]$sb.AppendLine("//")
  [void]$sb.AppendLine("// NOTE: pares-radix's .px engine (pluresdb-px) does NOT parse umbra's brace-block dialect")
  [void]$sb.AppendLine("// (verified). The original evolved source is preserved verbatim in the EVOLVED-SOURCE block at")
  [void]$sb.AppendLine("// the bottom (each line //-prefixed) for provenance / re-evolution. The executable procedure")
  [void]$sb.AppendLine("// below is the loadable, manual-trigger pares-radix form; it routes through the umbra-backed")
  [void]$sb.AppendLine("// evaluate_shadow_classifier action carrying model id `"$name`".")
  [void]$sb.AppendLine("")
  [void]$sb.AppendLine("procedure shadow_${name}:")
  [void]$sb.AppendLine("  trigger: manual")
  [void]$sb.AppendLine("  given: `"$($s.desc)`"")
  [void]$sb.AppendLine("  extract_features {content: `$content} -> `$features")
  [void]$sb.AppendLine("  evaluate_shadow_classifier {model: `"$name`", features: `$features} -> `$result")
  [void]$sb.AppendLine("  emit {shadow: `"$name`", result: `$result}")
  [void]$sb.AppendLine("")
  [void]$sb.AppendLine($sep)
  [void]$sb.AppendLine("// EVOLVED-SOURCE (verbatim, //-prefixed, do not edit)")
  [void]$sb.AppendLine($sep)
  [void]$sb.AppendLine($provenance)
  [void]$sb.AppendLine($sep)

  $target = Join-Path $out "shadow_$name.px"
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($target, $sb.ToString(), $utf8NoBom)
  Write-Output "wrote $target ($((Get-Item $target).Length) bytes)"
}
