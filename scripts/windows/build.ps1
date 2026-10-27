<#
.SYNOPSIS
Builds the openCrate Windows installer on this PC.

.DESCRIPTION
Does on your own computer what the Windows job in .github/workflows/installers.yml does on GitHub. It checks
the tools the build needs, offers to install missing ones with winget (asking first), then builds an unsigned,
per-user NSIS installer and prints where it is. See BUILDING.md.

.PARAMETER Yes
Install missing tools without asking about each one.

.PARAMETER Test
Run the Rust tests before building, as the Windows CI job does.

.PARAMETER TargetDir
Cargo's build folder. Default C:\t: a short path keeps OpenSSL's build under Windows path limits.

.EXAMPLE
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\build.ps1
#>
[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$Test,
    [string]$TargetDir = 'C:\t'
)

# Kept to plain ASCII: Windows PowerShell 5.1 reads a script without a byte order mark as ANSI.
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
# CI uses this Perl too: the Perl that comes with Git lacks modules OpenSSL's configure step needs.
$perl = 'C:\Strawberry\perl\bin\perl.exe'
$missing = New-Object System.Collections.Generic.List[string]

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host "== $Text" -ForegroundColor Cyan
}

function Invoke-Native([string]$Name, [scriptblock]$Command) {
    # Programs print progress on stderr, so only the exit code says whether they failed, and PowerShell does
    # not stop on an exit code by itself.
    Write-Step $Name
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try { & $Command } catch { throw "$Name could not start: $($_.Exception.Message)" }
    if ($LASTEXITCODE -ne 0) { throw "$Name failed (exit code $LASTEXITCODE). The messages above say why." }
}

function Get-ProbeOutput([scriptblock]$Command) {
    # Runs a quick check and returns what it printed, or '' if it could not run.
    $ErrorActionPreference = 'Continue'
    try { return ((& $Command 2>$null) -join "`n").Trim() } catch { return '' }
}

function Update-SessionPath {
    # Installers change PATH for new terminals only; pick up their additions in this one too.
    $known = @($env:Path -split ';')
    $saved = @(
        [Environment]::GetEnvironmentVariable('Path', 'Machine'),
        [Environment]::GetEnvironmentVariable('Path', 'User')
    ) -join ';'
    $cargoHome = $env:CARGO_HOME
    if (-not $cargoHome) { $cargoHome = Join-Path $env:USERPROFILE '.cargo' }
    foreach ($dir in (@($saved -split ';') + (Join-Path $cargoHome 'bin'))) {
        if ($dir -and ($known -notcontains $dir) -and (Test-Path $dir)) {
            $env:Path = "$env:Path;$dir"
            $known += $dir
        }
    }
}

function Find-NodePackage([int]$Major) {
    # winget has no fixed id for "Node 24": the LTS package moves to each new LTS line, and a per-version
    # package appears only for older lines. Take the first id whose current version is the one needed.
    foreach ($id in @("OpenJS.NodeJS.$Major", 'OpenJS.NodeJS.LTS', 'OpenJS.NodeJS')) {
        $info = Get-ProbeOutput { winget show --id $id --exact --source winget --accept-source-agreements }
        if ($info -match "(?m)^\s*Version:\s*$Major\.") { return $id }
    }
    return $null
}

function Resolve-Requirement {
    # Checks one thing the build needs. When it is missing, offers to install it with winget, then checks again.
    param(
        [string]$Name,
        [scriptblock]$Check,        # returns nothing when all is well, otherwise what is wrong
        [object]$WingetId,          # a package id, or a script block that finds one; $null when winget cannot help
        [string[]]$WingetArgs = @(),
        [string]$Note = '',         # shown before asking
        [string]$Manual             # what to do by hand
    )
    $problem = & $Check
    if (-not $problem) {
        Write-Host "  ok       $Name"
        return
    }
    Write-Host "  missing  $Name. $problem" -ForegroundColor Yellow
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Host "           winget is not available. $Manual"
        $missing.Add($Name)
        return
    }
    if ($WingetId -is [scriptblock]) { $WingetId = & $WingetId }
    if (-not $WingetId) {
        Write-Host "           $Manual"
        $missing.Add($Name)
        return
    }
    if ($Note) { Write-Host "           $Note" }
    $install = $Yes
    if (-not $install) {
        $answer = Read-Host "           Install $Name with winget ($WingetId) now? [y/N]"
        $install = $answer -match '^\s*y(es)?\s*$'
    }
    if (-not $install) {
        Write-Host "           Not installed. $Manual"
        $missing.Add($Name)
        return
    }

    $arguments = @('install', '--id', $WingetId, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements') + $WingetArgs
    $ErrorActionPreference = 'Continue'
    & winget @arguments
    if ($LASTEXITCODE -ne 0) {
        throw ("winget could not install $Name (exit code $LASTEXITCODE). If it asked for a restart, restart " +
            "Windows and run this script again. Otherwise: $Manual")
    }
    $ErrorActionPreference = 'Stop'

    Update-SessionPath
    $problem = & $Check
    if (-not $problem) {
        Write-Host "  ok       $Name (installed)" -ForegroundColor Green
        return
    }
    Write-Host ("           $Name was installed, but this terminal does not see it yet: $problem " +
        'Open a new terminal and run this script again.') -ForegroundColor Yellow
    $missing.Add($Name)
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw 'This script builds the Windows installer and runs on Windows. On a Mac, use scripts/mac/build.sh.'
    }
    $started = Get-Date
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath (Join-Path $repo 'package.json'))) {
        throw "Expected the openCrate repository at $repo (two folders above this script)."
    }
    # Absolute, because the Tauri CLI runs cargo from src-tauri and a relative folder would land there.
    $TargetDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($TargetDir)

    $nvmrc = (Get-Content -LiteralPath (Join-Path $repo '.nvmrc') -TotalCount 1).Trim()
    if ($nvmrc -notmatch '^v?(\d+)') { throw ".nvmrc holds '$nvmrc'; expected a Node version such as 24." }
    $nodeMajor = [int]$Matches[1]

    $toolchainFile = Join-Path $repo 'rust-toolchain.toml'
    $channelLine = Select-String -LiteralPath $toolchainFile -Pattern '^channel\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $channelLine) { throw "No channel found in $toolchainFile." }
    $channel = $channelLine.Matches[0].Groups[1].Value

    Write-Step 'Checking what the build needs'
    Write-Host '  Installs through winget may show a Windows administrator prompt.'

    Resolve-Requirement -Name 'Git' -WingetId 'Git.Git' `
        -Check { if (-not (Get-Command git -ErrorAction SilentlyContinue)) { 'Git is not installed.' } } `
        -Manual 'Install Git from https://git-scm.com/download/win.'

    Resolve-Requirement -Name "Node.js $nodeMajor" -WingetId { Find-NodePackage $nodeMajor } `
        -Check {
            if (-not (Get-Command node -ErrorAction SilentlyContinue)) { return 'Node.js is not installed.' }
            $version = Get-ProbeOutput { node --version }
            if ($version -notmatch '^v(\d+)\.') { return "node --version printed '$version'." }
            if ([int]$Matches[1] -ne $nodeMajor) {
                return "Node $version is installed; openCrate builds with Node $nodeMajor (.nvmrc)."
            }
            if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) { return 'npm is missing.' }
        } `
        -Manual ("Install Node.js $nodeMajor from https://nodejs.org. If another version is installed, " +
            'uninstall it first (Settings, Apps), or switch to it with your Node version manager.')

    # Before Rust: rustup's installer looks for these tools and complains if they are missing.
    Resolve-Requirement -Name 'Visual Studio 2022 Build Tools (C++)' `
        -WingetId {
            # winget cannot add a workload to a Build Tools 2022 that is already installed.
            if (Test-Path -LiteralPath $vswhere) {
                $existing = Get-ProbeOutput {
                    & $vswhere -products Microsoft.VisualStudio.Product.BuildTools -version '[17.0,18.0)' -property installationPath
                }
                if ($existing) { return $null }
            }
            'Microsoft.VisualStudio.2022.BuildTools'
        } `
        -WingetArgs @('--override',
            '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended') `
        -Check {
            if (-not (Test-Path -LiteralPath $vswhere)) { return 'No Visual Studio or Build Tools found.' }
            $vc = Get-ProbeOutput {
                & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            }
            if (-not $vc) { return 'Visual Studio or Build Tools is installed, but without the C++ build tools.' }
            $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib\*\um\x64\kernel32.lib'
            if (-not (Test-Path $sdk)) { return 'The C++ build tools are installed, but the Windows SDK is not.' }
        } `
        -Note 'Needs administrator rights and several GB of disk; the install can take a while.' `
        -Manual ('Install Visual Studio 2022 Build Tools from https://visualstudio.microsoft.com/visual-cpp-build-tools/ ' +
            'with the "Desktop development with C++" workload, which includes the Windows SDK. If Build Tools is ' +
            'already installed, open Visual Studio Installer, choose Modify and tick that workload.')

    Resolve-Requirement -Name 'Strawberry Perl' -WingetId 'StrawberryPerl.StrawberryPerl' `
        -Check { if (-not (Test-Path -LiteralPath $perl)) { "Not found at $perl." } } `
        -Manual 'Install Strawberry Perl from https://strawberryperl.com into C:\Strawberry (its default folder).'

    Resolve-Requirement -Name 'Rust (rustup)' -WingetId 'Rustlang.Rustup' `
        -Check {
            if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { return 'rustup is not installed.' }
            if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { return 'cargo is not on PATH.' }
        } `
        -Manual 'Install rustup from https://rustup.rs and keep its default (MSVC) setup.'

    if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
        Write-Host ('  optional ffmpeg is not installed. The build does not need it; openCrate uses it to convert ' +
            'formats on import, and the tests that need it are skipped. To add it: winget install ffmpeg')
    }
    if ($missing.Count -gt 0) {
        throw "Missing: $($missing -join ', '). Install what is listed above, then run this script again."
    }

    $savedEnv = @{}
    foreach ($name in @('OPENSSL_SRC_PERL', 'CARGO_TARGET_DIR')) {
        $savedEnv[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    Push-Location -LiteralPath $repo
    try {
        # Set for this build only; both are put back when the script ends.
        $env:OPENSSL_SRC_PERL = $perl
        $env:CARGO_TARGET_DIR = $TargetDir
        Write-Host ''
        Write-Host "Repository:   $repo"
        Write-Host "Build folder: $TargetDir"

        # Installed by name: "rustup show", which CI runs, no longer installs a missing toolchain in recent rustup.
        Invoke-Native "Installing Rust $channel (rust-toolchain.toml)" {
            # The components are quoted: unquoted, PowerShell would read the comma as a list and split the argument.
            rustup toolchain install $channel --profile minimal --component 'clippy,rustfmt'
        }
        # npm.cmd, not npm: the npm.ps1 shim needs script execution allowed, and it drops the "--" used below.
        Invoke-Native 'Installing packages (npm ci)' { npm.cmd ci }
        if ($Test) {
            # The app embeds the built frontend, so it must exist before the Rust crates compile.
            Invoke-Native 'Building the frontend' { npm.cmd run build }
            # As CI runs them. Dependencies unoptimised compile much faster, and the single quotes in the key are
            # TOML: Windows PowerShell would strip the double quotes CI uses.
            Invoke-Native 'Running the Rust tests' {
                cargo test --workspace --locked --no-fail-fast --config "profile.dev.package.'*'.opt-level=0"
            }
        }
        Invoke-Native 'Building the installer' { npm.cmd run tauri -- build --bundles nsis }
    } finally {
        Pop-Location
        foreach ($name in $savedEnv.Keys) {
            [Environment]::SetEnvironmentVariable($name, $savedEnv[$name], 'Process')
        }
    }

    $bundleDir = Join-Path $TargetDir 'release\bundle\nsis'
    $installer = Get-ChildItem -LiteralPath $bundleDir -Filter '*.exe' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $installer) { throw "The build finished, but no installer was found in $bundleDir." }

    Write-Step ('Done after {0:N0} min' -f ((Get-Date) - $started).TotalMinutes)
    Write-Host "Installer: $($installer.FullName)" -ForegroundColor Green
    Write-Host 'It installs for your user only and needs no administrator rights. It is unsigned, so Windows'
    Write-Host 'SmartScreen warns when you open it: choose More info, then Run anyway.'
} catch {
    Write-Host ''
    Write-Host "Stopped: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
