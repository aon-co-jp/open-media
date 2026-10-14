# 開発環境の診断(Windows)。引っ越し先のPCで、必要な道具がそろっているかを確かめる。
# Environment check (Windows). Run on a new PC to see which required tools are present.
#   powershell -File scripts\doctor.ps1
$ErrorActionPreference = "SilentlyContinue"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

function Check($name, $cmd, $required = $true) {
    $out = & cmd /c "$cmd 2>&1" | Select-Object -First 1
    $ok = ($LASTEXITCODE -eq 0) -and $out
    $mark = if ($ok) { "OK " } elseif ($required) { "NG " } else { "-- " }
    "{0} {1,-22} {2}" -f $mark, $name, $out
}

"== 必須 / Required =="
Check "git" "git --version"
Check "node (20+)" "node --version"
Check "npm" "npm --version"
Check "rustc" "rustc --version"
Check "cargo" "cargo --version"
Check "MSVC link.exe" "where link"
Check "gh (GitHub CLI)" "gh --version"
"== Android(任意 / optional) =="
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
"{0} {1,-22} {2}" -f ($(if (Test-Path "$sdk\ndk\27.1.12297006") { "OK " } else { "-- " }), "NDK 27.1.12297006", "$sdk\ndk")
Check "java (17)" "java -version" $false
Check "adb" "`"$sdk\platform-tools\adb.exe`" version" $false
Check "rust target android" "rustup target list --installed" $false
"== Ubuntu(WSL、テスト用 / test only) =="
Check "wsl" "wsl -l -q" $false
"== サイドカー(CIが同梱。ローカルで実行ファイルを作るときだけ必要) / Sidecars (only for local exe builds) =="
$bin = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")).Path "src-tauri\binaries"
foreach ($n in "ffmpeg-x86_64-pc-windows-msvc.exe", "ffprobe-x86_64-pc-windows-msvc.exe", "rs-ffmpeg-x86_64-pc-windows-msvc.exe", "rs-xorriso-x86_64-pc-windows-msvc.exe") {
    "{0} {1}" -f ($(if (Test-Path (Join-Path $bin $n)) { "OK " } else { "-- " }), $n)
}
"== 鍵・認証 / Keys & auth =="
gh auth status 2>&1 | Select-Object -First 2
"更新用の署名鍵(リポジトリには入れない) / updater signing key (never in the repo): 例 F:\open-media-updater.key"
