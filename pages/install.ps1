[CmdletBinding()]
param(
    [ValidateSet('stable', 'beta')][string]$Channel = $env:IGIT_CHANNEL,
    [string]$Version = $env:IGIT_VERSION,
    [string]$InstallDir = $env:IGIT_INSTALL_DIR,
    [string]$InstallRoot = $env:IGIT_INSTALL_ROOT,
    [switch]$NoModifyPath,
    [switch]$Check,
    [switch]$Json,
    [switch]$AllowDowngrade,
    [string]$FromExecutable,
    [string]$RunningVersion,
    [switch]$Unmanaged,
    [string]$ProgressDir,
    [switch]$LibraryOnly
)

$ErrorActionPreference = 'Stop'
$IgitTarget = 'x86_64-pc-windows-msvc'
$IgitTargets = @('x86_64-apple-darwin', 'aarch64-apple-darwin', 'x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl', $IgitTarget)
$IgitVersionPattern = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-beta\.([1-9][0-9]*))?$'
$IgitUtf8 = New-Object Text.UTF8Encoding($false)

function Get-IgitVersionParts([string]$Value) {
    if ($Value -cnotmatch $IgitVersionPattern) { throw "Invalid IGit version: $Value" }
    return @([long]$Matches[1], [long]$Matches[2], [long]$Matches[3], [long](-not $Matches[4]), [long]$Matches[4])
}

function Compare-IgitVersion([string]$Left, [string]$Right) {
    $a = Get-IgitVersionParts $Left
    $b = Get-IgitVersionParts $Right
    for ($i = 0; $i -lt $a.Count; $i++) {
        if ($a[$i] -lt $b[$i]) { return -1 }
        if ($a[$i] -gt $b[$i]) { return 1 }
    }
    return 0
}

function Get-IgitChannel([string]$Value) {
    $null = Get-IgitVersionParts $Value
    if ($Value.Contains('-beta.')) { return 'beta' }
    return 'stable'
}

function Get-IgitFullPath([string]$Value) {
    $path = [IO.Path]::GetFullPath($Value)
    if ($path.StartsWith('\\?\') -and -not $path.StartsWith('\\?\UNC\')) { $path = $path.Substring(4) }
    if ($path.IndexOfAny([char[]]@([char]0, [char]10, [char]13)) -ge 0) { throw 'Installation paths cannot contain control characters.' }
    return $path.TrimEnd([IO.Path]::DirectorySeparatorChar)
}

function Test-IgitUrl([string]$Value) {
    $uri = $null
    if (-not [Uri]::TryCreate($Value, [UriKind]::Absolute, [ref]$uri)) { return $false }
    return (-not $uri.UserInfo -and -not $uri.Fragment -and ($uri.Scheme -eq 'https' -or ($uri.Scheme -eq 'http' -and $uri.IsLoopback)))
}

function Invoke-IgitDownload([string]$Url, [string]$Destination, [long]$Limit = 134217728, [string]$Headers = '') {
    if (-not (Test-IgitUrl $Url)) { throw 'Downloads require HTTPS (HTTP is allowed only on loopback).' }
    # curl.exe is part of the supported Windows runner/OS family. Bound both
    # redirects and transfer time, and never pass credentials to asset hosts.
    $protocols = if (([Uri]$Url).Scheme -eq 'http') { '=https,http' } else { '=https' }
    $arguments = @('--fail', '--silent', '--show-error', '--location', '--proto', $protocols, '--proto-redir', '=https', '--tlsv1.2', '--connect-timeout', '15', '--max-time', '120', '--retry', '2', '--max-filesize', $Limit, '--output', $Destination)
    if ($Headers) { $arguments += @('--dump-header', $Headers) }
    $arguments += $Url
    & curl.exe @arguments
    if ($LASTEXITCODE -ne 0) { throw "Download failed: $Url" }
    $file = Get-Item -LiteralPath $Destination
    if ($file.Length -le 0 -or $file.Length -gt $Limit) { throw 'Download is empty or exceeds its size limit.' }
}

function Assert-IgitManifest($Manifest, [string]$ExpectedChannel, [string]$BaseUrl) {
    if ($Manifest.schema -ne 1 -or $Manifest.product -cne 'igit' -or $Manifest.channel -cne $ExpectedChannel) { throw 'Channel manifest identity mismatch.' }
    $releaseVersion = [string]$Manifest.version
    if ((Get-IgitChannel $releaseVersion) -cne $ExpectedChannel -or $Manifest.tag -cne "v$releaseVersion" -or $Manifest.prerelease -isnot [bool] -or $Manifest.prerelease -ne ($ExpectedChannel -eq 'beta')) { throw 'Channel manifest version mismatch.' }
    $names = @($Manifest.assets.PSObject.Properties.Name | Sort-Object)
    if (@(Compare-Object $names @($IgitTargets | Sort-Object)).Count -ne 0) { throw 'Channel manifest target set mismatch.' }
    $base = [Uri]$BaseUrl
    $origin = if ($base.Scheme -eq 'http' -and $base.IsLoopback) { $base.GetLeftPart([UriPartial]::Authority) } else { 'https://github.com' }
    foreach ($name in $IgitTargets) {
        $asset = $Manifest.assets.$name
        $extension = if ($name -eq $IgitTarget) { '.zip' } else { '.tar.gz' }
        $expected = "$origin/yelog/igit/releases/download/v$releaseVersion/igit_$($releaseVersion)_$name$extension"
        if ($asset.url -cne $expected -or $asset.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw "Invalid release asset: $name" }
    }
}

function Test-IgitPlainFile([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    return ((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0
}

function Read-IgitJson([string]$Path) {
    if (-not (Test-IgitPlainFile $Path) -or (Get-Item -LiteralPath $Path -Force).Length -gt 1048576) { throw "Missing or invalid installation metadata: $Path" }
    return [IO.File]::ReadAllText($Path, $IgitUtf8) | ConvertFrom-Json
}

function Write-IgitAtomic([string]$Path, [string]$Text) {
    $temporary = Join-Path ([IO.Path]::GetDirectoryName($Path)) ('.write-' + [Guid]::NewGuid().ToString('N'))
    try {
        [IO.File]::WriteAllText($temporary, $Text, $IgitUtf8)
        if ([IO.File]::Exists($Path)) { [IO.File]::Replace($temporary, $Path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $Path) }
    } finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}

function Get-IgitLauncher([string]$Root, [string]$Name) {
    # Keep CMD source ASCII even when the user's profile/install path is Unicode.
    return "@echo off`r`nsetlocal DisableDelayedExpansion`r`npowershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File `"%~dp0.$Name-launch.ps1`" %*`r`nexit /b %ERRORLEVEL%`r`n"
}

function Get-IgitLauncherScript([string]$Root, [string]$Name) {
    $encoded = [Convert]::ToBase64String($IgitUtf8.GetBytes($Root))
    return @'
$ErrorActionPreference = 'Stop'
$previousEncoding = [Console]::OutputEncoding
try {
    [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
    $root = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('ROOT_BASE64'))
    $selection = [IO.File]::ReadAllText((Join-Path $root 'current.txt')).Trim()
    if ($selection -cnotmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-beta\.[0-9]+)?_x86_64-pc-windows-msvc$') { throw 'Invalid IGit release pointer.' }
    & (Join-Path (Join-Path (Join-Path $root 'releases') $selection) 'COMMAND.exe') @args
    exit $LASTEXITCODE
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally { [Console]::OutputEncoding = $previousEncoding }
'@.Replace('ROOT_BASE64', $encoded).Replace('COMMAND', $Name)
}

function Assert-IgitRelativeFile([string]$Name) {
    if (-not $Name -or $Name.Contains('\') -or $Name.Contains(':') -or $Name.StartsWith('/') -or $Name.IndexOf([char]0) -ge 0) { throw "Unsafe archive entry: $Name" }
    foreach ($part in $Name.Split('/')) {
        if (-not $part -or $part -in @('.', '..') -or $part.EndsWith('.') -or $part.EndsWith(' ') -or $part -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)') { throw "Unsafe archive entry: $Name" }
    }
}

function Expand-IgitArchive([string]$Archive, [string]$Destination, [string]$ReleaseVersion) {
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $bundle = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $prefix = "igit_$($ReleaseVersion)_$IgitTarget/"
        $names = New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        [long]$total = 0
        if ($bundle.Entries.Count -lt 2 -or $bundle.Entries.Count -gt 256) { throw 'Unexpected archive entry count.' }
        foreach ($entry in $bundle.Entries) {
            Assert-IgitRelativeFile $entry.FullName
            $mode = ($entry.ExternalAttributes -shr 16) -band 61440
            if (-not $entry.FullName.StartsWith($prefix, [StringComparison]::Ordinal) -or -not $names.Add($entry.FullName) -or $mode -notin @(0, 32768) -or -not $entry.Name) { throw 'Archive contains an unexpected or nonregular file.' }
            $total += $entry.Length
        }
        if ($total -gt 134217728 -or -not $names.Contains($prefix + 'igit.exe') -or -not $names.Contains($prefix + 'ig.exe')) { throw 'Archive must contain both executables within its size limit.' }
        foreach ($entry in $bundle.Entries) {
            $relative = $entry.FullName.Substring($prefix.Length)
            Assert-IgitRelativeFile $relative
            $destinationFile = Join-Path $Destination $relative
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destinationFile)) | Out-Null
            $inputStream = $entry.Open()
            try {
                $outputStream = [IO.File]::Open($destinationFile, [IO.FileMode]::CreateNew)
                try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() }
            } finally { $inputStream.Dispose() }
        }
    } finally { $bundle.Dispose() }
}

function Assert-IgitBinaries([string]$Directory, [string]$ReleaseVersion) {
    $expected = "IGit $ReleaseVersion " + [char]0x2014 + ' terminal Git workbench'
    foreach ($name in @('igit', 'ig')) {
        $process = New-Object Diagnostics.Process
        try {
            $process.StartInfo.FileName = Join-Path $Directory "$name.exe"
            $process.StartInfo.Arguments = '--version'
            $process.StartInfo.UseShellExecute = $false
            $process.StartInfo.CreateNoWindow = $true
            $process.StartInfo.RedirectStandardOutput = $true
            $process.StartInfo.RedirectStandardError = $true
            $process.StartInfo.StandardOutputEncoding = $IgitUtf8
            $null = $process.Start()
            $output = $process.StandardOutput.ReadToEndAsync()
            $errors = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(15000)) { $process.Kill(); throw 'Staged executable version check timed out.' }
            if ($process.ExitCode -ne 0 -or $output.GetAwaiter().GetResult().Trim() -cne $expected) { throw "Staged $name does not report version $ReleaseVersion" }
            $null = $errors.GetAwaiter().GetResult()
        } finally { $process.Dispose() }
    }
}

function Assert-IgitDirectory([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Installation directory is replaced or linked: $Path" }
    }
}

function Get-IgitReleaseRecord([string]$Directory, [string]$ReleaseVersion, [string]$Digest) {
    $files = @{}
    Get-ChildItem -LiteralPath $Directory -File -Recurse | ForEach-Object {
        $name = $_.FullName.Substring($Directory.Length + 1).Replace('\', '/')
        $files[$name] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return @{ product = 'igit'; version = $ReleaseVersion; target = $IgitTarget; archive_sha256 = $Digest; files = $files }
}

function Assert-IgitRelease([string]$Directory) {
    Assert-IgitDirectory $Directory
    $record = Read-IgitJson (Join-Path $Directory '.igit-release.json')
    $null = Get-IgitVersionParts $record.version
    if ($record.product -cne 'igit' -or $record.target -cne $IgitTarget -or [IO.Path]::GetFileName($Directory) -cne "$($record.version)_$IgitTarget" -or $record.archive_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Installed release identity mismatch.' }
    $names = @($record.files.PSObject.Properties.Name)
    if ('igit.exe' -cnotin $names -or 'ig.exe' -cnotin $names) { throw 'Installed release ownership is incomplete.' }
    foreach ($property in $record.files.PSObject.Properties) {
        Assert-IgitRelativeFile $property.Name
        $path = Join-Path $Directory $property.Name
        $parent = [IO.Path]::GetDirectoryName($path)
        while ($parent -and $parent.Length -ge $Directory.Length) {
            Assert-IgitDirectory $parent
            $parent = [IO.Path]::GetDirectoryName($parent)
        }
        if (-not (Test-IgitPlainFile $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $property.Value) { throw "Installed file has changed: $path" }
    }
    return $record
}

function Get-IgitInstallation([string]$Root, [string]$ExpectedBin = '', [string]$Executable = '') {
    Assert-IgitDirectory $Root
    Assert-IgitDirectory (Join-Path $Root 'releases')
    $statePath = Join-Path $Root 'install.json'
    if (-not (Test-Path -LiteralPath $statePath)) { return $null }
    $state = Read-IgitJson $statePath
    if ($state.schema -ne 1 -or $state.product -cne 'igit' -or $state.manager -cne 'native' -or $state.target -cne $IgitTarget -or $state.root -ine $Root) { throw 'Installation ownership mismatch.' }
    if (-not [IO.Path]::IsPathRooted($state.bin_dir) -or ($ExpectedBin -and $state.bin_dir -ine $ExpectedBin)) { throw 'Installation directory differs from the recorded owner.' }
    Assert-IgitDirectory $state.bin_dir
    $pointer = Join-Path $Root 'current.txt'
    if (-not (Test-IgitPlainFile $pointer) -or (Get-Item -LiteralPath $pointer -Force).Length -gt 256) { throw 'Current release pointer is missing or replaced.' }
    $selection = [IO.File]::ReadAllText($pointer).Trim()
    if ($selection -cnotmatch '^(.+)_x86_64-pc-windows-msvc$') { throw 'Invalid current release pointer.' }
    $null = Get-IgitVersionParts $Matches[1]
    $release = Join-Path (Join-Path $Root 'releases') $selection
    foreach ($name in @('igit', 'ig')) {
        $launcher = Join-Path $state.bin_dir "$name.cmd"
        $script = Join-Path $state.bin_dir ".$name-launch.ps1"
        if (-not (Test-IgitPlainFile $launcher) -or [IO.File]::ReadAllText($launcher, $IgitUtf8) -cne (Get-IgitLauncher $Root $name) -or -not (Test-IgitPlainFile $script) -or [IO.File]::ReadAllText($script, $IgitUtf8) -cne (Get-IgitLauncherScript $Root $name) -or (Test-Path -LiteralPath (Join-Path $state.bin_dir "$name.exe"))) { throw "Launcher is not owned by this installation: $launcher" }
    }
    $record = Assert-IgitRelease $release
    if ($Executable -and $Executable -ine (Join-Path $release 'igit.exe') -and $Executable -ine (Join-Path $release 'ig.exe')) { throw 'The running executable is not the active native release; restart IGit.' }
    return @{ root = $Root; bin_dir = $state.bin_dir; version = $record.version; channel = (Get-IgitChannel $record.version); release = $release }
}

function Write-IgitProgress([string]$Stage) {
    if ($ProgressDir) {
        try { [IO.File]::WriteAllText((Join-Path $ProgressDir 'stage'), $Stage, $IgitUtf8) }
        catch { } # Progress is best effort; the installation result is authoritative.
    }
}

function Install-IgitRelease([string]$Root, [string]$Bin, [string]$Archive, [string]$ReleaseVersion, [string]$Digest) {
    Assert-IgitDirectory $Bin
    $existing = Get-IgitInstallation $Root $Bin
    if (-not $existing) {
        if ((Test-Path -LiteralPath $Root) -and @(Get-ChildItem -LiteralPath $Root -Force).Count) { throw "Refusing an unowned nonempty installation root: $Root" }
        foreach ($name in @('igit.cmd', 'ig.cmd', '.igit-launch.ps1', '.ig-launch.ps1', 'igit.exe', 'ig.exe', 'igit.ps1', 'ig.ps1', 'igit.bat', 'ig.bat')) {
            if (Test-Path -LiteralPath (Join-Path $Bin $name)) { throw "Refusing to overwrite an existing command: $name" }
        }
    }
    [IO.Directory]::CreateDirectory($Root) | Out-Null
    $lockPath = Join-Path $Root '.install.lock'
    try { $lock = [IO.File]::Open($lockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None) }
    catch { throw "Another installer may be running; inspect $lockPath" }
    $createdRelease = $false; $createdState = $false; $activated = $false
    $createdBin = -not [IO.Directory]::Exists($Bin)
    $createdLaunchers = @()
    $releases = Join-Path $Root 'releases'
    $selection = "$($ReleaseVersion)_$IgitTarget"
    $release = Join-Path $releases $selection
    $stage = Join-Path $releases ('.stage-' + [Guid]::NewGuid().ToString('N'))
    try {
        [IO.Directory]::CreateDirectory($releases) | Out-Null
        if (Test-Path -LiteralPath $release) {
            Write-IgitProgress 'verifying_installation'
            if ((Assert-IgitRelease $release).archive_sha256 -cne $Digest) { throw 'Existing release differs; refusing to replace it.' }
        } else {
            [IO.Directory]::CreateDirectory($stage) | Out-Null
            Write-IgitProgress 'extracting'
            Expand-IgitArchive $Archive $stage $ReleaseVersion
            Write-IgitProgress 'verifying_installation'
            Assert-IgitBinaries $stage $ReleaseVersion
            Write-IgitAtomic (Join-Path $stage '.igit-release.json') ((Get-IgitReleaseRecord $stage $ReleaseVersion $Digest) | ConvertTo-Json -Depth 8)
            [IO.Directory]::Move($stage, $release)
            $createdRelease = $true
        }
        if ($existing) {
            if ((Get-IgitInstallation $Root $Bin).release -ine $existing.release) { throw 'Active release changed during installation.' }
        } else {
            [IO.Directory]::CreateDirectory($Bin) | Out-Null
            foreach ($name in @('igit', 'ig')) {
                $sources = @{
                    "$name.cmd" = Get-IgitLauncher $Root $name
                    ".$name-launch.ps1" = Get-IgitLauncherScript $Root $name
                }
                foreach ($file in $sources.Keys) {
                    $launcher = Join-Path $Bin $file
                    $stream = [IO.File]::Open($launcher, [IO.FileMode]::CreateNew)
                    $createdLaunchers += $launcher
                    try {
                        $bytes = $IgitUtf8.GetBytes($sources[$file])
                        $stream.Write($bytes, 0, $bytes.Length)
                    } finally { $stream.Dispose() }
                }
            }
            Write-IgitAtomic (Join-Path $Root 'install.json') (@{ schema = 1; product = 'igit'; manager = 'native'; root = $Root; bin_dir = $Bin; target = $IgitTarget } | ConvertTo-Json)
            $createdState = $true
        }
        Write-IgitProgress 'activating'
        Write-IgitAtomic (Join-Path $Root 'current.txt') ($selection + "`n")
        $activated = $true
    } finally {
        if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
        if (-not $activated) {
            foreach ($launcher in $createdLaunchers) { [IO.File]::Delete($launcher) }
            if ($createdState) { [IO.File]::Delete((Join-Path $Root 'install.json')) }
            if ($createdRelease) { Remove-Item -LiteralPath $release -Recurse -Force }
            if ($createdBin -and [IO.Directory]::Exists($Bin) -and -not @(Get-ChildItem -LiteralPath $Bin -Force).Count) { [IO.Directory]::Delete($Bin) }
            if (-not $existing -and [IO.Directory]::Exists($releases) -and -not @(Get-ChildItem -LiteralPath $releases -Force).Count) { [IO.Directory]::Delete($releases) }
        }
        $lock.Dispose()
        [IO.File]::Delete($lockPath)
    }
}

function Invoke-IgitInstall {
    if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) { throw 'This installer requires 64-bit Windows.' }
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    if ($arch -ne 'AMD64') { throw "Unsupported Windows architecture: $arch" }
    if ($Unmanaged -and (-not $Check -or $FromExecutable -or -not $RunningVersion)) { throw 'Unmanaged inspection requires Check and RunningVersion.' }
    if ($FromExecutable) {
        $executable = Get-IgitFullPath $FromExecutable
        $root = [IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($executable)))
        $state = Get-IgitInstallation $root '' $executable
        if (-not $state -or $state.version -cne $RunningVersion) { throw 'Executable is not owned by the active native installation.' }
        $bin = $state.bin_dir
    } elseif ($Unmanaged) {
        $state = $null
        Get-IgitVersionParts $RunningVersion | Out-Null
    } else {
        $root = Get-IgitFullPath $(if ($InstallRoot) { $InstallRoot } else { Join-Path $env:LOCALAPPDATA 'IGit' })
        $bin = Get-IgitFullPath $(if ($InstallDir) { $InstallDir } else { Join-Path $root 'bin' })
        $state = Get-IgitInstallation $root $bin
    }
    $selectedChannel = if ($Channel) { $Channel } elseif ($state) { $state.channel } elseif ($Unmanaged -and $RunningVersion.Contains('-beta.')) { 'beta' } else { 'stable' }
    $base = if ($env:IGIT_CHANNEL_BASE_URL) { $env:IGIT_CHANNEL_BASE_URL.TrimEnd('/') } else { 'https://igit.yelog.org/channels' }
    if (-not (Test-IgitUrl $base)) { throw 'Invalid channel base URL.' }
    if ($ProgressDir) {
        if (-not $FromExecutable -or $Check -or -not $Json -or (Test-Path -LiteralPath $ProgressDir)) {
            throw 'Progress requires an embedded installation update and a new directory.'
        }
        $temporary = $ProgressDir # The Rust worker cleans up its private directory.
    } else {
        $temporary = Join-Path ([IO.Path]::GetTempPath()) ('igit-install-' + [Guid]::NewGuid().ToString('N'))
    }
    [IO.Directory]::CreateDirectory($temporary) | Out-Null
    try {
        $manifestPath = Join-Path $temporary 'manifest.json'
        try { Invoke-IgitDownload "$base/$selectedChannel.json" $manifestPath 1048576 }
        catch { throw "Cannot read the $selectedChannel release channel; it may not have a published release yet. $_" }
        $manifest = Read-IgitJson $manifestPath
        Assert-IgitManifest $manifest $selectedChannel $base
        if ($Version -and $Version -cne $manifest.version) { throw 'Requested version is not the published channel version.' }
        $currentVersion = if ($state) { $state.version } elseif ($Unmanaged) { $RunningVersion } else { $null }
        $report = [ordered]@{ schema = 1; product = 'igit'; channel = $selectedChannel; current_version = $currentVersion; target_version = $manifest.version; status = '' }
        $comparison = if ($currentVersion) { Compare-IgitVersion $manifest.version $currentVersion } else { 1 }
        if ($comparison -lt 0 -and -not $AllowDowngrade) {
            if ($state -and $selectedChannel -ne $state.channel) { throw 'Channel switch would downgrade IGit; use --allow-downgrade explicitly.' }
            $report.status = 'up_to_date'
        } elseif ($comparison -eq 0) { $report.status = 'up_to_date' }
        elseif ($Check) { $report.status = 'update_available' }
        else {
            [Console]::Error.WriteLine("Downloading IGit $($manifest.version) ($IgitTarget)...")
            $archive = Join-Path $temporary 'release.zip'
            $asset = $manifest.assets.$IgitTarget
            Write-IgitProgress 'downloading'
            $headers = if ($ProgressDir) { Join-Path $temporary 'download.headers' } else { '' }
            Invoke-IgitDownload $asset.url $archive -Headers $headers
            Write-IgitProgress 'verifying_checksum'
            $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actual -cne $asset.sha256) { throw 'Release archive checksum mismatch; installation unchanged.' }
            Install-IgitRelease $root $bin $archive $manifest.version $actual
            Write-IgitProgress 'finishing'
            $report.status = if ($state) { 'updated' } else { 'installed' }
            if (-not $FromExecutable -and -not $NoModifyPath) {
                try {
                    $userPath = [string][Environment]::GetEnvironmentVariable('Path', 'User')
                    if ($bin -notin ($userPath -split ';')) {
                        [Environment]::SetEnvironmentVariable('Path', (($userPath.TrimEnd(';') + ';' + $bin).Trim(';')), 'User')
                        [Console]::Error.WriteLine('Added IGit to the user PATH. Open a new terminal to use igit and ig.')
                    }
                } catch {
                    [Console]::Error.WriteLine("PATH setup skipped; add $bin to your user PATH. " + $_.Exception.Message)
                }
            }
        }
        if ($Unmanaged) {
            $report.action = 'Update with the original package manager, rebuild IGit, or use https://igit.yelog.org/.'
            if ($report.status -eq 'update_available') { $report.status = 'manager_action_required' }
        }
        return $report
    } finally { if (-not $ProgressDir) { Remove-Item -LiteralPath $temporary -Recurse -Force } }
}

if ($LibraryOnly) { return }
$previousEncoding = [Console]::OutputEncoding
try {
    [Console]::OutputEncoding = $IgitUtf8
    $report = Invoke-IgitInstall
    if ($Json) { $report | ConvertTo-Json -Compress }
    else { Write-Output "IGit $($report.target_version): $($report.status.Replace('_', ' ')) ($($report.channel))" }
} catch {
    if ($Json) { @{ schema = 1; product = 'igit'; status = 'error'; error = $_.Exception.Message } | ConvertTo-Json -Compress }
    else { [Console]::Error.WriteLine('IGit installer: ' + $_.Exception.Message) }
    exit 1
} finally { [Console]::OutputEncoding = $previousEncoding }
