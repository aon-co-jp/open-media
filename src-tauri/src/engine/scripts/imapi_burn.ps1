param([string]$IsoPath,[string]$Drive)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Runtime.InteropServices.ComTypes;
public static class NativeStream {
  [DllImport("shlwapi.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
  private static extern void SHCreateStreamOnFileEx(string path, uint mode, uint attrs, [MarshalAs(UnmanagedType.Bool)] bool create, IntPtr templ, out IStream stream);
  public static IStream Open(string path) { IStream s; SHCreateStreamOnFileEx(path, 0, 0x80, false, IntPtr.Zero, out s); return s; }
}
"@
try {
  $master = New-Object -ComObject IMAPI2.MsftDiscMaster2
  $recorder = $null
  for ($i = 0; $i -lt $master.Count; $i++) {
    $r = New-Object -ComObject IMAPI2.MsftDiscRecorder2
    $r.InitializeDiscRecorder($master.Item($i))
    if ($r.VolumePathNames -contains ($Drive.TrimEnd('\') + '\')) { $recorder = $r; break }
  }
  if ($null -eq $recorder) { throw "drive $Drive was not found via IMAPI2" }
  $fmt = New-Object -ComObject IMAPI2.MsftDiscFormat2Data
  if (-not $fmt.IsRecorderSupported($recorder)) { throw "the recorder does not support data writing" }
  $fmt.Recorder = $recorder
  $fmt.ClientName = 'make-disk'
  if (-not $fmt.IsCurrentMediaSupported($recorder)) { throw "no writable blank media is loaded (or the media is not supported/already written)" }
  if (-not $fmt.MediaHeuristicallyBlank) { throw "the loaded disc is not blank" }
  $fmt.ForceMediaToBeClosed = $true
  $stream = [NativeStream]::Open($IsoPath)
  $fmt.Write($stream)
  $recorder.EjectMedia()
  Write-Output "burn-ok"
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
