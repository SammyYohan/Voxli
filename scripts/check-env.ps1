# Vérification de l'environnement de développement Windows pour Voxli.
# Affiche un tableau de présence/absence pour chaque outil documenté dans
# docs/03 §2 (prérequis de construction).
#
# Exécuter : powershell -ExecutionPolicy Bypass -File scripts/check-env.ps1

$ErrorActionPreference = 'SilentlyContinue'

function Test-Tool {
    param([string] $Name, [string[]] $Candidates, [scriptblock] $Version)

    $found = $null
    foreach ($c in $Candidates) {
        $cmd = Get-Command $c -ErrorAction SilentlyContinue
        if (-not $cmd -and $c -eq 'cargo' -and (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) {
            $found = "$env:USERPROFILE\.cargo\bin\cargo.exe"
            break
        }
        if ($cmd) { $found = $cmd.Source; break }
    }
    if ($found) {
        $v = & $Version $found
        Write-Output ("  OK   {0,-10} {1}" -f $Name, $v)
    } else {
        Write-Output ("  MANQUANT {0}" -f $Name)
    }
}

Test-Tool 'rustc' @('rustc') { param($p) & $p --version }
Test-Tool 'cargo' @('cargo') { param($p) & $p --version }
Test-Tool 'node' @('node', 'node.exe') { param($p) & $p --version }
Test-Tool 'pnpm' @('pnpm.cmd', 'pnpm') { param($p) & $p --version }
Test-Tool 'cmake' @('cmake') { param($p) (& $p --version)[0] }

# Visual Studio Build Tools + SDK Windows (détection simple)
$vs = Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\Setup\SharedInstallation' -ErrorAction SilentlyContinue
if (-not $vs -or -not $vs.InstallationVersion) {
    $vs17 = Get-Content "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -ErrorAction SilentlyContinue
}
if (Test-Path "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe") {
    $inst = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -property installationVersion
    if ($inst) { Write-Output ("  OK   visual-studio {0}" -f $inst) } else { Write-Output '  MANQUANT visual-studio' }
} else {
    Write-Output '  MANQUANT visual-studio (vswhere absent)'
}

# SDK Windows : via les répertoires connus
$sdkRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\Include"
$sdk = Get-ChildItem $sdkRoot -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
if ($sdk) { Write-Output ("  OK   windows-sdk   {0}" -f $sdk.Name) } else { Write-Output '  MANQUANT windows-sdk' }

# Modèle ASR local (facultatif : la démo tourne avec le moteur factice)
$models = Join-Path $env:APPDATA 'Voxli\models'
$bin = Get-ChildItem $models -Filter 'ggml-*.bin' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($bin) { Write-Output ("  OK   modele-whisper {0}" -f $bin.Name) } else { Write-Output '  (info) aucun modele ggml dans %APPDATA%\Voxli\models — run scripts/fetch-model.ps1 pour le moteur reel' }

Write-Output ''
Write-Output 'Note : les modules purs (Voxli-core, tests) compilent sans microphone ni modele.'
