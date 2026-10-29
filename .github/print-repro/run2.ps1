# Round 2: save the Thai case, make one-change variants of its Type 3 fonts, print each normally.
param([string] $Cli, [string] $Dir, [string] $Printer)
$ErrorActionPreference = 'Continue'
$thai = "งาน Deck Slab type 1`n - งานวางแผ่นพื้น Viva Board หนา 20 มม. รวมยาแนวด้วยปูนทราย`n - งานเทคอนกรีต Deck Slab ด้วย Pump (รวมตกแต่งผิว, กรีดหน้าลาย และบ่มคอนกรีต)"
function Run-Steps([object[]] $steps, [string] $label) {
  $script = Join-Path $Dir "case.json"
  $steps | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $script
  $out = Join-Path $Dir "case.out"
  $p = Start-Process -FilePath $Cli -ArgumentList @('run', '--script', $script, '--root', $Dir) -NoNewWindow -PassThru -RedirectStandardOutput $out -RedirectStandardError "$out.err"
  $done = $p.WaitForExit(240000)
  if ($done) { $p.WaitForExit() }
  $text = (Get-Content -Raw $out -ErrorAction SilentlyContinue) + (Get-Content -Raw "$out.err" -ErrorAction SilentlyContinue)
  $spooled = $text -match 'spooled'
  $result = if (-not $done) { $p.Kill(); 'HUNG' } elseif ($spooled) { 'printed OK' } else { "DIED (exit '$($p.ExitCode)')" }
  $tail = ($text -replace "`r?`n", ' ').Trim()
  if ($tail.Length -gt 200) { $tail = $tail.Substring($tail.Length - 200) }
  Write-Output "::warning title=$label::$result :: $tail"
}
Run-Steps @(
  @{ tool = 'doc_open'; args = @{ path = 'base.pdf' } },
  @{ tool = 'page_add_text'; args = @{ doc = 1; page = 1; text = $thai; at = @(60, 700); width = 480; size = 11; bold = $true; background = '#FFFFFF' } },
  @{ tool = 'doc_save'; args = @{ doc = 1; path = 'thai.pdf' } }
) 'save thai.pdf'
python -m pip install --quiet pikepdf
python .github/print-repro/variants.py (Join-Path $Dir 'thai.pdf') $Dir
foreach ($name in @('thai', 'v-resources', 'v-no-descriptor', 'v-empty-glyphs-d0', 'v-bbox', 'v-no-tounicode', 'v-no-span', 'v-integer-widths', 'v-everything', 'v-textbook-type3')) {
  Run-Steps @(
    @{ tool = 'doc_open'; args = @{ path = "$name.pdf" } },
    @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }
  ) "print $name"
}
