# Androidのローカルビルド(Windows)。`npm run tauri android build`はWindowsの開発者モードが無いと
# jniLibsのシンボリックリンクで失敗するため、Rustのクロスコンパイル + gradlew直叩きで同じ結果を作る。
# Local Android build on Windows. `npm run tauri android build` fails without Windows Developer Mode
# (symlink creation), so cross-compile with cargo and package with gradlew directly.
#
# 使い方 / Usage:   powershell -File scripts\android-local-build.ps1 [-Install]
#   -Install  ビルド後、adbで接続中の端末へインストールする / install to the connected device via adb
# 前提 / Prerequisites: Rust + target aarch64-linux-android, Android SDK + NDK 27.1.12297006, Java 17, (adb for -Install)
param([switch]$Install)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$env:ANDROID_HOME = $sdk
$ndkVer = "27.1.12297006"
$env:NDK_HOME = "$sdk\ndk\$ndkVer"
$ndk = "$env:NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin"
if (-not (Test-Path $ndk)) { throw "NDK not found: $ndk (install ndk;$ndkVer with sdkmanager)" }
$env:CC_aarch64_linux_android = "$ndk\aarch64-linux-android24-clang.cmd"
$env:AR_aarch64_linux_android = "$ndk\llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$ndk\aarch64-linux-android24-clang.cmd"

Set-Location "$repo\src-tauri"
# フロントエンド(src/)はRustの中へ埋め込まれる。画面だけを変えた場合も、Rustの再ビルドが走るよう更新日時を進める。
# The frontend is embedded into the Rust library; bump mtimes so a frontend-only change triggers a rebuild.
foreach ($f in "src\lib.rs", "..\src\index.html", "..\src\main.js", "..\src\ux.js", "..\src\styles.css") {
    if (Test-Path $f) { (Get-Item $f).LastWriteTime = Get-Date }
}
cargo build --lib --target aarch64-linux-android --features tauri/custom-protocol
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Set-Location "gen\android"
New-Item -ItemType Directory -Force "app\src\main\jniLibs\arm64-v8a" | Out-Null
Copy-Item "..\..\target\aarch64-linux-android\debug\libmake_disk_lib.so" "app\src\main\jniLibs\arm64-v8a\" -Force
.\gradlew.bat assembleUniversalDebug "-PabiList=arm64-v8a" "-PtargetList=aarch64" -x rustBuildArm64Debug
if ($LASTEXITCODE -ne 0) { throw "gradle failed" }
$apk = "app\build\outputs\apk\universal\debug\app-universal-debug.apk"
Write-Host "APK: $((Resolve-Path $apk).Path)"
if ($Install) {
    # 署名が違う版が入っている場合は、先にアンインストールが必要(INSTALL_FAILED_UPDATE_INCOMPATIBLE)。
    & "$sdk\platform-tools\adb.exe" install -r $apk
}
