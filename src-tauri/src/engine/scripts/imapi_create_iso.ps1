param([string]$SourceDir,[string]$OutIso,[string]$Label)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System; using System.IO; using System.Runtime.InteropServices; using System.Runtime.InteropServices.ComTypes;
public static class IsoWriter {
  public static void Write(object comStream, string path) {
    IStream s = (IStream)comStream; System.Runtime.InteropServices.ComTypes.STATSTG st; s.Stat(out st, 1);
    long total = st.cbSize; byte[] buf = new byte[1048576]; IntPtr pRead = Marshal.AllocHGlobal(4);
    try { using (FileStream fs = File.Create(path)) { long done = 0; while (done < total) { int want = (int)Math.Min(buf.Length, total - done); s.Read(buf, want, pRead); int got = Marshal.ReadInt32(pRead); if (got <= 0) break; fs.Write(buf, 0, got); done += got; } } }
    finally { Marshal.FreeHGlobal(pRead); }
  }
}
"@
$img = New-Object -ComObject IMAPI2FS.MsftFileSystemImage
$img.FileSystemsToCreate = 3   # ISO9660 + Joliet
$img.VolumeName = $Label
$img.Root.AddTree($SourceDir, $false)
# Exclude the output ISO itself when it lives inside the source folder (left over from a previous run)
try { if (([IO.Path]::GetFullPath((Split-Path $OutIso -Parent)).TrimEnd([char]92) -ieq [IO.Path]::GetFullPath($SourceDir).TrimEnd([char]92))) { $img.Root.Remove((Split-Path $OutIso -Leaf)) } } catch { }
$res = $img.CreateResultImage()
$tmpIso = $OutIso + ".tmp"
[IsoWriter]::Write($res.ImageStream, $tmpIso)
$blocks = $res.TotalBlocks
$res = $null; $img = $null
[GC]::Collect(); [GC]::WaitForPendingFinalizers()
Move-Item -LiteralPath $tmpIso -Destination $OutIso -Force
"blocks=$blocks"
