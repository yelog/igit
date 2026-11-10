$ErrorActionPreference = 'Stop'
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $sourceRoot 'pages/install.ps1') -LibraryOnly

function Assert-True($Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Assert-Fails([scriptblock]$Action, [string]$Message) {
    try { & $Action } catch { return }
    throw "Expected failure: $Message"
}

$native = $env:OS -eq 'Windows_NT'
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('igit-windows-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temporary) | Out-Null
$previousEncoding = [Console]::OutputEncoding
[Console]::OutputEncoding = $IgitUtf8

# On a non-Windows host, exercise filesystem/manifest transactions with inert
# fixtures. CI runs the same cases with actual Windows executables and launchers.
if (-not $native) {
    function Assert-IgitBinaries([string]$Directory, [string]$ReleaseVersion) {
        foreach ($name in @('igit', 'ig')) {
            if ([IO.File]::ReadAllText((Join-Path $Directory "$name.exe")) -cne $ReleaseVersion) { throw 'Staged executable version mismatch.' }
        }
    }
}

function New-TestBinary([string]$Version) {
    $binary = Join-Path $temporary ("fixture-$Version.exe")
    if (Test-Path -LiteralPath $binary) { return $binary }
    if ($native) {
        $rust = Join-Path $temporary 'fixture.rs'
        $code = 'fn main() { let args: Vec<String> = std::env::args().skip(1).collect(); if args == ["--version"] { println!("IGit VERSION \u{2014} terminal Git workbench"); } else { println!("{}", args.join("|")); } }'.Replace('VERSION', $Version)
        [IO.File]::WriteAllText($rust, $code, $IgitUtf8)
        & rustc --edition=2021 $rust -o $binary
        Assert-True ($LASTEXITCODE -eq 0) 'Native Windows fixture must compile.'
    } else { [IO.File]::WriteAllText($binary, $Version, $IgitUtf8) }
    return $binary
}

function New-TestArchive([string]$Version, [switch]$BadBinary, [switch]$Unsafe) {
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $archive = Join-Path $temporary ([Guid]::NewGuid().ToString('N') + '.zip')
    $bundle = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in @('igit', 'ig')) {
            $reported = if ($BadBinary -and $name -eq 'ig') { '0.0.0' } else { $Version }
            $binary = New-TestBinary $reported
            $entry = $bundle.CreateEntry("igit_$($Version)_$IgitTarget/$name.exe")
            $stream = $entry.Open()
            try {
                $bytes = [IO.File]::ReadAllBytes($binary)
                $stream.Write($bytes, 0, $bytes.Length)
            } finally { $stream.Dispose() }
        }
        if ($Unsafe) { $null = $bundle.CreateEntry('../escape') }
    } finally { $bundle.Dispose() }
    return $archive
}

function Install-TestArchive([string]$Root, [string]$Bin, [string]$Version, [switch]$BadBinary, [switch]$Unsafe) {
    $archive = New-TestArchive $Version -BadBinary:$BadBinary -Unsafe:$Unsafe
    $digest = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Install-IgitRelease $Root $Bin $archive $Version $digest
}

function Get-TestSnapshot([string]$Root) {
    $selection = [IO.File]::ReadAllText((Join-Path $Root 'current.txt')).Trim()
    $release = Join-Path (Join-Path $Root 'releases') $selection
    return @(
        $selection,
        [IO.File]::ReadAllText((Join-Path $Root 'install.json')),
        (Get-FileHash -LiteralPath (Join-Path $release 'igit.exe') -Algorithm SHA256).Hash,
        (Get-FileHash -LiteralPath (Join-Path $release 'ig.exe') -Algorithm SHA256).Hash
    ) -join '|'
}

try {
    Assert-True ((Get-IgitChannel '0.1.0-beta.1') -eq 'beta') 'Beta channel must stay distinct.'
    Assert-True ((Compare-IgitVersion '0.1.0-beta.9' '0.1.0-beta.10') -eq -1) 'Beta counters compare numerically.'
    Assert-True ((Compare-IgitVersion '0.1.0' '0.1.0-beta.10') -eq 1) 'Stable promotion follows its betas.'
    Assert-Fails { Get-IgitVersionParts '0.1.0-beta.0' } 'Malformed versions are rejected.'
    foreach ($name in @('../escape', '/absolute', 'folder\escape', 'folder/../escape', 'file:stream', 'NUL', 'tail.')) {
        Assert-Fails { Assert-IgitRelativeFile $name } "Unsafe filename $name"
    }
    $assets = @{}
    foreach ($target in $IgitTargets) {
        $extension = if ($target -eq $IgitTarget) { '.zip' } else { '.tar.gz' }
        $assets[$target] = @{ url = "https://github.com/yelog/igit/releases/download/v0.1.0-beta.1/igit_0.1.0-beta.1_$target$extension"; sha256 = ('a' * 64) }
    }
    $manifest = @{ schema = 1; product = 'igit'; channel = 'beta'; version = '0.1.0-beta.1'; tag = 'v0.1.0-beta.1'; prerelease = $true; assets = $assets } | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    Assert-IgitManifest $manifest 'beta' 'https://igit.yelog.org/channels'
    Assert-Fails { Assert-IgitManifest $manifest 'stable' 'https://igit.yelog.org/channels' } 'Channels cannot silently switch.'
    $manifest.assets.$IgitTarget.url = 'https://example.com/foreign.zip'
    Assert-Fails { Assert-IgitManifest $manifest 'beta' 'https://igit.yelog.org/channels' } 'Foreign assets cannot be installed.'

    $unicode = [string][char]0x6D4B + [char]0x8BD5
    $root = Join-Path $temporary "data [literal] $unicode"
    $bin = Join-Path $temporary "bin [literal] $unicode"
    Install-TestArchive $root $bin '0.1.0-beta.1'
    $state = Get-IgitInstallation $root $bin
    Assert-True ($state.version -eq '0.1.0-beta.1') 'First version is active.'
    $before = Get-TestSnapshot $root
    Assert-Fails { Install-TestArchive $root $bin '0.1.0-beta.2' -BadBinary } 'Both executables must report the selected version.'
    Assert-True ((Get-TestSnapshot $root) -ceq $before) 'A bad second executable preserves both old commands.'
    Assert-Fails { Install-TestArchive $root $bin '0.1.0-beta.2' -Unsafe } 'Traversal must be rejected before extraction.'
    Assert-True ((Get-TestSnapshot $root) -ceq $before) 'An unsafe ZIP preserves the active installation.'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root '.install.lock'))) 'Failure releases only its own lock.'

    Install-TestArchive $root $bin '0.1.0-beta.2'
    $state = Get-IgitInstallation $root $bin
    Assert-True ($state.version -eq '0.1.0-beta.2' -and $state.channel -eq 'beta') 'Update retains the selected channel.'
    Assert-True (Test-Path -LiteralPath (Join-Path $root "releases/0.1.0-beta.1_$IgitTarget")) 'Updates retain immutable old versions.'
    if ($native) {
        foreach ($name in @('igit', 'ig')) {
            $output = & (Join-Path $bin "$name.cmd") --version
            Assert-True ($LASTEXITCODE -eq 0 -and ($output | Out-String).Contains('0.1.0-beta.2')) 'Both launchers must select the active executable.'
            $output = & (Join-Path $bin "$name.cmd") 'space [literal]'
            Assert-True (($output | Out-String).Trim() -ceq 'space [literal]') 'Launchers preserve literal arguments.'
        }
    }
    $lockPath = Join-Path $root '.install.lock'
    [IO.File]::WriteAllText($lockPath, 'another installer')
    Assert-Fails { Install-TestArchive $root $bin '0.1.0-beta.3' } 'An existing lock cannot be stolen.'
    Assert-True ([IO.File]::ReadAllText($lockPath) -ceq 'another installer') 'The foreign lock is preserved.'
    [IO.File]::Delete($lockPath)
    [IO.File]::AppendAllText((Join-Path $bin 'ig.cmd'), 'modified')
    Assert-Fails { Install-TestArchive $root $bin '0.1.0-beta.3' } 'Modified launchers stop update.'

    $foreignRoot = Join-Path $temporary 'foreign-data'
    $foreignBin = Join-Path $temporary 'foreign-bin'
    [IO.Directory]::CreateDirectory($foreignBin) | Out-Null
    [IO.File]::WriteAllText((Join-Path $foreignBin 'ig.exe'), 'foreign command')
    Assert-Fails { Install-TestArchive $foreignRoot $foreignBin '0.1.0-beta.1' } 'Existing EXEs cannot be shadowed.'
    Assert-True ([IO.File]::ReadAllText((Join-Path $foreignBin 'ig.exe')) -ceq 'foreign command') 'Foreign commands stay untouched.'

    if ($native) {
        $manifest.assets.$IgitTarget.url = "https://github.com/yelog/igit/releases/download/v0.1.0-beta.1/igit_0.1.0-beta.1_$IgitTarget.zip"
        $inspectionManifest = $manifest | ConvertTo-Json -Depth 8
        $originalDownload = ${function:Invoke-IgitDownload}
        function Invoke-IgitDownload([string]$Url, [string]$Destination, [long]$Limit = 0) {
            Assert-True ($Url.EndsWith('/beta.json')) 'Unmanaged inspection retains the running release channel.'
            [IO.File]::WriteAllText($Destination, $inspectionManifest, $IgitUtf8)
        }
        $Unmanaged = $true
        $FromExecutable = ''
        # Clear the dot-sourced parameter and its ValidateSet for this no-channel case.
        Remove-Variable -Name Channel
        $Channel = ''
        $Version = ''
        $InstallRoot = Join-Path $temporary 'unmanaged-root'
        $InstallDir = Join-Path $temporary 'unmanaged-bin'
        $Check = $true
        try {
            foreach ($case in @(
                @('0.0.9-beta.1', 'manager_action_required'),
                @('0.1.0-beta.1', 'up_to_date'),
                @('0.2.0-beta.1', 'up_to_date')
            )) {
                $RunningVersion = $case[0]
                $report = Invoke-IgitInstall
                Assert-True ($report.status -eq $case[1]) 'Unmanaged inspection compares versions without installing.'
                Assert-True ($report.current_version -eq $RunningVersion -and $report.channel -eq 'beta') 'Report identifies the running version and channel.'
                Assert-True ($report.action.Contains('original package manager')) 'Unmanaged copies provide owner guidance.'
                Assert-True (-not (Test-Path -LiteralPath $InstallRoot) -and -not (Test-Path -LiteralPath $InstallDir)) 'Inspection creates no installation files.'
            }
            $Check = $false
            Assert-Fails { Invoke-IgitInstall } 'Unmanaged mode must never install.'
        } finally {
            ${function:Invoke-IgitDownload} = $originalDownload
            $Unmanaged = $false
        }
    }
    Write-Output "Windows installer contract tests passed (native executable checks: $native)."
} finally {
    [Console]::OutputEncoding = $previousEncoding
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
