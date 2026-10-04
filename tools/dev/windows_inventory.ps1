# Read-only metadata. Does not open microphones, configure devices or install tools.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$result = [ordered]@{
    powershell = $PSVersionTable.PSVersion.ToString()
    platform = [Environment]::OSVersion.Platform.ToString()
    os = $null
    cpu = @()
    gpu = @()
    memory_bytes = $null
    errors = @()
    commands = [ordered]@{}
    sdk = [ordered]@{ root = $null; versions = @(); status = 'MISSING' }
}
try {
    $result.os = Get-CimInstance Win32_OperatingSystem |
        Select-Object Caption, Version, BuildNumber, OSArchitecture
    $result.cpu = @(Get-CimInstance Win32_Processor | Select-Object Name)
    $result.gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
    $result.memory_bytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
} catch {
    $result.errors += $_.Exception.Message
}
foreach ($name in @('git.exe', 'rustc.exe', 'cargo.exe', 'rustup.exe', 'node.exe', 'pnpm.cmd', 'python.exe', 'codex.cmd', 'cl.exe')) {
    $found = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    $result.commands[$name] = if ($found) { $found.Source } else { $null }
}
try {
    $kits = $null
    foreach ($sdkKey in @('HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots')) {
        $sdkEntry = Get-ItemProperty -LiteralPath $sdkKey -ErrorAction SilentlyContinue
        if ($sdkEntry -and $sdkEntry.KitsRoot10) { $kits = $sdkEntry.KitsRoot10; break }
    }
    if (-not $kits) { $result.sdk.status = 'MISSING'; throw 'Windows SDK KitsRoot10 not registered in either registry view' }
    $result.sdk.root = $kits
    $result.sdk.versions = @(Get-ChildItem -LiteralPath (Join-Path $kits 'Include') -Directory | Where-Object {
        (Test-Path -LiteralPath (Join-Path $_.FullName 'um\Windows.h')) -and
        (Test-Path -LiteralPath (Join-Path $_.FullName 'ucrt\stdio.h')) -and
        (Test-Path -LiteralPath (Join-Path $kits "Lib\$($_.Name)\um\x64\kernel32.lib")) -and
        (Test-Path -LiteralPath (Join-Path $kits "Lib\$($_.Name)\ucrt\x64\ucrt.lib"))
    } | Select-Object -ExpandProperty Name)
    if ($result.sdk.versions.Count -gt 0) { $result.sdk.status = 'DISCOVERED_NOT_COMPILED' }
} catch {
    if ($kits) { $result.sdk.status = 'UNKNOWN' }
    $result.sdk.error = $_.Exception.Message
}
$result | ConvertTo-Json -Depth 8 -Compress
