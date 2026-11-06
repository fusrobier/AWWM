#Requires -Version 5.1
<#
.SYNOPSIS
    Verifies the AWWM release artifacts built by `npm run tauri build` and stages them
    in release\<version>\ together with SHA256SUMS.txt.

.DESCRIPTION
    Read-only checks first; the script stops with exit code 1 and a clear message at the
    first failure:

      1. The NSIS setup (bundle\nsis\<productName>_<version>_*-setup.exe) and the MSI
         (bundle\msi\<productName>_<version>_*.msi) exist for the version declared in
         src-tauri\tauri.conf.json.
      2. bundle.windows.webviewInstallMode embeds a WebView2 installer (embedBootstrapper or
         offlineInstaller), and every WebView2 installer file the bundler embedded - the
         files referenced by the generated nsis\<arch>\installer.nsi and wix\<arch>\main.wxs -
         has a valid Authenticode signature whose signer is Microsoft Corporation and whose
         certificate chain ends in a Microsoft root. The installers therefore never contain
         or download an unverified dependency.
      3. <target>\release\<mainBinaryName>.exe carries file and product version = config version.
      4. Its PE import table (normal and delay-load imports) contains no Visual C++ runtime
         DLL (vcruntime*, msvcp<digit>*, msvcr<digit>*, concrt*, vccorlib*): the VC runtime is
         statically linked, so users need no VC++ Redistributable.

    Then it recreates release\<version>\ (only that folder), copies the NSIS setup, the MSI and
    the executable into it and writes SHA256SUMS.txt (format "<sha256>  <file name>",
    lowercase hex, LF line endings, compatible with `sha256sum -c`).

    Compatible with Windows PowerShell 5.1 and PowerShell 7.

.PARAMETER RepoRoot
    Repository root. Default: the parent folder of this script.

.PARAMETER TargetDir
    Cargo target directory. Default: $env:CARGO_TARGET_DIR if set, otherwise <RepoRoot>\target.

.EXAMPLE
    pwsh -File scripts\verify-release.ps1
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot),
    [string]$TargetDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------
function Write-Step([string]$Text) { Write-Host ''; Write-Host "== $Text" -ForegroundColor Cyan }
function Write-Ok([string]$Text) { Write-Host "  [ OK ] $Text" -ForegroundColor Green }
function Write-Info([string]$Text) { Write-Host "         $Text" }
function Stop-Release([string]$Text) { throw $Text }

# Reads a nested property ('a.b.c') of a ConvertFrom-Json object; $null when absent.
function Get-JsonValue($Object, [string]$Path) {
    $current = $Object
    foreach ($name in $Path.Split('.')) {
        if ($null -eq $current) { return $null }
        $property = $current.PSObject.Properties[$name]
        if ($null -eq $property) { return $null }
        $current = $property.Value
    }
    return $current
}

# ---------------------------------------------------------------------------
# Authenticode: valid signature, signer Microsoft Corporation, Microsoft root
# ---------------------------------------------------------------------------
function Assert-MicrosoftSigned([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Stop-Release "WebView2 installer referenced by the bundler does not exist: $Path"
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    Write-Info "File:        $Path"
    Write-Info "Status:      $($signature.Status)"
    if ($signature.Status -eq [System.Management.Automation.SignatureStatus]::NotSigned) {
        Stop-Release "WebView2 installer '$Path' is not digitally signed."
    }
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        Stop-Release "WebView2 installer '$Path' has no valid Authenticode signature ($($signature.Status): $($signature.StatusMessage))."
    }
    $certificate = $signature.SignerCertificate
    $commonName = $certificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    $organization = if ($certificate.Subject -match '(?:^|,\s*)O=("?)([^,"]+)\1') { $Matches[2] } else { '' }
    Write-Info "Signer:      $($certificate.Subject)"
    Write-Info "Issuer:      $($certificate.Issuer)"
    Write-Info "Thumbprint:  $($certificate.Thumbprint)"
    if ($null -ne $signature.TimeStamperCertificate) {
        Write-Info "Timestamp:   $($signature.TimeStamperCertificate.Subject)"
    }
    if ($commonName -ne 'Microsoft Corporation' -or $organization -ne 'Microsoft Corporation') {
        Stop-Release "WebView2 installer '$Path' is signed by '$($certificate.Subject)', not by Microsoft Corporation."
    }

    # Get-AuthenticodeSignature already validated trust; this only confirms whose root it is.
    $chain = New-Object System.Security.Cryptography.X509Certificates.X509Chain
    try {
        $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
        $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::IgnoreNotTimeValid
        [void]$chain.Build($certificate)
        $elements = @($chain.ChainElements)
        if ($elements.Count -eq 0) { Stop-Release "Could not build the certificate chain of '$Path'." }
        $root = $elements[$elements.Count - 1].Certificate
        Write-Info "Chain root:  $($root.Subject)"
        if ($root.Subject -notmatch '^CN=Microsoft Root Certificate Authority' -or $root.Subject -notmatch 'O=Microsoft Corporation') {
            Stop-Release "The signature of '$Path' does not chain to a Microsoft root certificate (root: $($root.Subject))."
        }
    } finally {
        $chain.Reset()
    }
    Write-Info "SHA-256:     $((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant())"
}

# ---------------------------------------------------------------------------
# Minimal PE reader: DLL names from the import and delay-load import directories
# ---------------------------------------------------------------------------
function Get-PeImport([string]$Path) {
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $length = [long]$bytes.Length
    $require = {
        param([long]$Offset, [long]$Size)
        if ($Offset -lt 0 -or ($Offset + $Size) -gt $length) { Stop-Release "'$Path' is not a valid PE file (read beyond end of file)." }
    }
    $u16 = { param([long]$Offset) & $require $Offset 2; [long][System.BitConverter]::ToUInt16($bytes, [int]$Offset) }
    $u32 = { param([long]$Offset) & $require $Offset 4; [long][System.BitConverter]::ToUInt32($bytes, [int]$Offset) }
    $u64 = { param([long]$Offset) & $require $Offset 8; [System.BitConverter]::ToUInt64($bytes, [int]$Offset) }

    if ((& $u16 0) -ne 0x5A4D) { Stop-Release "'$Path' is not a PE file (missing MZ header)." }
    $peOffset = & $u32 0x3C
    if ((& $u32 $peOffset) -ne 0x00004550) { Stop-Release "'$Path' is not a PE file (missing PE signature)." }
    $coff = $peOffset + 4
    $machine = & $u16 $coff
    $sectionCount = & $u16 ($coff + 2)
    $optionalSize = & $u16 ($coff + 16)
    $optional = $coff + 20
    $magic = & $u16 $optional
    switch ($magic) {
        0x20B { $imageBase = & $u64 ($optional + 24); $rvaCount = & $u32 ($optional + 108); $directories = $optional + 112 }
        0x10B { $imageBase = [uint64](& $u32 ($optional + 28)); $rvaCount = & $u32 ($optional + 92); $directories = $optional + 96 }
        default { Stop-Release "'$Path' has an unknown optional header magic 0x$('{0:X}' -f $magic)." }
    }

    $sections = @()
    $sectionTable = $optional + $optionalSize
    for ($i = 0; $i -lt $sectionCount; $i++) {
        $s = $sectionTable + 40 * $i
        $virtualSize = & $u32 ($s + 8)
        $rawSize = & $u32 ($s + 16)
        $sections += [pscustomobject]@{
            VirtualAddress = & $u32 ($s + 12)
            Span           = [Math]::Max($virtualSize, $rawSize)
            RawPointer     = & $u32 ($s + 20)
        }
    }
    $toOffset = {
        param([long]$Rva)
        foreach ($section in $sections) {
            if ($Rva -ge $section.VirtualAddress -and $Rva -lt ($section.VirtualAddress + $section.Span)) {
                return $Rva - $section.VirtualAddress + $section.RawPointer
            }
        }
        Stop-Release "'$Path': RVA 0x$('{0:X}' -f $Rva) is outside every section."
    }
    $readName = {
        param([long]$Rva)
        $start = & $toOffset $Rva
        $end = $start
        while ($end -lt $length -and $bytes[$end] -ne 0) { $end++ }
        if ($end -ge $length) { Stop-Release "'$Path': unterminated import name." }
        [System.Text.Encoding]::ASCII.GetString($bytes, [int]$start, [int]($end - $start))
    }
    $directory = {
        param([int]$Index)
        if ($Index -ge $rvaCount) { return 0 }
        & $u32 ($directories + 8 * $Index)
    }

    $imports = New-Object System.Collections.Generic.List[string]
    $importRva = & $directory 1
    if ($importRva -ne 0) {
        $descriptor = & $toOffset $importRva
        while ($true) {
            $nameRva = & $u32 ($descriptor + 12)
            if ($nameRva -eq 0 -and (& $u32 $descriptor) -eq 0 -and (& $u32 ($descriptor + 16)) -eq 0) { break }
            $imports.Add((& $readName $nameRva))
            $descriptor += 20
        }
    }

    $delayImports = New-Object System.Collections.Generic.List[string]
    $delayRva = & $directory 13
    if ($delayRva -ne 0) {
        $descriptor = & $toOffset $delayRva
        while ($true) {
            $attributes = & $u32 $descriptor
            $nameRva = & $u32 ($descriptor + 4)
            if ($nameRva -eq 0) { break }
            # Attribute bit 0 clear = legacy descriptor holding virtual addresses instead of RVAs.
            if (($attributes -band 1) -eq 0) { $nameRva = [long]([uint64]$nameRva - $imageBase) }
            $delayImports.Add((& $readName $nameRva))
            $descriptor += 32
        }
    }

    [pscustomobject]@{
        Machine      = $machine
        Imports      = $imports.ToArray()
        DelayImports = $delayImports.ToArray()
    }
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
try {
    $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).ProviderPath
    if (-not $TargetDir) {
        if ($env:CARGO_TARGET_DIR) { $TargetDir = $env:CARGO_TARGET_DIR } else { $TargetDir = Join-Path $RepoRoot 'target' }
    }
    if (-not [System.IO.Path]::IsPathRooted($TargetDir)) { $TargetDir = Join-Path $RepoRoot $TargetDir }
    $releaseOut = Join-Path $TargetDir 'release'

    Write-Step 'Configuration'
    $configPath = Join-Path $RepoRoot 'src-tauri\tauri.conf.json'
    if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) { Stop-Release "Tauri configuration not found: $configPath" }
    $config = Get-Content -LiteralPath $configPath -Raw -Encoding UTF8 | ConvertFrom-Json

    $version = [string](Get-JsonValue $config 'version')
    if ($version -like '*.json') {
        # Tauri allows "version" to point to a package.json file.
        $versionFile = Join-Path (Split-Path -Parent $configPath) $version
        $version = [string](Get-JsonValue (Get-Content -LiteralPath $versionFile -Raw -Encoding UTF8 | ConvertFrom-Json) 'version')
    }
    if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)([-+][0-9A-Za-z.+-]+)?$') { Stop-Release "Invalid or missing version in ${configPath}: '$version'." }
    $numericVersion = '{0}.{1}.{2}' -f $Matches[1], $Matches[2], $Matches[3]
    $productName = [string](Get-JsonValue $config 'productName')
    $binaryName = [string](Get-JsonValue $config 'mainBinaryName')
    if (-not $productName) { Stop-Release "productName is missing in $configPath." }
    if (-not $binaryName) { Stop-Release "mainBinaryName is missing in $configPath." }
    $webviewMode = [string](Get-JsonValue $config 'bundle.windows.webviewInstallMode.type')
    if (-not $webviewMode) { $webviewMode = 'downloadBootstrapper (Tauri default)' }
    Write-Info "Repository:  $RepoRoot"
    Write-Info "Target dir:  $TargetDir"
    Write-Info "Product:     $productName $version (binary $binaryName.exe)"
    Write-Info "WebView2:    webviewInstallMode = $webviewMode"

    # --- 1. Installers -------------------------------------------------------
    Write-Step "Installers for version $version"
    $nsisDir = Join-Path $releaseOut 'bundle\nsis'
    $msiDir = Join-Path $releaseOut 'bundle\msi'
    $setups = @(if (Test-Path -LiteralPath $nsisDir) { Get-ChildItem -LiteralPath $nsisDir -File -Filter "$($productName)_$($version)_*-setup.exe" })
    $msis = @(if (Test-Path -LiteralPath $msiDir) { Get-ChildItem -LiteralPath $msiDir -File -Filter "$($productName)_$($version)_*.msi" })
    if ($setups.Count -eq 0) { Stop-Release "NSIS installer $($productName)_$($version)_*-setup.exe not found in $nsisDir. Run 'npm run tauri build' first." }
    if ($msis.Count -eq 0) { Stop-Release "MSI installer $($productName)_$($version)_*.msi not found in $msiDir. Run 'npm run tauri build' first." }
    foreach ($file in $setups + $msis) {
        Write-Ok ('{0} ({1:N0} bytes, {2:yyyy-MM-dd HH:mm:ss})' -f $file.Name, $file.Length, $file.LastWriteTime)
        $installerSignature = Get-AuthenticodeSignature -LiteralPath $file.FullName
        Write-Info "Authenticode signature of the installer itself: $($installerSignature.Status)"
    }
    # Build time of each bundle type (its oldest file), used to detect later changes of the
    # WebView2 installer that bundle embedded.
    $bundleTime = @{
        NSIS = ($setups | Sort-Object LastWriteTime | Select-Object -First 1).LastWriteTime
        MSI  = ($msis | Sort-Object LastWriteTime | Select-Object -First 1).LastWriteTime
    }

    # --- 2. Embedded WebView2 installers --------------------------------------
    Write-Step 'Embedded Microsoft Edge WebView2 installers'
    if ($webviewMode -ne 'embedBootstrapper' -and $webviewMode -ne 'offlineInstaller') {
        Stop-Release "bundle.windows.webviewInstallMode is '$webviewMode'. A release must embed Microsoft's signed WebView2 installer ('embedBootstrapper' or 'offlineInstaller') so that it can be verified here and is never downloaded unverified at install time."
    }
    $embedded = New-Object System.Collections.Generic.List[object]

    $nsisScripts = @(if (Test-Path -LiteralPath (Join-Path $releaseOut 'nsis')) { Get-ChildItem -LiteralPath (Join-Path $releaseOut 'nsis') -Directory | ForEach-Object { Join-Path $_.FullName 'installer.nsi' } | Where-Object { Test-Path -LiteralPath $_ } })
    if ($nsisScripts.Count -eq 0) { Stop-Release "Generated NSIS script (nsis\<arch>\installer.nsi) not found under $releaseOut." }
    foreach ($script in $nsisScripts) {
        $text = [System.IO.File]::ReadAllText($script)
        $scriptVersion = if ($text -match '(?m)^!define VERSION "([^"]*)"') { $Matches[1] } else { '' }
        if ($scriptVersion -ne $version) { Stop-Release "$script was generated for version '$scriptVersion', not $version. Rebuild." }
        foreach ($define in 'WEBVIEW2BOOTSTRAPPERPATH', 'WEBVIEW2INSTALLERPATH') {
            if ($text -match "(?m)^!define $define `"([^`"]+)`"") {
                $embedded.Add([pscustomobject]@{ Bundle = 'NSIS'; Source = $script; Path = [System.IO.Path]::GetFullPath($Matches[1]) })
            }
        }
    }

    $wixScripts = @(if (Test-Path -LiteralPath (Join-Path $releaseOut 'wix')) { Get-ChildItem -LiteralPath (Join-Path $releaseOut 'wix') -Directory | ForEach-Object { Join-Path $_.FullName 'main.wxs' } | Where-Object { Test-Path -LiteralPath $_ } })
    if ($wixScripts.Count -eq 0) { Stop-Release "Generated WiX source (wix\<arch>\main.wxs) not found under $releaseOut." }
    foreach ($script in $wixScripts) {
        $text = [System.IO.File]::ReadAllText($script)
        foreach ($binary in [regex]::Matches($text, '<Binary\b[^>]*>')) {
            $id = if ($binary.Value -match '\bId="([^"]+)"') { $Matches[1] } else { '' }
            if ($id -notmatch '^MicrosoftEdgeWebView2(Setup|RuntimeInstaller)\.exe$') { continue }
            if ($binary.Value -notmatch '\bSourceFile="([^"]+)"') { Stop-Release "$script has a WebView2 <Binary> without SourceFile." }
            $embedded.Add([pscustomobject]@{ Bundle = 'MSI'; Source = $script; Path = [System.IO.Path]::GetFullPath($Matches[1]) })
        }
    }

    foreach ($bundle in 'NSIS', 'MSI') {
        if (@($embedded | Where-Object { $_.Bundle -eq $bundle }).Count -eq 0) {
            Stop-Release "The generated $bundle sources embed no WebView2 installer although webviewInstallMode is '$webviewMode'. Rebuild with 'npm run tauri build'."
        }
    }
    foreach ($item in $embedded) {
        Write-Host ''
        Write-Info "[$($item.Bundle)] referenced by $($item.Source)"
        Assert-MicrosoftSigned $item.Path
        # The NSIS bootstrapper lives in Tauri's shared tool cache; make sure no later build
        # (of any project) replaced it after the installer embedding it was produced.
        if ((Get-Item -LiteralPath $item.Path).LastWriteTime -gt $bundleTime[$item.Bundle]) {
            Stop-Release "$($item.Path) was modified after the $($item.Bundle) installer was built, so it may differ from the embedded copy. Rebuild the installers."
        }
        Write-Ok "[$($item.Bundle)] Microsoft-signed WebView2 installer verified"
    }

    # --- 3. Executable version -------------------------------------------------
    Write-Step "$binaryName.exe version"
    $exePath = Join-Path $releaseOut "$binaryName.exe"
    if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) { Stop-Release "$exePath not found. Run 'npm run tauri build' first." }
    $info = (Get-Item -LiteralPath $exePath).VersionInfo
    $fileNumeric = '{0}.{1}.{2}.{3}' -f $info.FileMajorPart, $info.FileMinorPart, $info.FileBuildPart, $info.FilePrivatePart
    $productNumeric = '{0}.{1}.{2}.{3}' -f $info.ProductMajorPart, $info.ProductMinorPart, $info.ProductBuildPart, $info.ProductPrivatePart
    Write-Info "FileVersion:    '$($info.FileVersion)' ($fileNumeric)"
    Write-Info "ProductVersion: '$($info.ProductVersion)' ($productNumeric)"
    if ($fileNumeric -ne "$numericVersion.0" -or $productNumeric -ne "$numericVersion.0") {
        Stop-Release "$binaryName.exe has numeric file/product version $fileNumeric / $productNumeric, expected $numericVersion.0."
    }
    if ($info.FileVersion -ne $version -or $info.ProductVersion -ne $version) {
        Stop-Release "$binaryName.exe has FileVersion '$($info.FileVersion)' / ProductVersion '$($info.ProductVersion)', expected '$version'."
    }
    Write-Ok "$binaryName.exe file and product version = $version"

    # --- 4. Runtime dependencies (PE imports) ----------------------------------
    Write-Step "$binaryName.exe imported DLLs"
    $pe = Get-PeImport $exePath
    $machineName = switch ($pe.Machine) { 0x8664 { 'x64' } 0xAA64 { 'ARM64' } 0x14C { 'x86' } default { '0x{0:X}' -f $pe.Machine } }
    Write-Info "Machine: $machineName"
    Write-Info "Imports ($(@($pe.Imports).Count)):"
    foreach ($dll in $pe.Imports) { Write-Info "  $dll" }
    Write-Info "Delay-load imports ($(@($pe.DelayImports).Count)):"
    foreach ($dll in $pe.DelayImports) { Write-Info "  $dll" }
    $vcRuntimePattern = '^(vcruntime|msvcp\d|msvcr\d|concrt|vccorlib)'
    $vcRuntime = @(@($pe.Imports) + @($pe.DelayImports) | Where-Object { $_ -match $vcRuntimePattern })
    if ($vcRuntime.Count -gt 0) {
        Stop-Release "$binaryName.exe imports Visual C++ runtime DLLs ($($vcRuntime -join ', ')); users would need the VC++ Redistributable. Keep the VC runtime statically linked."
    }
    Write-Ok 'No Visual C++ runtime DLL imported (only Windows system DLLs and the Windows 10+ Universal CRT)'

    # --- 5. Stage release files and checksums ----------------------------------
    Write-Step "Release folder"
    $releaseRoot = Join-Path $RepoRoot 'release'
    $releaseDir = Join-Path $releaseRoot $version
    if (Test-Path -LiteralPath $releaseDir) { Remove-Item -LiteralPath $releaseDir -Recurse -Force }
    New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
    $artifacts = @($setups + $msis | ForEach-Object { $_.FullName }) + @($exePath)
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($artifact in $artifacts) {
        Copy-Item -LiteralPath $artifact -Destination $releaseDir
        $copy = Join-Path $releaseDir (Split-Path -Leaf $artifact)
        $hash = (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant()
        $lines.Add("$hash  $(Split-Path -Leaf $artifact)")
    }
    $sumsPath = Join-Path $releaseDir 'SHA256SUMS.txt'
    [System.IO.File]::WriteAllText($sumsPath, (($lines -join "`n") + "`n"), (New-Object System.Text.UTF8Encoding($false)))
    Write-Info "Folder: $releaseDir"
    Write-Info 'SHA256SUMS.txt:'
    foreach ($line in $lines) { Write-Info "  $line" }
    Write-Ok "Release $version verified and staged"
    exit 0
} catch {
    Write-Host ''
    Write-Host "RELEASE VERIFICATION FAILED: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
