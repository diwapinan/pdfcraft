# Prints synthetic documents through doc_print (the app's own print path) to a local printer
# whose port is a file, one case per process, and reports each case's outcome as an annotation.
param([string] $Cli, [string] $Dir, [string] $Printer)
$ErrorActionPreference = 'Continue'
$thai = "งาน Deck Slab type 1`n - งานวางแผ่นพื้น Viva Board หนา 20 มม. รวมยาแนวด้วยปูนทราย`n - งานเทคอนกรีต Deck Slab ด้วย Pump (รวมตกแต่งผิว, กรีดหน้าลาย และบ่มคอนกรีต)"
$latin = "Deck Slab type 1`n - Viva Board 20 mm with mortar joints`n - Deck Slab concrete by pump"
$cases = @(
  @{ name = 'base-only';             text = $null;  bold = $false; bg = $null },
  @{ name = 'latin';                 text = $latin; bold = $false; bg = $null },
  @{ name = 'latin-bold-background'; text = $latin; bold = $true;  bg = '#FFFFFF' },
  @{ name = 'thai';                  text = $thai;  bold = $false; bg = $null },
  @{ name = 'thai-bold-background';  text = $thai;  bold = $true;  bg = '#FFFFFF' }
)
foreach ($c in $cases) {
  foreach ($asImage in @($false, $true)) {
    $steps = @(@{ tool = 'doc_open'; args = @{ path = 'base.pdf' } })
    if ($c.text) {
      $add = @{ doc = 1; page = 1; text = $c.text; at = @(60, 700); width = 480; size = 11; bold = $c.bold }
      if ($c.bg) { $add.background = $c.bg }
      $steps += @{ tool = 'page_add_text'; args = $add }
    }
    $steps += @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer; as_image = $asImage } }
    $script = Join-Path $Dir "case.json"
    $steps | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $script
    $label = "$($c.name)$(if ($asImage) { ' (print as image)' } else { '' })"
    $out = Join-Path $Dir "case.out"
    $p = Start-Process -FilePath $Cli -ArgumentList @('run', '--script', $script, '--root', $Dir) -NoNewWindow -PassThru -RedirectStandardOutput $out -RedirectStandardError "$out.err"
    if (-not $p.WaitForExit(240000)) { $p.Kill(); $result = 'HUNG (killed after 4 min)' }
    else {
      $code = $p.ExitCode
      $hex = '0x{0:X8}' -f ([uint32]([int64]$code -band 0xFFFFFFFF))
      $tail = (Get-Content -Raw "$out.err" -ErrorAction SilentlyContinue) + (Get-Content -Raw $out -ErrorAction SilentlyContinue)
      $tail = ($tail -replace "`r?`n", ' ').Trim()
      if ($tail.Length -gt 300) { $tail = $tail.Substring($tail.Length - 300) }
      $result = if ($code -eq 0) { 'printed OK' } elseif ($code -lt 0 -or $code -gt 255) { "CRASHED exit $hex" } else { "failed exit $code" }
      $result = "$result :: $tail"
    }
    Write-Output "::warning title=print $label::$result"
  }
}
