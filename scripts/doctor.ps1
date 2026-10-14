# 開発環境の診断(Windows)。引っ越し先のPCで、必要な道具がそろっているかを確かめる。
# Environment check (Windows). Run on a new PC to see which required tools are present.
#   powershell -File scripts\doctor.ps1
# (このファイルはBOM付きUTF-8。Windows PowerShell 5.1で日本語が化けないため / saved as UTF-8 with BOM for Windows PowerShell 5.1)
$ErrorActionPreference = "SilentlyContinue"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

# コマンドの出力の1行目を見て、「見つからない」系のメッセージでなければOKとする。
function Check($name, $cmd, $required = $true) {
    $out = (& cmd /c "$cmd 2>&1" | Select-Object -First 1)
    $bad = (-not $out) -or ($out -match "not recognized|認識されません|Could not find|見つかりません|^INFO:")
    $mark = if (-not $bad) { "OK " } elseif ($required) { "NG " } else { "-- " }
    "{0} {1,-24} {2}" -f $mark, $name, $(if ($bad) { "(not found / 見つかりません)" } else { $out })
}

"== 必須 / Required =="
Check "git" "git --version"
Check "node (20+)" "node --version"
Check "npm" "npm --version"
Check "rustc" "rustc --version"
Check "cargo" "cargo --version"
Check "gh (GitHub CLI)" "gh --version"
# MSVCのビルドツールは、PATHに無くてもvswhereで見つかる。
$vsw = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = if (Test-Path $vsw) { & $vsw -products * -latest -property installationPath } else { $null }
"{0} {1,-24} {2}" -f ($(if ($vs) { "OK " } else { "NG " }), "Visual Studio Build Tools", $(if ($vs) { $vs } else { "(not found: install 'Desktop development with C++')" }))

"== Android(任意 / optional) =="
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
"{0} {1,-24} {2}" -f ($(if (Test-Path "$sdk\ndk\27.1.12297006") { "OK " } else { "-- " }), "NDK 27.1.12297006", "$sdk\ndk")
Check "java (17)" "java -version" $false
Check "adb" "`"$sdk\platform-tools\adb.exe`" version" $false
$tg = (& cmd /c "rustup target list --installed 2>&1") -join " "
"{0} {1,-24} {2}" -f ($(if ($tg -match "aarch64-linux-android") { "OK " } else { "-- " }), "rust target android", $(if ($tg -match "aarch64-linux-android") { "aarch64-linux-android" } else { "(rustup target add aarch64-linux-android)" }))

"== Ubuntu(WSL、テスト用 / test only) =="
$wsl = ((& cmd /c "wsl -l -q 2>&1") -join "") -replace "`0", ""
"{0} {1,-24} {2}" -f ($(if ($wsl -match "Ubuntu") { "OK " } else { "-- " }), "wsl Ubuntu", $(if ($wsl) { $wsl.Trim() } else { "(not installed)" }))

"== サイドカー(CIが同梱。ローカルで実行ファイルを作るときだけ必要) / Sidecars (only for local exe builds) =="
$bin = Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")).Path "src-tauri\binaries"
foreach ($n in "ffmpeg-x86_64-pc-windows-msvc.exe", "ffprobe-x86_64-pc-windows-msvc.exe", "rs-ffmpeg-x86_64-pc-windows-msvc.exe", "rs-xorriso-x86_64-pc-windows-msvc.exe") {
    "{0} {1}" -f ($(if (Test-Path (Join-Path $bin $n)) { "OK " } else { "-- " }), $n)
}

"== 鍵・認証 / Keys & auth =="
(& cmd /c "gh auth status 2>&1") | Select-Object -First 2
"更新用の署名鍵(リポジトリには入れない) / updater signing key (never in the repo): e.g. F:\open-media-updater.key"
