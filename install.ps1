$ErrorActionPreference = "Stop"

$Repository = "Vaibhav701161/structtrace"
$Version = if ($env:STRUCTTRACE_VERSION) { $env:STRUCTTRACE_VERSION } else { "latest" }
$InstallDir = if ($env:STRUCTTRACE_INSTALL_DIR) {
    $env:STRUCTTRACE_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Programs\StructTrace"
}
$Target = "x86_64-pc-windows-msvc"
$Asset = "structtrace-$Target.zip"
$BaseUrl = if ($Version -eq "latest") {
    "https://github.com/$Repository/releases/latest/download"
} else {
    "https://github.com/$Repository/releases/download/$Version"
}
$TemporaryDir = Join-Path ([System.IO.Path]::GetTempPath()) ("structtrace-" + [guid]::NewGuid())

try {
    New-Item -ItemType Directory -Path $TemporaryDir | Out-Null
    $Archive = Join-Path $TemporaryDir $Asset
    $ChecksumFile = "$Archive.sha256"
    Invoke-WebRequest "$BaseUrl/$Asset" -OutFile $Archive
    Invoke-WebRequest "$BaseUrl/$Asset.sha256" -OutFile $ChecksumFile
    $Expected = ((Get-Content $ChecksumFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLowerInvariant()
    if ($Expected -ne $Actual) { throw "SHA-256 verification failed for $Asset" }
    Expand-Archive -Path $Archive -DestinationPath $TemporaryDir -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item (Join-Path $TemporaryDir "structtrace.exe") (Join-Path $InstallDir "structtrace.exe") -Force
    & (Join-Path $InstallDir "structtrace.exe") --version
    Write-Host "Installed StructTrace to $InstallDir\structtrace.exe"
    $PathEntries = $env:PATH -split ';'
    if ($PathEntries -notcontains $InstallDir) {
        $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $NewUserPath = if ([string]::IsNullOrWhiteSpace($UserPath)) { $InstallDir } else { "$UserPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
        $env:PATH = "$env:PATH;$InstallDir"
        Write-Host "Added $InstallDir to the current process and persistent user PATH."
    }
    Write-Host "Uninstall by deleting $InstallDir\structtrace.exe"
} finally {
    if (Test-Path $TemporaryDir) { Remove-Item -Recurse -Force $TemporaryDir }
}
