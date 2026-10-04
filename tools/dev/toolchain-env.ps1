# Dot-source this file. Changes this PowerShell process only, never user/system PATH.
$taskRustRoot = 'D:\Software\WitvoiceToolchain\Rust'
if (Test-Path -LiteralPath (Join-Path $taskRustRoot 'cargo\bin\rustc.exe')) {
    if (-not $env:CARGO_HOME) { $env:CARGO_HOME = Join-Path $taskRustRoot 'cargo' }
    if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $taskRustRoot 'rustup' }
    $taskCargoBin = Join-Path $env:CARGO_HOME 'bin'
    if (($env:PATH -split ';') -notcontains $taskCargoBin) {
        $env:PATH = "$taskCargoBin;$env:PATH"
    }
}
