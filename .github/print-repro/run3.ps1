# Round 3: with the fix, the cases that crashed print normally, and a foreign Type 3 font falls back
# to printing as pictures. At most 10 annotations per step, so one line per case.
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
  $result = if (-not $done) { $p.Kill(); 'HUNG' } elseif ($text -match 'spooled') { 'printed OK' } elseif ($text -match 'saved|"bytes"') { 'saved OK' } else { "DIED (exit '$($p.ExitCode)')" }
  $bytes = if ($text -match '"bytes":\s*(\d+)') { " ($($Matches[1]) bytes)" } else { '' }
  Write-Output "::warning title=$label::$result$bytes"
}
$add = @{ doc = 1; page = 1; text = $thai; at = @(60, 700); width = 480; size = 11; bold = $true; background = '#FFFFFF' }
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'base.pdf' } }, @{ tool = 'page_add_text'; args = $add }, @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }) 'fixed: thai bold background'
$list = $add.Clone(); $list.list = 'thai-decimal'
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'base.pdf' } }, @{ tool = 'page_add_text'; args = $list }, @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }) 'fixed: thai numbered list'
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'base.pdf' } }, @{ tool = 'page_add_text'; args = $add }, @{ tool = 'doc_save'; args = @{ doc = 1; path = 'thai.pdf' } }) 'save thai.pdf'
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'thai.pdf' } }, @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }) 'fixed: saved thai file reopened'
python -m pip install --quiet pikepdf
python .github/print-repro/variants.py (Join-Path $Dir 'thai.pdf') $Dir
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'v-textbook-type3.pdf' } }, @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }) 'fixed: textbook Type 3 (falls back to pictures)'
Run-Steps @(@{ tool = 'doc_open'; args = @{ path = 'base.pdf' } }, @{ tool = 'doc_print'; args = @{ doc = 1; printer = $Printer } }) 'fixed: scan only'
