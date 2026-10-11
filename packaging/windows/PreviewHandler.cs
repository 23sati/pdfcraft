// Linkco PDF Editor — Windows File Explorer PDF Preview Handler (IPreviewHandler) and
// PDF thumbnail provider (IThumbnailProvider, CLSID {3D8CDE4B-E969-481F-BEB0-5E3B98287416})
// Publisher: Al Rawabet Commercial Services & Contracting Company W.L.L. (Linkco — www.linkco.com.qa)
//
// Compiled into LinkcoPdfPreviewHandler.dll during the Windows build/packaging with the OS-included
// .NET Framework 4.x compiler (C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe, C# 5):
//
//   csc /nologo /target:library /optimize+ /platform:anycpu /codepage:65001 /out:LinkcoPdfPreviewHandler.dll
//       /r:System.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll PreviewHandler.cs
//
// Hosted out-of-process by its own instance of Windows' Preview Host (prevhost.exe as the
// DllSurrogate of AppID {1C0ADB7B-F017-4384-BF73-95AA5BC5D084}, as Microsoft requires for managed
// handlers; not the shared {6d2b5079-2f0b-48dd-ab7f-97cec514d30b}, where other vendors' handlers
// and runtimes live). Page rasterization is delegated to pdfcraft-cli.exe, which ships beside this DLL:
//
//   pdfcraft-cli preview <file.pdf> --page N --width PX --max-px 4096 --out <temp.png>
//
// so a damaged or hostile PDF can only ever crash or hang a short-lived child process (which is
// killed after a timeout), never Explorer or the preview host. The main Linkco PDF Editor window is
// never opened.
//
// Production notes:
//  * Rendering runs on a worker thread; the preview pane never blocks while a page renders.
//  * Selecting another file, paging quickly or closing the pane cancels (kills) the render in flight.
//  * The raster is sized to the pane width × zoom (DPI-aware) and re-rendered when the pane grows.
//  * The Explorer-provided stream is copied to a private temp file and released immediately, so the
//    previewed file is never kept locked (rename/delete/move keep working while it is previewed).
//  * Registration is per-user (HKCU, no administrator rights needed) and optionally machine-wide
//    (HKLM); it never touches ProgIDs shared with non-PDF file types (e.g. browsers' HTML ProgIDs).
//  * Thumbnails: LinkcoPdfThumbnailProvider renders page 1 the same way for Explorer's icon views.
//    It is registered on SystemFileAssociations\.pdf (consulted last) and Linkco's own ProgIDs, so
//    another PDF app's thumbnail provider keeps priority. Windows runs it in its isolated thumbnail
//    process (dllhost.exe).
//  * Uninstall: UnregisterPreviewHandlerForAllUsers() removes the per-user registration from every
//    user profile on the PC (mounting the hives of users who aren't signed in), restoring each
//    user's previous handlers, so no account is left pointing Explorer at the deleted DLL.
//  * Diagnostics: %USERPROFILE%\AppData\LocalLow\LinkcoPdfPreview\preview.log (size-capped), and
//    [LinkcoPdfPreview.LinkcoPdfPreviewHandler]::Diagnose() from PowerShell.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("Linkco PDF Preview Handler")]
[assembly: AssemblyDescription("Windows File Explorer PDF Preview Handler for Linkco PDF Editor")]
[assembly: AssemblyCompany("Al Rawabet Commercial Services & Contracting Company W.L.L.")]
[assembly: AssemblyProduct("Linkco PDF Editor")]
[assembly: AssemblyCopyright("\u00A9 Al Rawabet Commercial Services & Contracting Company W.L.L.")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]
[assembly: ComVisible(true)]

namespace LinkcoPdfPreview
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int left;
        public int top;
        public int right;
        public int bottom;

        public int Width { get { return right - left; } }
        public int Height { get { return bottom - top; } }
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MSG
    {
        public IntPtr hwnd;
        public uint message;
        public IntPtr wParam;
        public IntPtr lParam;
        public uint time;
        public int pt_x;
        public int pt_y;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct LOGFONT
    {
        public int lfHeight;
        public int lfWidth;
        public int lfEscapement;
        public int lfOrientation;
        public int lfWeight;
        public byte lfItalic;
        public byte lfUnderline;
        public byte lfStrikeOut;
        public byte lfCharSet;
        public byte lfOutPrecision;
        public byte lfClipPrecision;
        public byte lfQuality;
        public byte lfPitchAndFamily;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string lfFaceName;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct PREVIEWHANDLERFRAMEINFO
    {
        public IntPtr haccel;
        public uint cAccelEntries;
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("8895b1c6-b41f-4c1c-a562-0d564250836f")]
    public interface IPreviewHandler
    {
        void SetWindow(IntPtr hwnd, ref RECT rect);
        void SetRect(ref RECT rect);
        void DoPreview();
        void Unload();
        void SetFocus();
        void QueryFocus(out IntPtr phwnd);
        [PreserveSig]
        int TranslateAccelerator(ref MSG pmsg);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("196bf9a5-b346-4ef0-aa1e-5dcdb76768b1")]
    public interface IPreviewHandlerVisuals
    {
        void SetBackgroundColor(uint color);
        void SetFont(ref LOGFONT plf);
        void SetTextColor(uint color);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("fec87aaf-35f9-447a-adb7-20234491401a")]
    public interface IPreviewHandlerFrame
    {
        void GetWindowContext(out PREVIEWHANDLERFRAMEINFO pinfo);
        [PreserveSig]
        int TranslateAccelerator(ref MSG pmsg);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("b7d14566-0509-4cce-a71f-0a554233bd9b")]
    public interface IInitializeWithFile
    {
        void Initialize([MarshalAs(UnmanagedType.LPWStr)] string pszFilePath, uint grfMode);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("b824b49d-22ac-4161-ac8a-9916e8fa3f7f")]
    public interface IInitializeWithStream
    {
        void Initialize(IStream pstream, uint grfMode);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("00000114-0000-0000-C000-000000000046")]
    public interface IOleWindow
    {
        void GetWindow(out IntPtr phwnd);
        void ContextSensitiveHelp([MarshalAs(UnmanagedType.Bool)] bool fEnterMode);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("fc4801a3-2ba9-11cf-a229-00aa003d7352")]
    public interface IObjectWithSite
    {
        void SetSite([MarshalAs(UnmanagedType.IUnknown)] object pUnkSite);
        void GetSite(ref Guid riid, [MarshalAs(UnmanagedType.IUnknown)] out object ppvSite);
    }

    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("e357fccd-a995-4576-b01f-234630154e96")]
    public interface IThumbnailProvider
    {
        /// <summary>pdwAlpha is a WTS_ALPHATYPE: 0 unknown, 1 RGB, 2 ARGB.</summary>
        [PreserveSig]
        int GetThumbnail(uint cx, out IntPtr phbmp, out uint pdwAlpha);
    }

    /// <summary>
    /// URL Security Zones (urlmon): which zone File Explorer puts a file in. Used only to report why the
    /// preview pane is blank for a file (see <c>LinkcoPdfPreviewHandler.DiagnoseFile</c>).
    /// </summary>
    [ComImport]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    [Guid("79eac9ee-baf9-11ce-8c82-00aa004ba90b")]
    internal interface IInternetSecurityManager
    {
        [PreserveSig]
        int SetSecuritySite(IntPtr pSite);
        [PreserveSig]
        int GetSecuritySite(out IntPtr ppSite);
        [PreserveSig]
        int MapUrlToZone([MarshalAs(UnmanagedType.LPWStr)] string pwszUrl, out int pdwZone, int dwFlags);
    }

    /// <summary>CLSID_InternetSecurityManager.</summary>
    [ComImport]
    [Guid("7b8a2d94-0ac9-11d1-896c-00c04fb6bfc4")]
    internal class InternetSecurityManager
    {
    }

    internal static class NativeMethods
    {
        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr SetParent(IntPtr hWndChild, IntPtr hWndNewParent);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

        [DllImport("user32.dll")]
        public static extern IntPtr GetFocus();

        [DllImport("user32.dll", EntryPoint = "GetWindowLong")]
        private static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "GetWindowLongPtr")]
        private static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "SetWindowLong")]
        private static extern int SetWindowLong32(IntPtr hWnd, int nIndex, int dwNewLong);

        [DllImport("user32.dll", EntryPoint = "SetWindowLongPtr")]
        private static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

        [DllImport("user32.dll")]
        private static extern uint GetDpiForWindow(IntPtr hwnd);

        [DllImport("shell32.dll")]
        public static extern void SHChangeNotify(int wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);

        [DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
        private static extern int AssocQueryString(uint flags, int str, string pszAssoc, string pszExtra, StringBuilder pszOut, ref uint pcchOut);

        [DllImport("mpr.dll", CharSet = CharSet.Unicode, EntryPoint = "WNetGetConnectionW")]
        private static extern int WNetGetConnection(string lpLocalName, StringBuilder lpRemoteName, ref int lpnLength);

        /// <summary>The share a mapped network drive ("Z:") points at (\\server\share), or null.</summary>
        public static string MappedDriveTarget(string drive)
        {
            try
            {
                int length = 1024;
                StringBuilder remote = new StringBuilder(length);
                return WNetGetConnection(drive, remote, ref length) == 0 && remote.Length > 0 ? remote.ToString() : null;
            }
            catch
            {
                return null;
            }
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool DeleteFileW(string lpFileName);

        public const int GWL_STYLE = -16;
        public const uint WS_CHILD = 0x40000000;
        public const uint WS_POPUP = 0x80000000;
        public const uint WS_CAPTION = 0x00C00000;
        public const uint WS_THICKFRAME = 0x00040000;
        public const uint WS_VISIBLE = 0x10000000;
        public const uint WS_CLIPCHILDREN = 0x02000000;
        public const uint WS_CLIPSIBLINGS = 0x04000000;

        public const uint SWP_NOZORDER = 0x0004;
        public const uint SWP_NOACTIVATE = 0x0010;
        public const uint SWP_SHOWWINDOW = 0x0040;

        public const int SW_SHOW = 5;

        public const int SHCNE_ASSOCCHANGED = 0x08000000;
        public const uint SHCNF_IDLIST = 0x0000;
        public const int E_FAIL = unchecked((int)0x80004005);
        public const int S_OK = 0;
        public const int S_FALSE = 1;
        public const int ASSOCSTR_SHELLEXTENSION = 16;
        public const int WM_DPICHANGED_AFTERPARENT = 0x02E3;

        /// <summary>Makes a window a borderless child window (needed after SetParent across processes).</summary>
        public static void MakeChildWindow(IntPtr hwnd)
        {
            uint style;
            if (IntPtr.Size == 8)
                style = unchecked((uint)GetWindowLongPtr64(hwnd, GWL_STYLE).ToInt64());
            else
                style = unchecked((uint)GetWindowLong32(hwnd, GWL_STYLE));

            style &= ~(WS_POPUP | WS_CAPTION | WS_THICKFRAME);
            style |= WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;

            if (IntPtr.Size == 8)
                SetWindowLongPtr64(hwnd, GWL_STYLE, new IntPtr((long)style));
            else
                SetWindowLong32(hwnd, GWL_STYLE, unchecked((int)style));
        }

        /// <summary>The DPI of a window (Windows 10 1607+), or 0 when the API is unavailable.</summary>
        public static int GetWindowDpi(IntPtr hwnd)
        {
            try
            {
                uint dpi = GetDpiForWindow(hwnd);
                return dpi > 0 ? (int)dpi : 0;
            }
            catch (EntryPointNotFoundException)
            {
                return 0;
            }
            catch (DllNotFoundException)
            {
                return 0;
            }
        }

        /// <summary>The preview handler CLSID Windows actually resolves for an extension, or null.</summary>
        public static string QueryShellExtension(string extension, string category)
        {
            try
            {
                uint size = 260;
                StringBuilder sb = new StringBuilder((int)size);
                int hr = AssocQueryString(0, ASSOCSTR_SHELLEXTENSION, extension, category, sb, ref size);
                return hr == S_OK ? sb.ToString() : null;
            }
            catch
            {
                return null;
            }
        }

        /// <summary>Removes the "downloaded from the Internet" mark (Zone.Identifier stream).</summary>
        public static void RemoveZoneIdentifier(string path)
        {
            try { DeleteFileW(path + ":Zone.Identifier"); } catch { }
        }
    }

    /// <summary>Temp folder, CLI discovery and stale-file housekeeping shared by the handler.</summary>
    internal static class PreviewEnvironment
    {
        private static readonly object Gate = new object();
        private static string _tempDir;
        private static bool _swept;

        public static string TempDir
        {
            get
            {
                lock (Gate)
                {
                    if (_tempDir == null || !Directory.Exists(_tempDir))
                        _tempDir = ComputeWritableTempDir();
                    return _tempDir;
                }
            }
        }

        private static string ComputeWritableTempDir()
        {
            List<string> candidates = new List<string>();
            // LocalLow first: writable from both Low and Medium integrity levels, so it works whether
            // or not prevhost.exe runs in its Low-IL sandbox.
            try
            {
                string userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                if (!string.IsNullOrEmpty(userProfile))
                    candidates.Add(Path.Combine(userProfile, @"AppData\LocalLow\LinkcoPdfPreview"));
            }
            catch { }

            try
            {
                string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                if (!string.IsNullOrEmpty(localAppData))
                {
                    string parent = Path.GetDirectoryName(localAppData);
                    if (!string.IsNullOrEmpty(parent))
                        candidates.Add(Path.Combine(parent, @"LocalLow\LinkcoPdfPreview"));
                }
            }
            catch { }

            try
            {
                string sysTemp = Path.GetTempPath();
                if (!string.IsNullOrEmpty(sysTemp))
                {
                    candidates.Add(Path.Combine(sysTemp, @"Low\LinkcoPdfPreview"));
                    candidates.Add(Path.Combine(sysTemp, "LinkcoPdfPreview"));
                }
            }
            catch { }

            foreach (string dir in candidates)
            {
                try
                {
                    Directory.CreateDirectory(dir);
                    string probe = Path.Combine(dir, ".probe-" + Guid.NewGuid().ToString("N"));
                    File.WriteAllBytes(probe, new byte[] { 1 });
                    File.Delete(probe);
                    return dir;
                }
                catch { }
            }

            return Path.GetTempPath();
        }

        /// <summary>Once per process: deletes temp copies/rasters left behind by a killed preview host.</summary>
        public static void SweepStaleFilesOnce()
        {
            lock (Gate)
            {
                if (_swept)
                    return;
                _swept = true;
            }

            ThreadPool.QueueUserWorkItem(delegate
            {
                try
                {
                    DateTime cutoff = DateTime.UtcNow - TimeSpan.FromHours(6);
                    foreach (string f in Directory.GetFiles(TempDir))
                    {
                        string name = Path.GetFileName(f);
                        if (!(name.StartsWith("stream-", StringComparison.Ordinal) ||
                              name.StartsWith("page-", StringComparison.Ordinal) ||
                              name.StartsWith("thumb-", StringComparison.Ordinal) ||
                              name.StartsWith("selftest-", StringComparison.Ordinal) ||
                              name.StartsWith(".probe-", StringComparison.Ordinal)))
                            continue;
                        try
                        {
                            if (File.GetLastWriteTimeUtc(f) < cutoff)
                                File.Delete(f);
                        }
                        catch { }
                    }
                }
                catch { }
            });
        }

        /// <summary>Deletes a file now, or retries in the background while a killed process lets go of it.</summary>
        public static void DeleteFileSoon(string path)
        {
            if (string.IsNullOrEmpty(path))
                return;
            try
            {
                if (!File.Exists(path))
                    return;
                File.Delete(path);
                return;
            }
            catch { }

            ThreadPool.QueueUserWorkItem(delegate
            {
                for (int i = 0; i < 20; i++)
                {
                    try
                    {
                        if (!File.Exists(path))
                            return;
                        File.Delete(path);
                        return;
                    }
                    catch { }
                    Thread.Sleep(250);
                }
            });
        }

        /// <summary>
        /// Copies an Explorer-provided stream to a private temp file (named prefix + GUID + ".pdf") and
        /// releases the stream right away, so the previewed file is never kept locked. Throws on failure
        /// (after deleting the partial copy).
        /// </summary>
        public static string CopyStreamToTempFile(IStream pstream, string prefix, out string displayName)
        {
            displayName = null;
            if (pstream == null)
                throw new ArgumentNullException("pstream");
            string tempFile = Path.Combine(TempDir, prefix + Guid.NewGuid().ToString("N") + ".pdf");
            try
            {
                try
                {
                    System.Runtime.InteropServices.ComTypes.STATSTG stat;
                    pstream.Stat(out stat, 0);
                    if (!string.IsNullOrEmpty(stat.pwcsName))
                        displayName = Path.GetFileName(stat.pwcsName);
                }
                catch { }

                try { pstream.Seek(0, 0, IntPtr.Zero); } catch { }

                IntPtr bytesReadPtr = Marshal.AllocCoTaskMem(8);
                try
                {
                    using (FileStream fs = new FileStream(tempFile, FileMode.Create, FileAccess.Write, FileShare.Read, 1 << 16))
                    {
                        byte[] buffer = new byte[1 << 20];
                        while (true)
                        {
                            Marshal.WriteInt64(bytesReadPtr, 0);
                            pstream.Read(buffer, buffer.Length, bytesReadPtr);
                            int read = Marshal.ReadInt32(bytesReadPtr);
                            if (read <= 0)
                                break;
                            fs.Write(buffer, 0, read);
                        }
                    }
                }
                finally
                {
                    Marshal.FreeCoTaskMem(bytesReadPtr);
                }
                return tempFile;
            }
            catch
            {
                DeleteFileSoon(tempFile);
                throw;
            }
            finally
            {
                try { Marshal.ReleaseComObject(pstream); } catch { }
            }
        }

        public static RegistryKey OpenHive(RegistryHive hive)
        {
            RegistryView view = Environment.Is64BitOperatingSystem ? RegistryView.Registry64 : RegistryView.Default;
            return RegistryKey.OpenBaseKey(hive, view);
        }

        public static string LocateCliExecutable()
        {
            List<string> dirs = new List<string>();
            try
            {
                string asmLoc = Assembly.GetExecutingAssembly().Location;
                if (!string.IsNullOrEmpty(asmLoc))
                    dirs.Add(Path.GetDirectoryName(asmLoc));
            }
            catch { }

            try
            {
                string codeBase = Assembly.GetExecutingAssembly().CodeBase;
                if (!string.IsNullOrEmpty(codeBase))
                    dirs.Add(Path.GetDirectoryName(new Uri(codeBase).LocalPath));
            }
            catch { }

            foreach (string dir in dirs)
            {
                try
                {
                    if (string.IsNullOrEmpty(dir))
                        continue;
                    string candidate = Path.Combine(dir, "pdfcraft-cli.exe");
                    if (File.Exists(candidate))
                        return candidate;
                }
                catch { }
            }

            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
            {
                try
                {
                    using (RegistryKey root = OpenHive(hive))
                    using (RegistryKey k = root.OpenSubKey(LinkcoPdfPreviewHandler.LinkcoConfigKey, false))
                    {
                        if (k == null)
                            continue;
                        string cliPath = k.GetValue("CliPath") as string;
                        if (!string.IsNullOrEmpty(cliPath) && File.Exists(cliPath))
                            return cliPath;

                        string installDir = k.GetValue("InstallDir") as string;
                        if (!string.IsNullOrEmpty(installDir))
                        {
                            string candidate = Path.Combine(installDir, "pdfcraft-cli.exe");
                            if (File.Exists(candidate))
                                return candidate;
                        }
                    }
                }
                catch { }
            }

            string[] fallbacks = new string[]
            {
                SafeCombine(Environment.SpecialFolder.LocalApplicationData, @"Programs\Linkco\Linkco PDF Editor\pdfcraft-cli.exe"),
                SafeCombine(Environment.SpecialFolder.LocalApplicationData, @"Programs\Linkco PDF Editor\pdfcraft-cli.exe"),
                SafeCombine(Environment.SpecialFolder.ProgramFiles, @"Linkco\Linkco PDF Editor\pdfcraft-cli.exe"),
                SafeCombine(Environment.SpecialFolder.ProgramFiles, @"Linkco PDF Editor\pdfcraft-cli.exe")
            };
            foreach (string candidate in fallbacks)
            {
                try
                {
                    if (!string.IsNullOrEmpty(candidate) && File.Exists(candidate))
                        return candidate;
                }
                catch { }
            }

            return null;
        }

        private static string SafeCombine(Environment.SpecialFolder folder, string relative)
        {
            try
            {
                string baseDir = Environment.GetFolderPath(folder);
                return string.IsNullOrEmpty(baseDir) ? null : Path.Combine(baseDir, relative);
            }
            catch
            {
                return null;
            }
        }
    }

    /// <summary>Small, size-capped diagnostic log in the preview temp folder (no document contents).</summary>
    /// <summary>
    /// A registry root the registration code writes to: HKCU, HKLM or another user's profile. Paths
    /// are relative to the hive root ("Software\Classes\...", "Software\Linkco\..."). For a user
    /// profile whose classes live in a separately loaded hive (UsrClass.dat, mounted next to
    /// NTUSER.DAT) "Software\Classes\..." is routed to that hive, exactly as Windows maps
    /// HKCU\Software\Classes for a logged-on user. Does not own the keys it wraps.
    /// </summary>
    internal sealed class RegRoot
    {
        private const string ClassesPrefix = @"Software\Classes";
        private readonly RegistryKey _root;
        private readonly RegistryKey _classes;

        public RegRoot(RegistryKey root) : this(root, null)
        {
        }

        public RegRoot(RegistryKey root, RegistryKey classes)
        {
            if (root == null)
                throw new ArgumentNullException("root");
            _root = root;
            _classes = classes;
        }

        public static implicit operator RegRoot(RegistryKey root)
        {
            return root == null ? null : new RegRoot(root);
        }

        public string Name
        {
            get { return _root.Name; }
        }

        /// <summary>The key a path lives in, and the path relative to it.</summary>
        private RegistryKey Route(string path, out string rest)
        {
            if (_classes != null && path.StartsWith(ClassesPrefix, StringComparison.OrdinalIgnoreCase) &&
                path.Length > ClassesPrefix.Length && path[ClassesPrefix.Length] == '\\')
            {
                rest = path.Substring(ClassesPrefix.Length + 1);
                return _classes;
            }
            rest = path;
            return _root;
        }

        public RegistryKey OpenSubKey(string path)
        {
            return OpenSubKey(path, false);
        }

        public RegistryKey OpenSubKey(string path, bool writable)
        {
            string rest;
            return Route(path, out rest).OpenSubKey(rest, writable);
        }

        public RegistryKey CreateSubKey(string path)
        {
            string rest;
            return Route(path, out rest).CreateSubKey(rest);
        }

        public void DeleteSubKey(string path, bool throwOnMissingSubKey)
        {
            string rest;
            Route(path, out rest).DeleteSubKey(rest, throwOnMissingSubKey);
        }

        public void DeleteSubKeyTree(string path, bool throwOnMissingSubKey)
        {
            string rest;
            Route(path, out rest).DeleteSubKeyTree(rest, throwOnMissingSubKey);
        }

        /// <summary>The names directly under Software\Classes (extensions, ProgIDs, CLSID, ...).</summary>
        public string[] ClassesSubKeyNames()
        {
            if (_classes != null)
                return _classes.GetSubKeyNames();
            using (RegistryKey c = _root.OpenSubKey(ClassesPrefix, false))
            {
                return c == null ? new string[0] : c.GetSubKeyNames();
            }
        }
    }

    /// <summary>
    /// Visits the registry of every user profile on this PC, so an elevated uninstaller can remove
    /// what Linkco PDF Editor registered for each user (it registers per user, for each user who
    /// starts it). Logged-on users' hives are already loaded under HKEY_USERS; the others are
    /// mounted from NTUSER.DAT and UsrClass.dat for the visit and unmounted right after. Mounting
    /// needs the backup and restore privileges, i.e. an elevated administrator or LocalSystem.
    /// </summary>
    internal static class UserProfileHives
    {
        private const string ProfileListKey = @"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
        private static readonly IntPtr HKEY_USERS = new IntPtr(unchecked((int)0x80000003));
        private const uint TOKEN_ADJUST_PRIVILEGES = 0x0020;
        private const uint TOKEN_QUERY = 0x0008;
        private const int SE_PRIVILEGE_ENABLED = 0x00000002;
        private const int ERROR_NOT_ALL_ASSIGNED = 1300;

        [StructLayout(LayoutKind.Sequential, Pack = 4)]
        private struct TokenPrivilege
        {
            public int Count;
            public long Luid;
            public int Attributes;
        }

        [DllImport("advapi32.dll", EntryPoint = "RegLoadKeyW", CharSet = CharSet.Unicode, ExactSpelling = true)]
        private static extern int RegLoadKey(IntPtr hKey, string lpSubKey, string lpFile);

        [DllImport("advapi32.dll", EntryPoint = "RegUnLoadKeyW", CharSet = CharSet.Unicode, ExactSpelling = true)]
        private static extern int RegUnLoadKey(IntPtr hKey, string lpSubKey);

        [DllImport("kernel32.dll")]
        private static extern IntPtr GetCurrentProcess();

        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr handle);

        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool OpenProcessToken(IntPtr process, uint desiredAccess, out IntPtr token);

        [DllImport("advapi32.dll", EntryPoint = "LookupPrivilegeValueW", CharSet = CharSet.Unicode, ExactSpelling = true, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool LookupPrivilegeValue(string systemName, string name, out long luid);

        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AdjustTokenPrivileges(IntPtr token, [MarshalAs(UnmanagedType.Bool)] bool disableAll,
            ref TokenPrivilege newState, int bufferLength, IntPtr previousState, IntPtr returnLength);

        /// <summary>Real people: local and domain accounts (S-1-5-21-...) and Microsoft Entra ID accounts (S-1-12-1-...).</summary>
        internal static bool IsUserSid(string sid)
        {
            if (string.IsNullOrEmpty(sid) || !(sid.StartsWith("S-1-5-21-", StringComparison.Ordinal) || sid.StartsWith("S-1-12-1-", StringComparison.Ordinal)))
                return false;
            foreach (char c in sid)
            {
                if (!(c == '-' || c == 'S' || (c >= '0' && c <= '9')))
                    return false; // e.g. "S-1-5-21-...-1001.bak", a profile Windows set aside
            }
            return true;
        }

        private static bool EnablePrivilege(string name)
        {
            IntPtr token;
            if (!OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, out token))
                return false;
            try
            {
                TokenPrivilege tp = new TokenPrivilege();
                if (!LookupPrivilegeValue(null, name, out tp.Luid))
                    return false;
                tp.Count = 1;
                tp.Attributes = SE_PRIVILEGE_ENABLED;
                if (!AdjustTokenPrivileges(token, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero))
                    return false;
                return Marshal.GetLastWin32Error() != ERROR_NOT_ALL_ASSIGNED;
            }
            finally
            {
                CloseHandle(token);
            }
        }

        /// <summary>
        /// Calls <paramref name="visit"/> once per user profile with that user's registry. Returns
        /// false when profiles that aren't loaded can't be mounted (not elevated); loaded ones are
        /// still visited when the process may open them.
        /// </summary>
        public static bool ForEach(Action<string, RegRoot> visit)
        {
            List<KeyValuePair<string, string>> profiles = new List<KeyValuePair<string, string>>();
            using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
            using (RegistryKey list = hklm.OpenSubKey(ProfileListKey, false))
            {
                if (list != null)
                {
                    foreach (string sid in list.GetSubKeyNames())
                    {
                        if (!IsUserSid(sid))
                            continue;
                        using (RegistryKey p = list.OpenSubKey(sid, false))
                        {
                            string dir = p == null ? null : p.GetValue("ProfileImagePath") as string;
                            profiles.Add(new KeyValuePair<string, string>(sid, dir ?? ""));
                        }
                    }
                }
            }

            bool canMount = EnablePrivilege("SeRestorePrivilege") && EnablePrivilege("SeBackupPrivilege");
            using (RegistryKey users = PreviewEnvironment.OpenHive(RegistryHive.Users))
            {
                foreach (KeyValuePair<string, string> profile in profiles)
                {
                    string sid = profile.Key;
                    try
                    {
                        RegistryKey loaded = null;
                        try { loaded = users.OpenSubKey(sid, true); } catch (System.Security.SecurityException) { } catch (UnauthorizedAccessException) { }
                        if (loaded != null)
                        {
                            using (loaded)
                            using (RegistryKey classes = OpenOrNull(users, sid + "_Classes"))
                            {
                                visit(sid, new RegRoot(loaded, classes));
                            }
                            continue;
                        }
                        if (canMount)
                            VisitUnloaded(users, sid, profile.Value, visit);
                    }
                    catch (Exception ex)
                    {
                        PreviewLog.Error("User profile " + sid, ex);
                    }
                }
            }
            return canMount;
        }

        private static RegistryKey OpenOrNull(RegistryKey parent, string name)
        {
            try { return parent.OpenSubKey(name, true); } catch { return null; }
        }

        private static void VisitUnloaded(RegistryKey users, string sid, string profileDir, Action<string, RegRoot> visit)
        {
            if (string.IsNullOrEmpty(profileDir))
                return;
            string ntUser = Path.Combine(profileDir, "NTUSER.DAT");
            string usrClass = Path.Combine(profileDir, @"AppData\Local\Microsoft\Windows\UsrClass.dat");
            // Without a per-user classes hive the user has no per-user COM or shell registrations,
            // so there is nothing of ours to remove.
            if (!File.Exists(ntUser) || !File.Exists(usrClass))
                return;

            string mount = "LinkcoPdfCleanup-" + Guid.NewGuid().ToString("N").Substring(0, 12);
            string mountClasses = mount + "_Classes";
            int rc = RegLoadKey(HKEY_USERS, mount, ntUser);
            if (rc != 0)
            {
                PreviewLog.Write("Could not mount the registry of " + sid + " (error " + rc.ToString(CultureInfo.InvariantCulture) + "); skipped.");
                return;
            }
            bool classesMounted = false;
            try
            {
                rc = RegLoadKey(HKEY_USERS, mountClasses, usrClass);
                classesMounted = rc == 0;
                if (!classesMounted)
                {
                    PreviewLog.Write("Could not mount the classes registry of " + sid + " (error " + rc.ToString(CultureInfo.InvariantCulture) + "); skipped.");
                    return;
                }
                using (RegistryKey software = users.OpenSubKey(mount, true))
                using (RegistryKey classes = users.OpenSubKey(mountClasses, true))
                {
                    if (software != null && classes != null)
                        visit(sid, new RegRoot(software, classes));
                }
            }
            finally
            {
                if (classesMounted)
                    Unmount(mountClasses);
                Unmount(mount);
            }
        }

        private static void Unmount(string mount)
        {
            for (int attempt = 0; attempt < 5; attempt++)
            {
                // A handle still open anywhere in the process keeps the hive loaded.
                GC.Collect();
                GC.WaitForPendingFinalizers();
                int rc = RegUnLoadKey(HKEY_USERS, mount);
                if (rc == 0)
                    return;
                Thread.Sleep(100 * (attempt + 1));
            }
            PreviewLog.Write("Could not unmount HKEY_USERS\\" + mount + "; Windows unloads it at the next restart.");
        }
    }

    /// <summary>
    /// Runs pdfcraft-cli.exe for the preview pane and the thumbnail provider. It calls CreateProcess
    /// directly instead of using System.Diagnostics.Process so that:
    ///  * Windows never shows the spinning "app starting" cursor (STARTF_FORCEOFFFEEDBACK). The
    ///    engine is a hidden helper, and every selection in Explorer started one;
    ///  * stdout/stderr go to small temp files instead of pipes, so no wait can ever block on pipe
    ///    EOF (Process.WaitForExit() waits for EOF with no timeout, forever if any other process
    ///    inherited the pipe);
    ///  * the child runs in a kill-on-close job object, so it never outlives the preview host
    ///    (Explorer kills prevhost.exe when a preview takes too long).
    /// </summary>
    internal sealed class CliProcess : IDisposable
    {
        private const uint CREATE_SUSPENDED = 0x00000004;
        private const uint BELOW_NORMAL_PRIORITY_CLASS = 0x00004000;
        private const uint CREATE_NO_WINDOW = 0x08000000;
        private const int STARTF_USESHOWWINDOW = 0x00000001;
        private const int STARTF_FORCEOFFFEEDBACK = 0x00000080;
        private const int STARTF_USESTDHANDLES = 0x00000100;
        private const short SW_HIDE = 0;
        private const uint GENERIC_READ = 0x80000000;
        private const uint GENERIC_WRITE = 0x40000000;
        private const uint FILE_SHARE_ALL = 0x00000007;
        private const uint CREATE_ALWAYS = 2;
        private const uint OPEN_EXISTING = 3;
        private const uint FILE_ATTRIBUTE_TEMPORARY = 0x00000100;
        private const uint WAIT_OBJECT_0 = 0;
        private const uint WAIT_TIMEOUT = 0x00000102;
        private const int JobObjectExtendedLimitInformation = 9;
        private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
        private const int MaxOutputBytes = 64 * 1024;
        private static readonly IntPtr InvalidHandle = new IntPtr(-1);

        /// <summary>Our inheritable handles must not leak into another of our own concurrent spawns.</summary>
        private static readonly object SpawnGate = new object();

        [StructLayout(LayoutKind.Sequential)]
        private struct SECURITY_ATTRIBUTES
        {
            public int nLength;
            public IntPtr lpSecurityDescriptor;
            public int bInheritHandle;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct STARTUPINFO
        {
            public int cb;
            public IntPtr lpReserved;
            public IntPtr lpDesktop;
            public IntPtr lpTitle;
            public int dwX;
            public int dwY;
            public int dwXSize;
            public int dwYSize;
            public int dwXCountChars;
            public int dwYCountChars;
            public int dwFillAttribute;
            public int dwFlags;
            public short wShowWindow;
            public short cbReserved2;
            public IntPtr lpReserved2;
            public IntPtr hStdInput;
            public IntPtr hStdOutput;
            public IntPtr hStdError;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct PROCESS_INFORMATION
        {
            public IntPtr hProcess;
            public IntPtr hThread;
            public int dwProcessId;
            public int dwThreadId;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_BASIC_LIMIT_INFORMATION
        {
            public long PerProcessUserTimeLimit;
            public long PerJobUserTimeLimit;
            public uint LimitFlags;
            public UIntPtr MinimumWorkingSetSize;
            public UIntPtr MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass;
            public uint SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IO_COUNTERS
        {
            public ulong ReadOperationCount;
            public ulong WriteOperationCount;
            public ulong OtherOperationCount;
            public ulong ReadTransferCount;
            public ulong WriteTransferCount;
            public ulong OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
        {
            public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
            public IO_COUNTERS IoInfo;
            public UIntPtr ProcessMemoryLimit;
            public UIntPtr JobMemoryLimit;
            public UIntPtr PeakProcessMemoryUsed;
            public UIntPtr PeakJobMemoryUsed;
        }

        [DllImport("kernel32.dll", EntryPoint = "CreateProcessW", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CreateProcess(
            string lpApplicationName,
            StringBuilder lpCommandLine,
            IntPtr lpProcessAttributes,
            IntPtr lpThreadAttributes,
            [MarshalAs(UnmanagedType.Bool)] bool bInheritHandles,
            uint dwCreationFlags,
            IntPtr lpEnvironment,
            string lpCurrentDirectory,
            ref STARTUPINFO lpStartupInfo,
            out PROCESS_INFORMATION lpProcessInformation);

        [DllImport("kernel32.dll", EntryPoint = "CreateFileW", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateFile(string lpFileName, uint dwDesiredAccess, uint dwShareMode, ref SECURITY_ATTRIBUTES lpSecurityAttributes, uint dwCreationDisposition, uint dwFlagsAndAttributes, IntPtr hTemplateFile);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr hObject);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr hHandle, uint dwMilliseconds);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool TerminateProcess(IntPtr hProcess, uint uExitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetExitCodeProcess(IntPtr hProcess, out uint lpExitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint ResumeThread(IntPtr hThread);

        [DllImport("kernel32.dll", EntryPoint = "CreateJobObjectW", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateJobObject(IntPtr lpJobAttributes, string lpName);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetInformationJobObject(IntPtr hJob, int infoClass, ref JOBOBJECT_EXTENDED_LIMIT_INFORMATION info, uint cbInfo);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AssignProcessToJobObject(IntPtr hJob, IntPtr hProcess);

        private readonly object _gate = new object();
        private readonly string _stdoutPath;
        private readonly string _stderrPath;
        private IntPtr _process;
        private IntPtr _job;
        private int _pid;

        private CliProcess(string stdoutPath, string stderrPath)
        {
            _stdoutPath = stdoutPath;
            _stderrPath = stderrPath;
        }

        public int Id
        {
            get { return _pid; }
        }

        /// <summary>
        /// Starts <paramref name="exe"/> hidden, at below-normal priority, with stdout/stderr captured to
        /// temp files named after <paramref name="prefix"/>. Throws Win32Exception when it can't start.
        /// </summary>
        public static CliProcess Start(string exe, string arguments, string prefix)
        {
            string stem = Path.Combine(
                PreviewEnvironment.TempDir,
                prefix + "-" + Process.GetCurrentProcess().Id.ToString(CultureInfo.InvariantCulture) + "-" + Guid.NewGuid().ToString("N"));
            CliProcess cp = new CliProcess(stem + ".out", stem + ".err");
            IntPtr hIn = InvalidHandle;
            IntPtr hOut = InvalidHandle;
            IntPtr hErr = InvalidHandle;
            bool started = false;
            try
            {
                SECURITY_ATTRIBUTES sa = new SECURITY_ATTRIBUTES();
                sa.nLength = Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES));
                sa.bInheritHandle = 1;

                StringBuilder commandLine = new StringBuilder(PreviewPaneControl.QuoteArg(exe) + " " + arguments);
                STARTUPINFO si = new STARTUPINFO();
                si.cb = Marshal.SizeOf(typeof(STARTUPINFO));
                si.dwFlags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW | STARTF_FORCEOFFFEEDBACK;
                si.wShowWindow = SW_HIDE;

                PROCESS_INFORMATION pi;
                int error = 0;
                lock (SpawnGate)
                {
                    hIn = CreateFile("NUL", GENERIC_READ, FILE_SHARE_ALL, ref sa, OPEN_EXISTING, 0, IntPtr.Zero);
                    hOut = CreateFile(cp._stdoutPath, GENERIC_WRITE, FILE_SHARE_ALL, ref sa, CREATE_ALWAYS, FILE_ATTRIBUTE_TEMPORARY, IntPtr.Zero);
                    if (hOut == InvalidHandle)
                        throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                    hErr = CreateFile(cp._stderrPath, GENERIC_WRITE, FILE_SHARE_ALL, ref sa, CREATE_ALWAYS, FILE_ATTRIBUTE_TEMPORARY, IntPtr.Zero);
                    if (hErr == InvalidHandle)
                        throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                    si.hStdInput = hIn == InvalidHandle ? IntPtr.Zero : hIn;
                    si.hStdOutput = hOut;
                    si.hStdError = hErr;

                    started = CreateProcess(
                        exe,
                        commandLine,
                        IntPtr.Zero,
                        IntPtr.Zero,
                        true,
                        CREATE_NO_WINDOW | CREATE_SUSPENDED | BELOW_NORMAL_PRIORITY_CLASS,
                        IntPtr.Zero,
                        Path.GetDirectoryName(exe),
                        ref si,
                        out pi);
                    if (!started)
                        error = Marshal.GetLastWin32Error();
                    CloseQuietly(ref hIn);
                    CloseQuietly(ref hOut);
                    CloseQuietly(ref hErr);
                }
                if (!started)
                    throw new System.ComponentModel.Win32Exception(error);

                cp._process = pi.hProcess;
                cp._pid = pi.dwProcessId;
                cp._job = CreateKillOnCloseJob(pi.hProcess);
                ResumeThread(pi.hThread);
                CloseHandle(pi.hThread);
                return cp;
            }
            catch
            {
                CloseQuietly(ref hIn);
                CloseQuietly(ref hOut);
                CloseQuietly(ref hErr);
                cp.Dispose();
                throw;
            }
        }

        /// <summary>Puts the (still suspended) child in a job that kills it when the last handle closes.</summary>
        private static IntPtr CreateKillOnCloseJob(IntPtr process)
        {
            IntPtr job = IntPtr.Zero;
            try
            {
                job = CreateJobObject(IntPtr.Zero, null);
                if (job == IntPtr.Zero)
                    return IntPtr.Zero;
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION info = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                uint size = (uint)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
                if (SetInformationJobObject(job, JobObjectExtendedLimitInformation, ref info, size) && AssignProcessToJobObject(job, process))
                    return job;
                // Optional hardening only: e.g. a host job that forbids nesting on old Windows.
                CloseHandle(job);
                return IntPtr.Zero;
            }
            catch
            {
                if (job != IntPtr.Zero)
                    CloseHandle(job);
                return IntPtr.Zero;
            }
        }

        private static void CloseQuietly(ref IntPtr handle)
        {
            if (handle != IntPtr.Zero && handle != InvalidHandle)
            {
                try { CloseHandle(handle); } catch { }
            }
            handle = InvalidHandle;
        }

        /// <summary>True once the process has exited (or can no longer be waited on); false on timeout.</summary>
        public bool WaitForExit(int milliseconds)
        {
            IntPtr h;
            lock (_gate)
                h = _process;
            if (h == IntPtr.Zero)
                return true;
            uint r = WaitForSingleObject(h, (uint)Math.Max(0, milliseconds));
            return r != WAIT_TIMEOUT;
        }

        /// <summary>The exit code, or null while running / unknown.</summary>
        public uint? ExitCode
        {
            get
            {
                lock (_gate)
                {
                    uint code;
                    if (_process == IntPtr.Zero || !GetExitCodeProcess(_process, out code) || code == 259 /* STILL_ACTIVE */)
                        return null;
                    return code;
                }
            }
        }

        /// <summary>Terminates the process if it is still running. Safe from any thread, any number of times.</summary>
        public void Kill()
        {
            lock (_gate)
            {
                if (_process == IntPtr.Zero)
                    return;
                try
                {
                    if (WaitForSingleObject(_process, 0) == WAIT_TIMEOUT)
                        TerminateProcess(_process, 1);
                }
                catch { }
            }
        }

        public string ReadStdout()
        {
            return ReadCapped(_stdoutPath);
        }

        public string ReadStderr()
        {
            return ReadCapped(_stderrPath);
        }

        private static string ReadCapped(string path)
        {
            try
            {
                using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                {
                    int length = (int)Math.Min(fs.Length, MaxOutputBytes);
                    byte[] buffer = new byte[length];
                    int read = 0;
                    while (read < length)
                    {
                        int n = fs.Read(buffer, read, length - read);
                        if (n <= 0)
                            break;
                        read += n;
                    }
                    return Encoding.UTF8.GetString(buffer, 0, read);
                }
            }
            catch
            {
                return "";
            }
        }

        public void Dispose()
        {
            lock (_gate)
            {
                if (_job != IntPtr.Zero)
                {
                    // KILL_ON_JOB_CLOSE: a child still running at this point is terminated here.
                    try { CloseHandle(_job); } catch { }
                    _job = IntPtr.Zero;
                }
                else if (_process != IntPtr.Zero)
                {
                    try
                    {
                        if (WaitForSingleObject(_process, 0) == WAIT_TIMEOUT)
                            TerminateProcess(_process, 1);
                    }
                    catch { }
                }
                if (_process != IntPtr.Zero)
                {
                    try { CloseHandle(_process); } catch { }
                    _process = IntPtr.Zero;
                }
            }
            PreviewEnvironment.DeleteFileSoon(_stdoutPath);
            PreviewEnvironment.DeleteFileSoon(_stderrPath);
        }
    }

    internal static class PreviewLog
    {
        private static readonly object Gate = new object();
        private const long MaxBytes = 256 * 1024;

        public static void Write(string message)
        {
            try
            {
                string path = Path.Combine(PreviewEnvironment.TempDir, "preview.log");
                string line = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff", CultureInfo.InvariantCulture) +
                    " [" + Process.GetCurrentProcess().Id.ToString(CultureInfo.InvariantCulture) + "] " + message + Environment.NewLine;
                lock (Gate)
                {
                    FileInfo fi = new FileInfo(path);
                    if (fi.Exists && fi.Length > MaxBytes)
                    {
                        File.Copy(path, path + ".1", true);
                        File.Delete(path);
                    }
                    File.AppendAllText(path, line, Encoding.UTF8);
                }
            }
            catch { }
        }

        public static void Error(string where, Exception ex)
        {
            Write("ERROR " + where + ": " + (ex == null ? "(null)" : ex.GetType().Name + ": " + ex.Message));
        }

        private static int _crashLoggerInstalled;

        /// <summary>
        /// Records an exception that is about to end the host process (prevhost.exe / dllhost.exe),
        /// with its stack, so "an error in the Linkco PDF Preview Handler" can be traced. Logging only.
        /// </summary>
        public static void InstallCrashLogger()
        {
            if (Interlocked.Exchange(ref _crashLoggerInstalled, 1) != 0)
                return;
            try
            {
                AppDomain.CurrentDomain.UnhandledException += delegate(object sender, UnhandledExceptionEventArgs e)
                {
                    Exception ex = e.ExceptionObject as Exception;
                    string text = ex == null ? Convert.ToString(e.ExceptionObject, CultureInfo.InvariantCulture) : ex.ToString();
                    if (text != null && text.Length > 4000)
                        text = text.Substring(0, 4000);
                    Write("FATAL unhandled exception in " + Process.GetCurrentProcess().ProcessName + " (terminating=" + e.IsTerminating + "): " + text);
                };
            }
            catch { }
        }
    }

    [ComVisible(true)]
    [ClassInterface(ClassInterfaceType.None)]
    [ProgId("LinkcoPDFEditor.PreviewHandler")]
    [Guid(LinkcoPdfPreviewHandler.ClsidString)]
    public sealed class LinkcoPdfPreviewHandler :
        IPreviewHandler,
        IPreviewHandlerVisuals,
        IInitializeWithFile,
        IInitializeWithStream,
        IOleWindow,
        IObjectWithSite
    {
        public const string ClsidString = "D4E7B6A2-4C91-4E3A-9B12-7A8F5C3E1D20";
        public const string ClsidBraced = "{D4E7B6A2-4C91-4E3A-9B12-7A8F5C3E1D20}";
        public const string HandlerProgId = "LinkcoPDFEditor.PreviewHandler";
        public const string HandlerName = "Linkco PDF Preview Handler";
        public const string PreviewHandlerCategoryGuid = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";
        /// <summary>Windows' shared preview host. No longer used for this handler (see LinkcoAppId).</summary>
        public const string PrevHostAppId = "{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}";

        /// <summary>
        /// Linkco's own AppID, whose DllSurrogate is prevhost.exe: Explorer starts a dedicated
        /// prevhost.exe for this handler instead of the one shared by every vendor's handlers.
        /// Microsoft requires this for managed preview handlers ("Providing Your Own Process for a
        /// Preview Handler"): in the shared host another handler's runtime (.NET 8 handlers such as
        /// PowerToys', other CLR versions) or crash takes this one down with it, which Explorer
        /// reports as "an error in the Linkco PDF Preview Handler".
        /// </summary>
        public const string LinkcoAppId = "{1C0ADB7B-F017-4384-BF73-95AA5BC5D084}";
        public const string LinkcoAppIdName = "Linkco PDF Preview Host";
        public const string PreviewSurrogate = @"%SystemRoot%\system32\prevhost.exe";
        public const string DotNetComCategory = "{62C8FE65-4EBB-45e7-B440-6E39B2CDBF29}";
        public const string EdgePreviewHandlerClsid = "{3A84F9C2-6164-485C-A7D9-4B27F8AC009E}";
        public const string LinkcoConfigKey = @"Software\Linkco\Linkco PDF Editor";

        /// <summary>
        /// Bumped whenever the registry layout changes, so LinkcoPDFEditor.exe re-registers (repairs)
        /// existing installs on its next start. Keep in sync with apps/pdfcraft/src/windows_preview.rs.
        /// </summary>
        public const int RegistrationSchema = 6;

        /// <summary>ProgIDs owned by Linkco PDF Editor (or its upstream PdfCraft).</summary>
        private static readonly string[] OwnProgIds = new string[]
        {
            "LinkcoPDFEditor.Document",
            "PdfCraft.Document"
        };

        /// <summary>PDF-only ProgIDs of other PDF viewers that can be the default .pdf handler.</summary>
        private static readonly string[] KnownPdfProgIds = new string[]
        {
            "MSEdgePDF",
            "Acrobat.Document.DC",
            "AcroExch.Document.DC",
            "AcroExch.Document",
            "AcroExch.Document.7",
            "FoxitReader.Document",
            "FoxitPDFEditor.Document",
            "PDFXEdit.PDF"
        };

        /// <summary>
        /// ProgIDs shared with web pages and other non-PDF types. Version 0.5.0 builds (schema 2)
        /// registered on some of these; registration and uninstall remove that again.
        /// </summary>
        private static readonly string[] KnownSharedProgIds = new string[]
        {
            "MSEdgeHTM",
            "ChromeHTML",
            "BraveHTML",
            "FirefoxHTML",
            "OperaStable",
            "htmlfile"
        };

        private static readonly string[] NonPdfExtensions = new string[]
        {
            ".htm", ".html", ".shtml", ".xht", ".xhtml", ".mht", ".mhtml", ".svg", ".xml",
            ".txt", ".webp", ".png", ".jpg", ".jpeg", ".gif", ".epub", ".url"
        };

        private IntPtr _parentHwnd = IntPtr.Zero;
        private RECT _bounds;
        private string _filePath;
        private string _displayName;
        private string _tempStreamPath;
        private string _initError;
        private object _site;
        private IPreviewHandlerFrame _frame;
        private PreviewPaneControl _control;
        private bool _hasBackground;
        private Color _background;
        private bool _hasTextColor;
        private Color _textColor;
        private string _fontFace;

        public LinkcoPdfPreviewHandler()
        {
            // An exception thrown from a COM constructor makes CoCreateInstance fail, which Explorer
            // reports as "an error in the preview handler"; nothing here is needed to preview.
            try { PreviewLog.InstallCrashLogger(); } catch { }
            try { PreviewEnvironment.SweepStaleFilesOnce(); } catch { }
        }

        // Every COM entry point is a guarded wrapper around a NoInlining *Core method: an exception
        // raised while the JIT compiles a method (a type or assembly that fails to load) happens
        // before that method's own try block runs, and would otherwise escape to prevhost.exe.

        // ------------------------------------------------------------------ initialization

        public void Initialize(string pszFilePath, uint grfMode)
        {
            try { InitializeFileCore(pszFilePath); }
            catch (Exception ex) { PreviewLog.Error("IInitializeWithFile.Initialize", ex); _initError = "This PDF document could not be opened for preview."; }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void InitializeFileCore(string pszFilePath)
        {
            try
            {
                ResetDocument();
                _filePath = pszFilePath;
                _displayName = string.IsNullOrEmpty(pszFilePath) ? null : Path.GetFileName(pszFilePath);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("IInitializeWithFile.Initialize", ex);
                _initError = "This PDF document could not be opened for preview.";
            }
        }

        public void Initialize(IStream pstream, uint grfMode)
        {
            try { InitializeStreamCore(pstream); }
            catch (Exception ex) { PreviewLog.Error("IInitializeWithStream.Initialize", ex); _initError = "This PDF document could not be read for preview."; }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void InitializeStreamCore(IStream pstream)
        {
            try
            {
                ResetDocument();
                if (pstream == null)
                {
                    _initError = "Windows did not provide the PDF document to preview.";
                    return;
                }
                string displayName;
                Stopwatch clock = Stopwatch.StartNew();
                _tempStreamPath = PreviewEnvironment.CopyStreamToTempFile(pstream, "stream-", out displayName);
                _filePath = _tempStreamPath;
                if (displayName != null)
                    _displayName = displayName;
                long bytes = 0;
                try { bytes = new FileInfo(_tempStreamPath).Length; } catch { }
                PreviewLog.Write("Initialize(stream): " + bytes.ToString(CultureInfo.InvariantCulture) + " bytes copied in " +
                    clock.ElapsedMilliseconds.ToString(CultureInfo.InvariantCulture) + " ms");
            }
            catch (Exception ex)
            {
                PreviewLog.Error("IInitializeWithStream.Initialize", ex);
                _initError = "This PDF document could not be read for preview.";
                CleanupTempStreamFile();
            }
        }

        private void ResetDocument()
        {
            if (_control != null)
                _control.CancelRendering();
            CleanupTempStreamFile();
            _initError = null;
            _filePath = null;
            _displayName = null;
        }

        // ------------------------------------------------------------------ IPreviewHandler

        public void SetWindow(IntPtr hwnd, ref RECT rect)
        {
            try { SetWindowCore(hwnd, ref rect); }
            catch (Exception ex) { PreviewLog.Error("SetWindow", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void SetWindowCore(IntPtr hwnd, ref RECT rect)
        {
            try
            {
                _parentHwnd = hwnd;
                _bounds = rect;
                if (_control != null && _parentHwnd != IntPtr.Zero)
                {
                    AttachControl();
                    UpdateBounds();
                }
            }
            catch (Exception ex)
            {
                PreviewLog.Error("SetWindow", ex);
            }
        }

        public void SetRect(ref RECT rect)
        {
            try { SetRectCore(ref rect); }
            catch (Exception ex) { PreviewLog.Error("SetRect", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void SetRectCore(ref RECT rect)
        {
            try
            {
                _bounds = rect;
                UpdateBounds();
            }
            catch (Exception ex)
            {
                PreviewLog.Error("SetRect", ex);
            }
        }

        public void DoPreview()
        {
            try { DoPreviewCore(); }
            catch (Exception ex) { PreviewLog.Error("DoPreview", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void DoPreviewCore()
        {
            try
            {
                if (_parentHwnd == IntPtr.Zero)
                    return;

                if (_control == null || _control.IsDisposed)
                {
                    _control = new PreviewPaneControl();
                    ApplyVisuals();
                }
                AttachControl();
                UpdateBounds();
                PreviewLog.Write("DoPreview: pane " + _bounds.Width.ToString(CultureInfo.InvariantCulture) + "x" +
                    _bounds.Height.ToString(CultureInfo.InvariantCulture) + (_initError != null ? ", init error: " + _initError : ""));
                _control.LoadDocument(_filePath, _displayName, _initError);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("DoPreview", ex);
                if (_control != null)
                {
                    try { _control.ShowMessage("This PDF document could not be previewed.", true); } catch { }
                }
            }
        }

        public void Unload()
        {
            try { UnloadCore(); }
            catch (Exception ex) { PreviewLog.Error("Unload", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void UnloadCore()
        {
            try
            {
                if (_control != null)
                {
                    PreviewPaneControl c = _control;
                    _control = null;
                    try
                    {
                        c.Shutdown();
                        c.Dispose();
                    }
                    catch (Exception ex)
                    {
                        PreviewLog.Error("Unload/dispose", ex);
                    }
                }
                CleanupTempStreamFile();
                _filePath = null;
                _displayName = null;
                _initError = null;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("Unload", ex);
            }
        }

        public void SetFocus()
        {
            try { SetFocusCore(); }
            catch (Exception ex) { PreviewLog.Error("SetFocus", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void SetFocusCore()
        {
            if (_control != null && !_control.IsDisposed)
            {
                try { _control.Focus(); } catch { }
            }
        }

        public void QueryFocus(out IntPtr phwnd)
        {
            phwnd = IntPtr.Zero;
            try { phwnd = NativeMethods.GetFocus(); }
            catch (Exception ex) { PreviewLog.Error("QueryFocus", ex); }
        }

        public int TranslateAccelerator(ref MSG pmsg)
        {
            try { return TranslateAcceleratorCore(ref pmsg); }
            catch (Exception ex) { PreviewLog.Error("TranslateAccelerator", ex); }
            return NativeMethods.S_FALSE;
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private int TranslateAcceleratorCore(ref MSG pmsg)
        {
            // Let Explorer handle its own shortcuts (Alt+P, F5, Tab out of the pane, …).
            IPreviewHandlerFrame frame = _frame;
            if (frame != null)
            {
                try { return frame.TranslateAccelerator(ref pmsg); } catch { }
            }
            return NativeMethods.S_FALSE;
        }

        // ------------------------------------------------------------------ IPreviewHandlerVisuals

        public void SetBackgroundColor(uint color)
        {
            try
            {
                _background = ColorFromColorRef(color);
                _hasBackground = true;
                ApplyVisuals();
            }
            catch (Exception ex) { PreviewLog.Error("SetBackgroundColor", ex); }
        }

        public void SetFont(ref LOGFONT plf)
        {
            try
            {
                _fontFace = string.IsNullOrEmpty(plf.lfFaceName) ? null : plf.lfFaceName;
                ApplyVisuals();
            }
            catch (Exception ex) { PreviewLog.Error("SetFont", ex); }
        }

        public void SetTextColor(uint color)
        {
            try
            {
                _textColor = ColorFromColorRef(color);
                _hasTextColor = true;
                ApplyVisuals();
            }
            catch (Exception ex) { PreviewLog.Error("SetTextColor", ex); }
        }

        private static Color ColorFromColorRef(uint color)
        {
            return Color.FromArgb((int)(color & 0xFF), (int)((color >> 8) & 0xFF), (int)((color >> 16) & 0xFF));
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void ApplyVisuals()
        {
            if (_control == null || _control.IsDisposed)
                return;
            try
            {
                _control.ApplyTheme(
                    _hasBackground ? (Color?)_background : null,
                    _hasTextColor ? (Color?)_textColor : null,
                    _fontFace);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("ApplyVisuals", ex);
            }
        }

        // ------------------------------------------------------------------ IOleWindow / IObjectWithSite

        public void GetWindow(out IntPtr phwnd)
        {
            phwnd = _parentHwnd;
            try { phwnd = ControlWindow(); }
            catch (Exception ex) { PreviewLog.Error("GetWindow", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private IntPtr ControlWindow()
        {
            return (_control != null && !_control.IsDisposed && _control.IsHandleCreated) ? _control.Handle : _parentHwnd;
        }

        public void ContextSensitiveHelp(bool fEnterMode)
        {
        }

        public void SetSite(object pUnkSite)
        {
            try
            {
                _site = pUnkSite;
                _frame = null;
                if (pUnkSite != null)
                {
                    try { _frame = pUnkSite as IPreviewHandlerFrame; } catch { _frame = null; }
                }
            }
            catch (Exception ex) { PreviewLog.Error("SetSite", ex); }
        }

        public void GetSite(ref Guid riid, out object ppvSite)
        {
            ppvSite = null;
            if (_site == null)
            {
                Marshal.ThrowExceptionForHR(NativeMethods.E_FAIL);
                return;
            }
            IntPtr punk = Marshal.GetIUnknownForObject(_site);
            try
            {
                IntPtr ppv;
                int hr = Marshal.QueryInterface(punk, ref riid, out ppv);
                if (hr != 0)
                {
                    Marshal.ThrowExceptionForHR(hr);
                    return;
                }
                try
                {
                    ppvSite = Marshal.GetObjectForIUnknown(ppv);
                }
                finally
                {
                    Marshal.Release(ppv);
                }
            }
            finally
            {
                Marshal.Release(punk);
            }
        }

        // ------------------------------------------------------------------ hosting

        private void AttachControl()
        {
            if (_control == null || _parentHwnd == IntPtr.Zero)
                return;

            IntPtr handle = _control.Handle;
            if (NativeMethods.SetParent(handle, _parentHwnd) == IntPtr.Zero)
            {
                int err = Marshal.GetLastWin32Error();
                if (err != 0)
                    PreviewLog.Write("SetParent failed: " + new System.ComponentModel.Win32Exception(err).Message +
                        " (" + err.ToString(CultureInfo.InvariantCulture) + ")");
            }
            NativeMethods.MakeChildWindow(handle);
            NativeMethods.ShowWindow(handle, NativeMethods.SW_SHOW);
            if (!_control.Visible)
                _control.Visible = true;
            _control.RefreshDpi();
        }

        private void UpdateBounds()
        {
            if (_control == null || _control.IsDisposed || _parentHwnd == IntPtr.Zero)
                return;
            int w = Math.Max(1, _bounds.Width);
            int h = Math.Max(1, _bounds.Height);
            NativeMethods.SetWindowPos(
                _control.Handle,
                IntPtr.Zero,
                _bounds.left,
                _bounds.top,
                w,
                h,
                NativeMethods.SWP_NOZORDER | NativeMethods.SWP_NOACTIVATE | NativeMethods.SWP_SHOWWINDOW);
            _control.Invalidate();
        }

        private void CleanupTempStreamFile()
        {
            string path = _tempStreamPath;
            _tempStreamPath = null;
            if (!string.IsNullOrEmpty(path))
                PreviewEnvironment.DeleteFileSoon(path);
        }

        // ------------------------------------------------------------------ registration

        [ComRegisterFunction]
        public static void Register(Type t)
        {
            if (t == null || t != typeof(LinkcoPdfPreviewHandler))
                return;
            RegisterPreviewHandler(Assembly.GetExecutingAssembly().Location, IsElevated());
        }

        [ComUnregisterFunction]
        public static void Unregister(Type t)
        {
            if (t == null || t != typeof(LinkcoPdfPreviewHandler))
                return;
            UnregisterPreviewHandler();
        }

        /// <summary>Per-user registration (no administrator rights needed).</summary>
        public static void RegisterPreviewHandler(string dllPath)
        {
            RegisterPreviewHandler(dllPath, false);
        }

        /// <summary>
        /// Registers the handler for the current user and, when <paramref name="machineWide"/> is set
        /// (elevated installers), for all users. Throws when the per-user registration fails.
        /// </summary>
        public static void RegisterPreviewHandler(string dllPath, bool machineWide)
        {
            if (string.IsNullOrEmpty(dllPath))
                throw new ArgumentException("The preview handler DLL path is empty.", "dllPath");
            string fullDllPath = Path.GetFullPath(dllPath);
            if (!File.Exists(fullDllPath))
                throw new FileNotFoundException("The preview handler DLL was not found.", fullDllPath);

            NativeMethods.RemoveZoneIdentifier(fullDllPath);
            string cliPath = Path.Combine(Path.GetDirectoryName(fullDllPath) ?? "", "pdfcraft-cli.exe");
            if (File.Exists(cliPath))
                NativeMethods.RemoveZoneIdentifier(cliPath);

            bool wasCurrent = IsRegistrationCurrent(fullDllPath);

            using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
            {
                RegisterInRoot(hkcu, fullDllPath, true);
            }

            if (machineWide)
            {
                try
                {
                    using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                    {
                        RegisterInRoot(hklm, fullDllPath, false);
                    }
                }
                catch (Exception ex)
                {
                    PreviewLog.Error("RegisterPreviewHandler(HKLM)", ex);
                }
            }

            if (!wasCurrent)
                StopStalePrevHost();
            NotifyShell();

            string effective = QueryEffectiveHandler();
            PreviewLog.Write("Registered " + HandlerName + " (schema " + RegistrationSchema.ToString(CultureInfo.InvariantCulture) +
                ", machineWide=" + machineWide + "); Explorer resolves .pdf previews to " + (effective ?? "(none)"));
        }

        /// <summary>Removes every registration (per-user and, when permitted, machine-wide).</summary>
        public static void UnregisterPreviewHandler()
        {
            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
            {
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    {
                        UnregisterFromRoot(root, hive == RegistryHive.CurrentUser);
                    }
                }
                catch (Exception ex)
                {
                    if (hive == RegistryHive.CurrentUser)
                        PreviewLog.Error("UnregisterPreviewHandler(HKCU)", ex);
                }
            }
            StopStalePrevHost();
            NotifyShell();
        }

        /// <summary>
        /// Removes Linkco PDF Editor's per-user registration from every user profile on this PC —
        /// it registers for each user who starts it — restoring each user's previous preview and
        /// thumbnail handlers, so no account is left pointing Explorer at a deleted DLL. For
        /// uninstallers running elevated (or as LocalSystem, e.g. a deferred MSI custom action);
        /// machine-wide (HKLM) entries are left to the installer. Returns the number of profiles
        /// cleaned, or -1 when profiles of users who aren't signed in could not be opened because
        /// the process isn't elevated (signed-in users' profiles are still cleaned if accessible).
        /// </summary>
        public static int UnregisterPreviewHandlerForAllUsers()
        {
            int cleaned = 0;
            bool complete = UserProfileHives.ForEach(delegate(string sid, RegRoot root)
            {
                try
                {
                    if (CleanUserRoot(root))
                    {
                        cleaned++;
                        PreviewLog.Write("Removed the per-user registration of " + sid + ".");
                    }
                }
                catch (Exception ex)
                {
                    PreviewLog.Error("UnregisterPreviewHandlerForAllUsers(" + sid + ")", ex);
                }
            });
            if (cleaned > 0)
            {
                StopStalePrevHost();
                NotifyShell();
            }
            return complete ? cleaned : -1;
        }

        /// <summary>The preview handler CLSID Explorer currently resolves for .pdf files.</summary>
        public static string QueryEffectiveHandler()
        {
            return NativeMethods.QueryShellExtension(".pdf", PreviewHandlerCategoryGuid);
        }

        /// <summary>True when Explorer resolves .pdf previews to this handler.</summary>
        public static bool IsEffectiveHandler()
        {
            return string.Equals(QueryEffectiveHandler(), ClsidBraced, StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>Human-readable registration and environment report for support and tests.</summary>
        public static string Diagnose()
        {
            StringBuilder sb = new StringBuilder();
            Assembly asm = typeof(LinkcoPdfPreviewHandler).Assembly;
            sb.AppendLine(HandlerName + " " + asm.GetName().Version);
            sb.AppendLine("  CLSID:                 " + ClsidBraced);
            sb.AppendLine("  Registration schema:   " + RegistrationSchema.ToString(CultureInfo.InvariantCulture));
            sb.AppendLine("  .NET runtime:          " + Environment.Version + (Environment.Is64BitProcess ? " (64-bit)" : " (32-bit)"));
            string effective = QueryEffectiveHandler();
            sb.AppendLine("  Explorer .pdf handler: " + (effective ?? "(none)") +
                (string.Equals(effective, ClsidBraced, StringComparison.OrdinalIgnoreCase) ? "  [Linkco - OK]" : "  [NOT Linkco]"));
            string thumbs = NativeMethods.QueryShellExtension(".pdf", LinkcoPdfThumbnailProvider.ThumbnailCategoryGuid);
            sb.AppendLine("  Explorer .pdf thumbs:  " + (thumbs ?? "(none)") +
                (string.Equals(thumbs, LinkcoPdfThumbnailProvider.ClsidBraced, StringComparison.OrdinalIgnoreCase) ? "  [Linkco - OK]" :
                 thumbs == null ? "  [none]" : "  [another app's thumbnails take precedence]"));
            sb.AppendLine("  UserChoice ProgId:     " + (ReadUserChoiceProgId(".pdf") ?? "(none)"));
            string cli = PreviewEnvironment.LocateCliExecutable();
            sb.AppendLine("  pdfcraft-cli.exe:      " + (cli ?? "(not found)"));
            sb.AppendLine("  Temp folder:           " + PreviewEnvironment.TempDir);

            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
            {
                string label = hive == RegistryHive.CurrentUser ? "HKCU" : "HKLM";
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    using (RegistryKey thumbInproc = root.OpenSubKey(@"Software\Classes\CLSID\" + LinkcoPdfThumbnailProvider.ClsidBraced + @"\InprocServer32", false))
                    using (RegistryKey inproc = root.OpenSubKey(@"Software\Classes\CLSID\" + ClsidBraced + @"\InprocServer32", false))
                    {
                        sb.AppendLine("  " + label + " thumbnails:       " + (thumbInproc == null ? "(not registered)" : thumbInproc.GetValue("CodeBase") as string ?? "(missing CodeBase)"));
                        if (inproc == null)
                        {
                            sb.AppendLine("  " + label + " COM server:       (not registered)");
                            continue;
                        }
                        sb.AppendLine("  " + label + " CodeBase:         " + (inproc.GetValue("CodeBase") as string ?? "(missing)"));
                        sb.AppendLine("  " + label + " ThreadingModel:   " + (inproc.GetValue("ThreadingModel") as string ?? "(missing)"));
                    }
                }
                catch (Exception ex)
                {
                    sb.AppendLine("  " + label + ": " + ex.Message);
                }
            }

            if (IsUacDisabled())
                sb.AppendLine("  User Account Control:  off - Windows ignores per-user COM registrations, so only the machine-wide one (installer) is used");
            sb.AppendLine("  Effective COM server:  " + DescribeEffectiveServer(ClsidBraced));
            sb.AppendLine("  Preview host process:  " + DescribeHostAppId());
            sb.AppendLine("  .NET Framework:        " + DescribeNetFramework());
            sb.AppendLine("  COM activation test:   " + ActivationTest(ClsidString));
            foreach (string other in OtherPdfPreviewHandlers())
                sb.AppendLine("  Other .pdf previewer:  " + other);

            // Explorer only honours DisableLowILProcessIsolation from HKLM: a per-user registration
            // runs in the low-integrity preview host (which this handler supports; it is slower to start).
            object lowIl = null;
            try
            {
                using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                using (RegistryKey c = hklm.OpenSubKey(@"Software\Classes\CLSID\" + ClsidBraced, false))
                    lowIl = c == null ? null : c.GetValue("DisableLowILProcessIsolation");
            }
            catch { }
            sb.AppendLine("  Preview host:          " + (lowIl is int && (int)lowIl == 1
                ? "normal integrity (machine-wide registration)"
                : "low integrity (per-user registration)"));

            sb.AppendLine("  Engine self-test:      " + EngineSelfTest(cli));

            string log = Path.Combine(PreviewEnvironment.TempDir, "preview.log");
            sb.AppendLine("  Preview log:           " + log);
            foreach (string line in TailLines(log, 15))
                sb.AppendLine("    " + line);

            sb.AppendLine("  Recent crashes (Windows Application log, last 3 days):");
            foreach (string line in RecentHostCrashes(3, 4))
                sb.AppendLine("    " + line);
            return sb.ToString();
        }

        /// <summary>
        /// The AppID (preview host process) of the registration COM uses: Linkco's own prevhost.exe,
        /// or the shared one left by an older registration.
        /// </summary>
        /// <summary>
        /// True when User Account Control is turned off. Explorer then runs with full administrator rights
        /// and COM ignores per-user (HKCU) class registrations: only the machine-wide one can work.
        /// </summary>
        internal static bool IsUacDisabled()
        {
            try
            {
                using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                using (RegistryKey k = hklm.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", false))
                {
                    object v = k == null ? null : k.GetValue("EnableLUA");
                    return v is int && (int)v == 0;
                }
            }
            catch
            {
                return false;
            }
        }

        /// <summary>The hives COM reads class registrations from, in priority order.</summary>
        private static RegistryHive[] ComHives()
        {
            return IsUacDisabled()
                ? new RegistryHive[] { RegistryHive.LocalMachine }
                : new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine };
        }

        internal static string DescribeHostAppId()
        {
            foreach (RegistryHive hive in ComHives())
            {
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    using (RegistryKey clsid = root.OpenSubKey(@"Software\Classes\CLSID\" + ClsidBraced, false))
                    {
                        if (clsid == null)
                            continue;
                        string label = hive == RegistryHive.CurrentUser ? "HKCU" : "HKLM";
                        string appId = clsid.GetValue("AppID") as string;
                        if (string.IsNullOrEmpty(appId))
                            return label + ", no AppID  [BROKEN: Explorer cannot host it; start Linkco PDF Editor once or reinstall]";
                        if (string.Equals(appId, PrevHostAppId, StringComparison.OrdinalIgnoreCase))
                            return label + ", shared prevhost " + appId + "  [OLD: shared with other apps' handlers; start Linkco PDF Editor once or reinstall]";
                        string surrogate = null;
                        foreach (RegistryHive h in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
                        {
                            using (RegistryKey r = PreviewEnvironment.OpenHive(h))
                            using (RegistryKey a = r.OpenSubKey(@"Software\Classes\AppID\" + appId, false))
                            {
                                if (a != null)
                                {
                                    surrogate = a.GetValue("DllSurrogate", null, RegistryValueOptions.DoNotExpandEnvironmentNames) as string;
                                    break;
                                }
                            }
                        }
                        if (string.IsNullOrEmpty(surrogate))
                            return label + ", AppID " + appId + " has no DllSurrogate  [BROKEN: start Linkco PDF Editor once or reinstall]";
                        return label + ", AppID " + appId + " -> " + surrogate +
                            (string.Equals(appId, LinkcoAppId, StringComparison.OrdinalIgnoreCase) ? "  [own host - OK]" : "");
                    }
                }
                catch (Exception ex)
                {
                    return "unreadable (" + ex.Message + ")";
                }
            }
            return "(not registered)";
        }

        /// <summary>The installed .NET Framework 4.x release (the handler needs 4.0 or later; 4.8 recommended).</summary>
        internal static string DescribeNetFramework()
        {
            try
            {
                using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                using (RegistryKey k = hklm.OpenSubKey(@"SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full", false))
                {
                    if (k == null)
                        return "4.x not installed  [BROKEN: install .NET Framework 4.8]";
                    object release = k.GetValue("Release");
                    string version = k.GetValue("Version") as string ?? "?";
                    int r = release is int ? (int)release : 0;
                    return version + " (release " + r.ToString(CultureInfo.InvariantCulture) + ")" +
                        (r >= 528040 ? "  [OK]" : "  [old: install .NET Framework 4.8]");
                }
            }
            catch (Exception ex)
            {
                return "unreadable (" + ex.Message + ")";
            }
        }

        /// <summary>
        /// Other apps' preview handlers registered for .pdf (e.g. PowerToys, Adobe, a browser), with
        /// where they are registered. Explorer picks one; the "Explorer .pdf handler" line shows which.
        /// </summary>
        internal static List<string> OtherPdfPreviewHandlers()
        {
            List<string> found = new List<string>();
            string choice = ReadUserChoiceProgId(".pdf");
            List<string> places = new List<string>();
            places.Add(@"Software\Classes\.pdf\ShellEx\" + PreviewHandlerCategoryGuid);
            places.Add(@"Software\Classes\SystemFileAssociations\.pdf\ShellEx\" + PreviewHandlerCategoryGuid);
            if (!string.IsNullOrEmpty(choice))
                places.Add(@"Software\Classes\" + choice + @"\ShellEx\" + PreviewHandlerCategoryGuid);
            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
            {
                string label = hive == RegistryHive.CurrentUser ? "HKCU" : "HKLM";
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    {
                        foreach (string place in places)
                        {
                            using (RegistryKey k = root.OpenSubKey(place, false))
                            {
                                string clsid = k == null ? null : k.GetValue("") as string;
                                if (string.IsNullOrEmpty(clsid) || string.Equals(clsid, ClsidBraced, StringComparison.OrdinalIgnoreCase))
                                    continue;
                                found.Add(clsid + " " + PreviewHandlerDisplayName(clsid) + "  (" + label + @"\" + place.Substring(@"Software\Classes\".Length) + ")");
                            }
                        }
                    }
                }
                catch { }
            }
            return found;
        }

        private static string PreviewHandlerDisplayName(string clsid)
        {
            if (string.Equals(clsid, "{A5A41CC7-02CB-41D4-8C9B-9087040D6098}", StringComparison.OrdinalIgnoreCase))
                return "\"PowerToys PDF Preview Handler\" (turn it off in PowerToys > File Explorer add-ons to use Linkco's)";
            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.CurrentUser, RegistryHive.LocalMachine })
            {
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    {
                        using (RegistryKey list = root.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers", false))
                        {
                            string name = list == null ? null : list.GetValue(clsid) as string;
                            if (!string.IsNullOrEmpty(name))
                                return "\"" + name + "\"";
                        }
                        using (RegistryKey c = root.OpenSubKey(@"Software\Classes\CLSID\" + clsid, false))
                        {
                            string name = c == null ? null : c.GetValue("") as string;
                            if (!string.IsNullOrEmpty(name))
                                return "\"" + name + "\"";
                        }
                    }
                }
                catch { }
            }
            return "(unnamed)";
        }

        /// <summary>
        /// Which registration COM actually uses for <paramref name="clsidBraced"/> (HKCU overrides HKLM),
        /// whether its DLL exists, and the DLL's assembly version.
        /// </summary>
        internal static string DescribeEffectiveServer(string clsidBraced)
        {
            foreach (RegistryHive hive in ComHives())
            {
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    using (RegistryKey inproc = root.OpenSubKey(@"Software\Classes\CLSID\" + clsidBraced + @"\InprocServer32", false))
                    {
                        if (inproc == null)
                            continue;
                        string label = hive == RegistryHive.CurrentUser ? "HKCU" : "HKLM";
                        string codeBase = inproc.GetValue("CodeBase") as string;
                        string assembly = inproc.GetValue("Assembly") as string;
                        string path = CodeBaseToPath(codeBase);
                        if (path == null)
                            return label + ", CodeBase missing or unreadable (" + (codeBase ?? "null") + ")  [BROKEN: re-register]";
                        if (IsEscapedCodeBase(codeBase) && !File.Exists(path))
                            return label + ", CodeBase " + codeBase + "  [BROKEN: percent-escaped CodeBase, .NET cannot find the DLL (0x80070002); start Linkco PDF Editor 1.0.0 once or reinstall]";
                        if (!File.Exists(path))
                            return label + ", " + path + "  [BROKEN: the DLL does not exist; start Linkco PDF Editor once or reinstall]";
                        string version = "?";
                        try { version = AssemblyName.GetAssemblyName(path).Version.ToString(); } catch (Exception ex) { version = "unreadable: " + ex.Message; }
                        bool marked = File.Exists(path + ":Zone.Identifier");
                        return label + ", " + path + " (assembly " + version + ", registered as " + (assembly ?? "?") + ")" +
                            (marked ? "  [BLOCKED: downloaded-file mark; run Unblock-File on it]" : "");
                    }
                }
                catch (Exception ex)
                {
                    return "unreadable (" + ex.Message + ")";
                }
            }
            return IsUacDisabled()
                ? "(not registered machine-wide)  [BROKEN: User Account Control is off, so the per-user registration is ignored; install Linkco PDF Editor for all users]"
                : "(not registered)";
        }

        /// <summary>
        /// The InprocServer32 <c>CodeBase</c> for <paramref name="fullDllPath"/>, in the form regasm writes
        /// (<c>file:///C:/Program Files/...</c>). The CLR's COM activation hands this string to the assembly
        /// binder as-is and does NOT percent-decode it, so <c>Uri.AbsoluteUri</c>'s <c>Program%20Files</c>
        /// names a folder that does not exist: every activation (prevhost, Explorer) then fails with
        /// 0x80070002 and Explorer reports "an error in the Linkco PDF Preview Handler".
        /// </summary>
        internal static string ComCodeBase(string fullDllPath)
        {
            string p = fullDllPath.Replace('\\', '/');
            if (p.StartsWith("//", StringComparison.Ordinal))
                return "file:" + p; // UNC: file://server/share/...
            return "file:///" + p;
        }

        /// <summary>The file the CLR loads for a CodeBase: like the binder, no percent-decoding.</summary>
        internal static string CodeBaseToPath(string codeBase)
        {
            if (string.IsNullOrEmpty(codeBase))
                return null;
            try
            {
                if (codeBase.StartsWith("file:", StringComparison.OrdinalIgnoreCase))
                {
                    string p = codeBase.Substring(5);
                    string drive = p.TrimStart('/');
                    if (drive.Length >= 2 && drive[1] == ':')
                        return drive.Replace('/', '\\');
                    if (p.StartsWith("//", StringComparison.Ordinal))
                        return "\\\\" + p.TrimStart('/').Replace('/', '\\');
                    return null;
                }
                return codeBase;
            }
            catch
            {
                return null;
            }
        }

        /// <summary>True for a CodeBase that the CLR cannot load because it is percent-escaped.</summary>
        internal static bool IsEscapedCodeBase(string codeBase)
        {
            if (string.IsNullOrEmpty(codeBase))
                return false;
            for (int i = 0; i + 2 < codeBase.Length; i++)
            {
                if (codeBase[i] == '%' && Uri.IsHexDigit(codeBase[i + 1]) && Uri.IsHexDigit(codeBase[i + 2]))
                    return true;
            }
            return false;
        }

        /// <summary>
        /// Creates the handler through COM, as Explorer's preview host does, so a registration or
        /// assembly-load problem shows up as a concrete error instead of Explorer's generic
        /// "This file can't be previewed because of an error in the Linkco PDF Preview Handler".
        /// </summary>
        /// <summary>
        /// Creates the handler the way prevhost.exe does: in a fresh native process (64-bit cscript), so the
        /// CLR must find and load the DLL from the registration. An in-process test from PowerShell reuses
        /// the copy of this DLL that is already loaded and passes even when Explorer cannot load it.
        /// </summary>
        internal static string ActivationTest(string clsid)
        {
            string vbs = null;
            try
            {
                string windows = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
                string system = Environment.Is64BitOperatingSystem && !Environment.Is64BitProcess ? "Sysnative" : "System32";
                string cscript = Path.Combine(Path.Combine(windows, system), "cscript.exe");
                if (!File.Exists(cscript))
                    return InProcessActivationTest(clsid) + " (in this process; cscript.exe not found)";
                vbs = Path.Combine(Path.GetTempPath(), "linkco-activation-" + Guid.NewGuid().ToString("N") + ".vbs");
                File.WriteAllText(vbs,
                    "On Error Resume Next\r\n" +
                    "Set o = GetObject(\"new:" + new Guid(clsid).ToString("B") + "\")\r\n" +
                    "If Err.Number <> 0 Then\r\n" +
                    "  WScript.Echo \"FAILED (0x\" & Hex(Err.Number) & \" \" & Err.Description & \")\"\r\n" +
                    "Else\r\n" +
                    "  WScript.Echo \"OK\"\r\n" +
                    "End If\r\n", Encoding.ASCII);
                ProcessStartInfo psi = new ProcessStartInfo(cscript, "//nologo //E:vbscript \"" + vbs + "\"");
                psi.UseShellExecute = false;
                psi.CreateNoWindow = true;
                psi.RedirectStandardOutput = true;
                psi.RedirectStandardError = true;
                using (Process p = Process.Start(psi))
                {
                    if (p == null)
                        return "FAILED (could not start cscript.exe)";
                    if (!p.WaitForExit(20000))
                    {
                        try { p.Kill(); } catch { }
                        return "FAILED (creating the handler did not finish in 20 s)";
                    }
                    string output = (p.StandardOutput.ReadToEnd() + " " + p.StandardError.ReadToEnd()).Trim();
                    if (output.StartsWith("OK", StringComparison.Ordinal))
                        return "OK (fresh process, as prevhost.exe)";
                    if (output.IndexOf("80070002", StringComparison.Ordinal) >= 0)
                        output += "  [.NET cannot find the DLL the registration names: check CodeBase above; run diagnose-preview.ps1 -Repair]";
                    return output.Length > 0 ? output : "FAILED (cscript exit code " + p.ExitCode.ToString(CultureInfo.InvariantCulture) + ")";
                }
            }
            catch (Exception ex)
            {
                return InProcessActivationTest(clsid) + " (in this process; " + ex.Message + ")";
            }
            finally
            {
                if (vbs != null)
                {
                    try { File.Delete(vbs); } catch { }
                }
            }
        }

        private static string InProcessActivationTest(string clsid)
        {
            object instance = null;
            try
            {
                Type t = Type.GetTypeFromCLSID(new Guid(clsid), true);
                instance = Activator.CreateInstance(t);
                if (instance == null)
                    return "FAILED (COM returned no object)";
                // Ask by IID: when called from PowerShell the object comes from the COM-loaded copy of this
                // DLL, whose IPreviewHandler is a different .NET type from the caller's.
                bool previewer = false;
                IntPtr unknown = Marshal.GetIUnknownForObject(instance);
                try
                {
                    Guid iid = typeof(IPreviewHandler).GUID;
                    IntPtr ppv;
                    previewer = Marshal.QueryInterface(unknown, ref iid, out ppv) == 0;
                    if (ppv != IntPtr.Zero)
                        Marshal.Release(ppv);
                }
                finally
                {
                    Marshal.Release(unknown);
                }
                string from = "";
                try { from = instance.GetType().Assembly.Location; } catch { }
                return previewer
                    ? "OK" + (from.Length > 0 ? " (" + from + ")" : "")
                    : "FAILED (the object does not implement IPreviewHandler)";
            }
            catch (Exception ex)
            {
                Exception inner = ex is TargetInvocationException && ex.InnerException != null ? ex.InnerException : ex;
                return "FAILED (0x" + Marshal.GetHRForException(inner).ToString("X8", CultureInfo.InvariantCulture) + " " +
                    inner.GetType().Name + ": " + inner.Message + ")";
            }
            finally
            {
                if (instance != null && Marshal.IsComObject(instance))
                {
                    try { Marshal.ReleaseComObject(instance); } catch { }
                }
            }
        }

        /// <summary>
        /// Application-log errors from the last <paramref name="days"/> days that mention the preview
        /// host, this DLL or the engine (Application Error, .NET Runtime, Windows Error Reporting).
        /// </summary>
        internal static List<string> RecentHostCrashes(int days, int max)
        {
            List<string> found = new List<string>();
            try
            {
                DateTime since = DateTime.Now.AddDays(-days);
                using (EventLog log = new EventLog("Application"))
                {
                    EventLogEntryCollection entries = log.Entries;
                    int scanned = 0;
                    for (int i = entries.Count - 1; i >= 0 && scanned < 5000 && found.Count < max; i--, scanned++)
                    {
                        EventLogEntry entry = entries[i];
                        if (entry.TimeGenerated < since)
                            break;
                        if (entry.EntryType != EventLogEntryType.Error)
                            continue;
                        string source = entry.Source ?? "";
                        if (source != "Application Error" && source != ".NET Runtime" && source != "Windows Error Reporting" && source != "Application Hang")
                            continue;
                        string message = entry.Message ?? "";
                        if (message.IndexOf("prevhost", StringComparison.OrdinalIgnoreCase) < 0 &&
                            message.IndexOf("LinkcoPdfPreview", StringComparison.OrdinalIgnoreCase) < 0 &&
                            message.IndexOf("pdfcraft-cli", StringComparison.OrdinalIgnoreCase) < 0)
                            continue;
                        string flat = message.Replace("\r", " ").Replace("\n", " ");
                        while (flat.Contains("  "))
                            flat = flat.Replace("  ", " ");
                        if (flat.Length > 900)
                            flat = flat.Substring(0, 900) + "\u2026";
                        found.Add(entry.TimeGenerated.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + " " + source + ": " + flat);
                    }
                }
                if (found.Count == 0)
                    found.Add("(none)");
            }
            catch (Exception ex)
            {
                found.Add("(Application log unreadable: " + ex.Message + ")");
            }
            return found;
        }

        /// <summary>Starts pdfcraft-cli.exe exactly as the preview pane does and reports how it went.</summary>
        internal static string EngineSelfTest(string cli)
        {
            if (string.IsNullOrEmpty(cli))
                return "FAILED (pdfcraft-cli.exe not found)";
            try
            {
                Stopwatch clock = Stopwatch.StartNew();
                using (CliProcess proc = CliProcess.Start(cli, "--version", "selftest"))
                {
                    if (!proc.WaitForExit(15000))
                    {
                        proc.Kill();
                        return "FAILED (no answer within 15 s)";
                    }
                    string output = proc.ReadStdout().Trim();
                    int newline = output.IndexOfAny(new char[] { '\r', '\n' });
                    if (newline >= 0)
                        output = output.Substring(0, newline);
                    uint? code = proc.ExitCode;
                    string ms = clock.ElapsedMilliseconds.ToString(CultureInfo.InvariantCulture);
                    if (code.HasValue && code.Value == 0 && output.Length > 0)
                        return "OK in " + ms + " ms (" + output + ")";
                    return "FAILED (exit code " + (code.HasValue ? "0x" + code.Value.ToString("X", CultureInfo.InvariantCulture) : "?") + " after " + ms + " ms" +
                        (output.Length > 0 ? ": " + output : "") + ")";
                }
            }
            catch (Exception ex)
            {
                return "FAILED (" + ex.GetType().Name + ": " + ex.Message + ")";
            }
        }

        private static List<string> TailLines(string path, int count)
        {
            List<string> lines = new List<string>();
            try
            {
                if (!File.Exists(path))
                {
                    lines.Add("(no log yet: Explorer has not loaded the preview handler since installation)");
                    return lines;
                }
                using (FileStream fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                using (StreamReader reader = new StreamReader(fs, Encoding.UTF8))
                {
                    string line;
                    while ((line = reader.ReadLine()) != null)
                    {
                        lines.Add(line);
                        if (lines.Count > count)
                            lines.RemoveAt(0);
                    }
                }
            }
            catch (Exception ex)
            {
                lines.Add("(log unreadable: " + ex.Message + ")");
            }
            return lines;
        }

        // ---- Files on network shares: Explorer's preview pane and the Windows security zones ----
        //
        // Since the October 2025 Windows update (KB5066835) File Explorer shows no preview for a file in
        // the Internet or Restricted sites zone. Explorer decides that before it loads any preview
        // handler, so this handler can neither show a message there nor work around it. A share
        // reached by a plain server name (\\fileserver\share) is in the Local intranet zone and previews
        // normally; one reached by a dotted name (\\files.corp.example.com\share) or an IP address
        // (\\10.0.0.5\share) is in the Internet zone unless that server is listed as an intranet site;
        // and a downloaded file (Mark of the Web) is in the zone it came from, wherever it is stored.
        // DiagnoseFile reports which applies to a file. AllowNetworkPreview lists one server as a Local
        // intranet site for the current user only (what Internet Options > Security > Local intranet >
        // Sites > Advanced does) and DisallowNetworkPreview takes it out again. Nothing here runs
        // unless the user calls it; no zone setting is ever changed for every server or every user.

        internal const string ZoneMapKey = @"Software\Microsoft\Windows\CurrentVersion\Internet Settings\ZoneMap";
        private const string ZonePolicyKey = @"Software\Policies\Microsoft\Windows\CurrentVersion\Internet Settings";
        private const int MUTZ_ISFILE = 0x2;
        private const int MUTZ_REQUIRESAVEDFILECHECK = 0x400;

        /// <summary>
        /// The server of a network path: \\server\share\a.pdf, \\?\UNC\server\share\a.pdf and the WebDAV
        /// form \\server@SSL@443\DavWWWRoot\a.pdf all give "server". Null for a local path.
        /// </summary>
        internal static string UncServer(string path)
        {
            if (string.IsNullOrEmpty(path))
                return null;
            string p = path.Trim().Replace('/', '\\');
            if (p.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
                p = @"\\" + p.Substring(8);
            else if (p.StartsWith(@"\\?\", StringComparison.Ordinal) || p.StartsWith(@"\\.\", StringComparison.Ordinal))
                return null;
            if (!p.StartsWith(@"\\", StringComparison.Ordinal))
                return null;
            string host = p.Substring(2);
            int end = host.IndexOf('\\');
            if (end >= 0)
                host = host.Substring(0, end);
            int at = host.IndexOf('@');
            if (at >= 0)
                host = host.Substring(0, at);
            host = host.Trim().TrimEnd('.');
            return host.Length == 0 ? null : host;
        }

        /// <summary>True for a dotted IPv4 address such as 10.0.0.5.</summary>
        internal static bool IsIPv4Literal(string host)
        {
            if (string.IsNullOrEmpty(host))
                return false;
            string[] parts = host.Split('.');
            if (parts.Length != 4)
                return false;
            foreach (string part in parts)
            {
                int n;
                if (part.Length == 0 || part.Length > 3 || !int.TryParse(part, NumberStyles.None, CultureInfo.InvariantCulture, out n) || n > 255)
                    return false;
            }
            return true;
        }

        /// <summary>
        /// The key under ZoneMap that lists <paramref name="host"/> as a site, in the layout Internet
        /// Options uses: "fileserver" -> Domains\fileserver, "corp.local" -> Domains\corp.local,
        /// "files.corp.example.com" -> Domains\example.com\files.corp, and an IP address -> a Ranges
        /// entry of our own (Ranges\Linkco_10.0.0.5). Null when <paramref name="host"/> is no server name.
        /// </summary>
        internal static string ZoneMapSiteKey(string host)
        {
            if (string.IsNullOrEmpty(host))
                return null;
            string h = host.Trim().TrimEnd('.').ToLowerInvariant();
            if (h.Length == 0 || h.IndexOfAny(new char[] { '\\', '/', '*', ':', ' ', '@', '%' }) >= 0)
                return null;
            if (IsIPv4Literal(h))
                return @"Ranges\Linkco_" + h;
            string[] labels = h.Split('.');
            foreach (string label in labels)
            {
                if (label.Length == 0)
                    return null;
            }
            if (labels.Length <= 2)
                return @"Domains\" + h;
            int n = labels.Length;
            return @"Domains\" + labels[n - 2] + "." + labels[n - 1] + @"\" + string.Join(".", labels, 0, n - 2);
        }

        /// <summary>
        /// Lists <paramref name="host"/> as a Local intranet site for files (file: = zone 1) in
        /// <paramref name="hkcu"/>. Returns the ZoneMap subkey written, or null for no server name.
        /// </summary>
        internal static string AddIntranetSite(RegistryKey hkcu, string host)
        {
            string site = ZoneMapSiteKey(host);
            if (site == null)
                return null;
            using (RegistryKey k = hkcu.CreateSubKey(ZoneMapKey + @"\" + site))
            {
                if (k == null)
                    return null;
                if (site.StartsWith(@"Ranges\", StringComparison.Ordinal))
                    k.SetValue(":Range", host.Trim().TrimEnd('.'), RegistryValueKind.String);
                k.SetValue("file", 1, RegistryValueKind.DWord);
            }
            return site;
        }

        /// <summary>
        /// Takes <paramref name="host"/> off the sites list for files again: removes its file: entry,
        /// then its key and an emptied parent domain key (never Domains or Ranges themselves, and never
        /// a key that still holds other entries, such as http: ones the user added). True when an entry
        /// was removed.
        /// </summary>
        internal static bool RemoveIntranetSite(RegistryKey hkcu, string host)
        {
            string site = ZoneMapSiteKey(host);
            if (site == null)
                return false;
            bool removed = false;
            using (RegistryKey k = hkcu.OpenSubKey(ZoneMapKey + @"\" + site, true))
            {
                if (k == null)
                    return false;
                if (k.GetValue("file") != null)
                {
                    k.DeleteValue("file", false);
                    removed = true;
                }
                if (site.StartsWith(@"Ranges\", StringComparison.Ordinal) && k.ValueCount == 1 && k.GetValue(":Range") != null)
                    k.DeleteValue(":Range", false);
            }
            string[] parts = site.Split('\\');
            for (int depth = parts.Length; depth >= 2; depth--)
            {
                string path = ZoneMapKey + @"\" + string.Join(@"\", parts, 0, depth);
                bool empty;
                using (RegistryKey k = hkcu.OpenSubKey(path, false))
                {
                    if (k == null)
                        continue;
                    empty = k.SubKeyCount == 0 && k.ValueCount == 0;
                }
                if (!empty)
                    break;
                hkcu.DeleteSubKey(path, false);
            }
            return removed;
        }

        /// <summary>A zone number as Internet Options names it.</summary>
        private static string ZoneName(int zone)
        {
            switch (zone)
            {
                case 0: return "Local computer (0)";
                case 1: return "Local intranet (1)";
                case 2: return "Trusted sites (2)";
                case 3: return "Internet (3)";
                case 4: return "Restricted sites (4)";
                default: return zone < 0 ? "(unknown)" : "Custom zone (" + zone.ToString(CultureInfo.InvariantCulture) + ")";
            }
        }

        /// <summary>The zone Windows puts <paramref name="path"/> in; with the file check, a Mark of the Web counts too. -1 when unknown.</summary>
        private static int QueryZone(string path, bool includeFileMark)
        {
            object manager = null;
            try
            {
                manager = new InternetSecurityManager();
                int zone;
                int flags = MUTZ_ISFILE | (includeFileMark ? MUTZ_REQUIRESAVEDFILECHECK : 0);
                return ((IInternetSecurityManager)manager).MapUrlToZone(path, out zone, flags) == 0 ? zone : -1;
            }
            catch
            {
                return -1;
            }
            finally
            {
                if (manager != null)
                {
                    try
                    {
                        Marshal.ReleaseComObject(manager);
                    }
                    catch { }
                }
            }
        }

        /// <summary>A full path with a mapped network drive (Z:\a.pdf) replaced by its share (\\server\share\a.pdf).</summary>
        private static string ResolveMappedDrive(string fullPath, out string drive)
        {
            drive = null;
            if (fullPath.Length >= 2 && fullPath[1] == ':' && char.IsLetter(fullPath[0]))
            {
                string remote = NativeMethods.MappedDriveTarget(fullPath.Substring(0, 2));
                if (!string.IsNullOrEmpty(remote) && remote.StartsWith(@"\\", StringComparison.Ordinal))
                {
                    drive = fullPath.Substring(0, 2).ToUpperInvariant();
                    return remote.TrimEnd('\\') + fullPath.Substring(2);
                }
            }
            return fullPath;
        }

        /// <summary>
        /// The server an argument names: a path to a file or folder on a share or mapped drive, or a
        /// bare server name ("files.corp.example.com", "10.0.0.5"). Null when it is a local path.
        /// </summary>
        private static string ServerFromArgument(string pathOrServer, out string resolved, out string drive)
        {
            resolved = null;
            drive = null;
            if (string.IsNullOrEmpty(pathOrServer))
                return null;
            string arg = pathOrServer.Trim().Trim('"');
            if (arg.IndexOfAny(new char[] { '\\', '/', ':' }) < 0)
                return ZoneMapSiteKey(arg) == null ? null : arg.TrimEnd('.');
            try
            {
                resolved = ResolveMappedDrive(Path.GetFullPath(arg), out drive);
            }
            catch
            {
                resolved = arg;
            }
            return UncServer(resolved);
        }

        /// <summary>True when policy makes the zone settings machine-wide, so per-user sites are ignored.</summary>
        private static bool ZonesAreMachineWideByPolicy()
        {
            try
            {
                using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                using (RegistryKey k = hklm.OpenSubKey(ZonePolicyKey, false))
                {
                    object v = k == null ? null : k.GetValue("Security_HKLM_only");
                    return v is int && (int)v == 1;
                }
            }
            catch
            {
                return false;
            }
        }

        /// <summary>True when an administrator set a Site to Zone Assignment List (Group Policy).</summary>
        private static bool SiteListSetByPolicy()
        {
            foreach (RegistryHive hive in new RegistryHive[] { RegistryHive.LocalMachine, RegistryHive.CurrentUser })
            {
                try
                {
                    using (RegistryKey root = PreviewEnvironment.OpenHive(hive))
                    using (RegistryKey k = root.OpenSubKey(ZonePolicyKey + @"\ZoneMapKey", false))
                    {
                        if (k != null && k.ValueCount > 0)
                            return true;
                    }
                }
                catch { }
            }
            return false;
        }

        /// <summary>
        /// Why a PDF does or doesn't preview in File Explorer: where it is stored (local disk, network
        /// share, mapped drive, OneDrive/cloud), the security zone Windows puts it in, whether it carries
        /// a Mark of the Web, and what to do when Explorer blocks its preview. Changes nothing.
        /// </summary>
        public static string DiagnoseFile(string path)
        {
            StringBuilder sb = new StringBuilder();
            string full;
            try
            {
                full = Path.GetFullPath((path ?? "").Trim().Trim('"'));
            }
            catch (Exception ex)
            {
                return "Not a usable path: " + (path ?? "(null)") + " (" + ex.Message + ")" + Environment.NewLine;
            }
            string drive;
            string resolved = ResolveMappedDrive(full, out drive);
            string server = UncServer(resolved);
            sb.AppendLine(HandlerName + ": preview check for " + full);
            sb.AppendLine("  File exists:           " + (File.Exists(full) ? "yes" : "NO (or not reachable)"));

            string location;
            if (server != null)
            {
                location = "network share on " + server + (drive != null ? " (mapped drive " + drive + " = " + resolved + ")" : "");
                if (IsIPv4Literal(server))
                    location += ", reached by IP address";
                else if (server.IndexOf('.') >= 0)
                    location += ", reached by a dotted (fully qualified) name";
            }
            else
            {
                location = "this computer";
                string[] cloudRoots = { Environment.GetEnvironmentVariable("OneDrive"), Environment.GetEnvironmentVariable("OneDriveCommercial"), Environment.GetEnvironmentVariable("OneDriveConsumer") };
                foreach (string root in cloudRoots)
                {
                    if (!string.IsNullOrEmpty(root) && full.StartsWith(root.TrimEnd('\\') + @"\", StringComparison.OrdinalIgnoreCase))
                    {
                        location = "OneDrive folder (" + root + ")";
                        break;
                    }
                }
                try
                {
                    // FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS / RECALL_ON_OPEN / OFFLINE: an online-only cloud file.
                    int attrs = (int)File.GetAttributes(full);
                    if ((attrs & (0x400000 | 0x40000 | 0x1000)) != 0)
                        location += ", online-only: Windows downloads it when Explorer previews it";
                }
                catch { }
            }
            sb.AppendLine("  Location:              " + location);

            int zoneOfPlace = QueryZone(resolved, false);
            int zone = QueryZone(resolved, true);
            bool marked = zone > zoneOfPlace && zoneOfPlace >= 0;
            sb.AppendLine("  Security zone:         " + ZoneName(zone) + (marked ? " (raised by the file's Mark of the Web; its location is " + ZoneName(zoneOfPlace) + ")" : ""));
            sb.AppendLine("  Downloaded file (MOTW): " + (zone < 0 ? "(unknown)" : marked ? "yes" : "no"));
            bool blocked = zone == 3 || zone == 4;
            sb.AppendLine("  Explorer preview pane: " + (zone < 0 ? "(unknown)" : blocked ? "BLOCKED by Windows for this zone (since the October 2025 update), before any preview handler runs" : "allowed"));
            sb.AppendLine("  Linkco preview handler: " + (IsEffectiveHandler() ? "registered for .pdf" : "NOT the .pdf preview handler (run RegisterPreviewHandler)"));
            if (ZonesAreMachineWideByPolicy())
                sb.AppendLine("  Policy:                zone settings are machine-wide (Security_HKLM_only): ask your IT administrator");
            else if (SiteListSetByPolicy())
                sb.AppendLine("  Policy:                your organisation sets a Site to Zone Assignment List: ask IT to add the server");
            if (blocked)
            {
                if (marked)
                    sb.AppendLine("  Fix:                   if you trust the file: Unblock-File -LiteralPath '" + full.Replace("'", "''") + "' (or Properties > Unblock)");
                if (server != null && zoneOfPlace >= 3)
                    sb.AppendLine("  Fix:                   if you trust the server: [LinkcoPdfPreview.LinkcoPdfPreviewHandler]::AllowNetworkPreview('" + server + "') lists " + server + " as a Local intranet site for you; or ask IT to add it to the Local intranet zone");
            }
            return sb.ToString();
        }

        /// <summary>
        /// Lets File Explorer preview PDFs (and other files) from the server of
        /// <paramref name="pathOrServer"/> by listing that server as a Local intranet site for the
        /// current user, as Internet Options does. Only for servers you trust: files from an intranet
        /// site also open with fewer warnings. Returns a report of what was done.
        /// </summary>
        public static string AllowNetworkPreview(string pathOrServer)
        {
            string resolved, drive;
            string server = ServerFromArgument(pathOrServer, out resolved, out drive);
            if (server == null)
                return "Not on a network share: " + pathOrServer + ". For a downloaded file use Unblock-File (or Properties > Unblock) instead." + Environment.NewLine;
            if (ZonesAreMachineWideByPolicy())
                return "Not changed: on this PC the security zones are set by your organisation for all users (Security_HKLM_only). Ask your IT administrator to add " + server + " to the Local intranet zone." + Environment.NewLine;
            try
            {
                string site;
                using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
                    site = AddIntranetSite(hkcu, server);
                if (site == null)
                    return "Not a server name: " + server + Environment.NewLine;
                PreviewLog.Write("Listed " + server + " as a Local intranet site (HKCU\\" + ZoneMapKey + "\\" + site + ").");
                string report = server + " is now a Local intranet site for you (HKCU\\" + ZoneMapKey + "\\" + site + ", file = 1). Close and reopen File Explorer to preview its PDFs." + Environment.NewLine;
                if (SiteListSetByPolicy())
                    report += "Note: your organisation sets a Site to Zone Assignment List, which may override this; if previews stay blank, ask IT to add " + server + "." + Environment.NewLine;
                return report;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("AllowNetworkPreview(" + server + ")", ex);
                return "Could not list " + server + " as a Local intranet site: " + ex.Message + Environment.NewLine;
            }
        }

        /// <summary>Undoes <see cref="AllowNetworkPreview"/> for the server of <paramref name="pathOrServer"/>.</summary>
        public static string DisallowNetworkPreview(string pathOrServer)
        {
            string resolved, drive;
            string server = ServerFromArgument(pathOrServer, out resolved, out drive);
            if (server == null)
                return "Not on a network share: " + pathOrServer + Environment.NewLine;
            try
            {
                bool removed;
                using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
                    removed = RemoveIntranetSite(hkcu, server);
                if (removed)
                    PreviewLog.Write("Removed " + server + " from the Local intranet sites.");
                return (removed ? server + " is no longer a Local intranet site for you." : server + " was not listed as a Local intranet site for you; nothing changed.") + Environment.NewLine;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("DisallowNetworkPreview(" + server + ")", ex);
                return "Could not change the sites list: " + ex.Message + Environment.NewLine;
            }
        }

        private static bool IsElevated()
        {
            try
            {
                using (System.Security.Principal.WindowsIdentity id = System.Security.Principal.WindowsIdentity.GetCurrent())
                {
                    return new System.Security.Principal.WindowsPrincipal(id).IsInRole(System.Security.Principal.WindowsBuiltInRole.Administrator);
                }
            }
            catch
            {
                return false;
            }
        }

        private static void NotifyShell()
        {
            try
            {
                NativeMethods.SHChangeNotify(NativeMethods.SHCNE_ASSOCCHANGED, NativeMethods.SHCNF_IDLIST, IntPtr.Zero, IntPtr.Zero);
            }
            catch { }
        }

        private static bool IsRegistrationCurrent(string fullDllPath)
        {
            try
            {
                string expectedCodeBase = ComCodeBase(fullDllPath);
                string expectedAssembly = typeof(LinkcoPdfPreviewHandler).Assembly.FullName;
                using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
                using (RegistryKey cfg = hkcu.OpenSubKey(LinkcoConfigKey, false))
                using (RegistryKey inproc = hkcu.OpenSubKey(@"Software\Classes\CLSID\" + ClsidBraced + @"\InprocServer32", false))
                using (RegistryKey clsid = hkcu.OpenSubKey(@"Software\Classes\CLSID\" + ClsidBraced, false))
                using (RegistryKey appId = hkcu.OpenSubKey(@"Software\Classes\AppID\" + LinkcoAppId, false))
                {
                    if (cfg == null || inproc == null || clsid == null || appId == null)
                        return false;
                    if (!string.Equals(clsid.GetValue("AppID") as string, LinkcoAppId, StringComparison.OrdinalIgnoreCase) ||
                        string.IsNullOrEmpty(appId.GetValue("DllSurrogate", null, RegistryValueOptions.DoNotExpandEnvironmentNames) as string))
                        return false;
                    object schema = cfg.GetValue("PreviewHandlerSchema");
                    return schema is int && (int)schema == RegistrationSchema &&
                        string.Equals(cfg.GetValue("PreviewHandlerDll") as string, fullDllPath, StringComparison.OrdinalIgnoreCase) &&
                        string.Equals(inproc.GetValue("CodeBase") as string, expectedCodeBase, StringComparison.OrdinalIgnoreCase) &&
                        string.Equals(inproc.GetValue("Assembly") as string, expectedAssembly, StringComparison.Ordinal) &&
                        string.Equals(inproc.GetValue("ThreadingModel") as string, "Apartment", StringComparison.OrdinalIgnoreCase);
                }
            }
            catch
            {
                return false;
            }
        }

        private static void RegisterInRoot(RegRoot root, string fullDllPath, bool perUser)
        {
            string dllDir = Path.GetDirectoryName(fullDllPath) ?? "";
            string cliPath = Path.Combine(dllDir, "pdfcraft-cli.exe");
            string codeBase = ComCodeBase(fullDllPath);

            using (RegistryKey appId = root.CreateSubKey(@"Software\Classes\AppID\" + LinkcoAppId))
            {
                appId.SetValue("", LinkcoAppIdName);
                appId.SetValue("DllSurrogate", PreviewSurrogate, RegistryValueKind.ExpandString);
            }

            using (RegistryKey clsidKey = WriteComServer(root, ClsidBraced, HandlerName, typeof(LinkcoPdfPreviewHandler), HandlerProgId, codeBase))
            {
                clsidKey.SetValue("AppID", LinkcoAppId);
                // The handler starts pdfcraft-cli.exe and reads the PDF by path; the Low-IL sandbox
                // of prevhost.exe would block both.
                clsidKey.SetValue("DisableLowILProcessIsolation", 1, RegistryValueKind.DWord);
            }

            using (RegistryKey handlers = root.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers"))
            {
                handlers.SetValue(ClsidBraced, HandlerName);
            }

            // Windows resolves the preview handler through the default ProgID first (UserChoice, then
            // HKCR\.pdf's default), then .pdf itself, then SystemFileAssociations\.pdf. Register on
            // all of them — but only on ProgIDs that exist and that serve PDF files exclusively.
            BackupAndSetShellEx(root, @"Software\Classes\.pdf\ShellEx\" + PreviewHandlerCategoryGuid, "PreviousPdfPreviewHandler");
            BackupAndSetShellEx(root, @"Software\Classes\SystemFileAssociations\.pdf\ShellEx\" + PreviewHandlerCategoryGuid, "PreviousSysPdfPreviewHandler");

            foreach (string progId in CandidatePdfProgIds(perUser))
            {
                try
                {
                    BackupAndSetShellEx(root, ProgIdShellExPath(progId), "PrevProgId_" + progId);
                }
                catch (Exception ex)
                {
                    PreviewLog.Error("Register ShellEx on " + progId, ex);
                }
            }

            // Repair: earlier builds registered on browser ProgIDs that also open web pages.
            foreach (string progId in PreviouslyTouchedProgIds(root))
            {
                if (IsSharedProgId(progId))
                {
                    try { RestoreOrRemoveShellEx(root, progId, perUser); } catch { }
                }
            }

            // File Explorer thumbnails (icon views) for PDFs. Users who want none turn thumbnails off in
            // Explorer (Folder Options > "Always show icons, never thumbnails").
            try
            {
                RegisterThumbnailProvider(root, codeBase);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("Register thumbnail provider", ex);
            }

            using (RegistryKey cfg = root.CreateSubKey(LinkcoConfigKey))
            {
                if (cfg.GetValue("InstallDir") == null && !string.IsNullOrEmpty(dllDir))
                    cfg.SetValue("InstallDir", dllDir);
                if (File.Exists(cliPath))
                    cfg.SetValue("CliPath", cliPath);
                cfg.SetValue("PreviewHandlerDll", fullDllPath);
                cfg.SetValue("PreviewHandlerDllKey", DllPathKey(fullDllPath));
                cfg.SetValue("PreviewHandlerSchema", RegistrationSchema, RegistryValueKind.DWord);
                // Let Linkco PDF Editor tell cheaply (without loading .NET) whether this registration
                // still matches the DLL on disk and the user's current default PDF app; it registers
                // again when either changes.
                cfg.SetValue("PreviewHandlerDllStamp", DllStamp(fullDllPath));
                if (perUser)
                    cfg.SetValue("PreviewHandlerUserChoice", ReadUserChoiceProgId(".pdf") ?? "");
            }
        }

        /// <summary>
        /// The DLL path in plain ASCII, for Linkco PDF Editor's start-up check, which reads the
        /// registry through reg.exe: reg.exe prints in the console's OEM code page, which can't carry
        /// most non-ASCII text (an Arabic user name in C:\Users\..., say), so comparing the path
        /// itself would fail on every start. ASCII letters are upper-cased (paths compare
        /// case-insensitively); '%', control characters and every non-ASCII UTF-16 code unit become
        /// "%XXXX" (upper-case hex). windows_preview.rs's dll_path_key must produce the same text.
        /// </summary>
        public static string DllPathKey(string path)
        {
            StringBuilder sb = new StringBuilder(path.Length + 16);
            foreach (char c in path)
            {
                if (c >= 'a' && c <= 'z')
                    sb.Append((char)(c - 32));
                else if (c < 0x20 || c > 0x7E || c == '%')
                    sb.Append('%').Append(((int)c).ToString("X4", CultureInfo.InvariantCulture));
                else
                    sb.Append(c);
            }
            return sb.ToString();
        }

        /// <summary>"length:last-write-time-as-UTC-FILETIME" of a file, both decimal.</summary>
        public static string DllStamp(string path)
        {
            try
            {
                FileInfo fi = new FileInfo(path);
                return fi.Length.ToString(CultureInfo.InvariantCulture) + ":" +
                    fi.LastWriteTimeUtc.ToFileTimeUtc().ToString(CultureInfo.InvariantCulture);
            }
            catch
            {
                return "";
            }
        }

        /// <summary>
        /// Writes a .NET COM server registration (mscoree.dll + assembly/class/CodeBase, Apartment
        /// threading) for <paramref name="type"/> and its ProgID, and returns the open CLSID key for
        /// class-specific values. The caller disposes it.
        /// </summary>
        private static RegistryKey WriteComServer(RegRoot root, string clsid, string name, Type type, string progId, string codeBase)
        {
            Assembly asm = type.Assembly;
            string asmFullName = asm.FullName;
            string asmVersion = asm.GetName().Version.ToString();
            string className = type.FullName;

            using (RegistryKey handlerProgId = root.CreateSubKey(@"Software\Classes\" + progId))
            {
                handlerProgId.SetValue("", name);
                using (RegistryKey c = handlerProgId.CreateSubKey("CLSID"))
                {
                    c.SetValue("", clsid);
                }
            }

            RegistryKey clsidKey = root.CreateSubKey(@"Software\Classes\CLSID\" + clsid);
            try
            {
                clsidKey.SetValue("", name);
                clsidKey.SetValue("DisplayName", name);
                using (RegistryKey inproc = clsidKey.CreateSubKey("InprocServer32"))
                {
                    inproc.SetValue("", "mscoree.dll");
                    // COM only understands Apartment/Both/Free/Neutral; the handlers use WinForms/GDI+, so Apartment.
                    inproc.SetValue("ThreadingModel", "Apartment");
                    inproc.SetValue("Class", className);
                    inproc.SetValue("Assembly", asmFullName);
                    inproc.SetValue("RuntimeVersion", "v4.0.30319");
                    inproc.SetValue("CodeBase", codeBase);

                    foreach (string sub in inproc.GetSubKeyNames())
                    {
                        if (!string.Equals(sub, asmVersion, StringComparison.OrdinalIgnoreCase))
                        {
                            try { inproc.DeleteSubKeyTree(sub, false); } catch { }
                        }
                    }
                    using (RegistryKey ver = inproc.CreateSubKey(asmVersion))
                    {
                        ver.SetValue("Class", className);
                        ver.SetValue("Assembly", asmFullName);
                        ver.SetValue("RuntimeVersion", "v4.0.30319");
                        ver.SetValue("CodeBase", codeBase);
                    }
                }
                using (RegistryKey p = clsidKey.CreateSubKey("ProgId"))
                {
                    p.SetValue("", progId);
                }
                using (RegistryKey cat = clsidKey.CreateSubKey(@"Implemented Categories\" + DotNetComCategory))
                {
                }
                return clsidKey;
            }
            catch
            {
                clsidKey.Dispose();
                throw;
            }
        }

        /// <summary>
        /// Registers the thumbnail provider on SystemFileAssociations\.pdf and Linkco's own ProgIDs
        /// only. Explorer consults the default app's ProgID and .pdf before SystemFileAssociations, so
        /// a thumbnail provider another PDF app registered there keeps working.
        /// </summary>
        private static void RegisterThumbnailProvider(RegRoot root, string codeBase)
        {
            string clsid = LinkcoPdfThumbnailProvider.ClsidBraced;
            using (WriteComServer(root, clsid, LinkcoPdfThumbnailProvider.ProviderName, typeof(LinkcoPdfThumbnailProvider), LinkcoPdfThumbnailProvider.ProgIdName, codeBase))
            {
            }
            BackupAndSetShellEx(root, SysPdfThumbnailPath, "PreviousSysPdfThumbnailProvider", clsid);
            foreach (string own in OwnProgIds)
            {
                if (ProgIdExists(own))
                    BackupAndSetShellEx(root, ProgIdShellExPath(own, LinkcoPdfThumbnailProvider.ThumbnailCategoryGuid), "PrevThumbProgId_" + own, clsid);
            }
        }

        private static void UnregisterThumbnailProvider(RegRoot root, bool perUser)
        {
            string clsid = LinkcoPdfThumbnailProvider.ClsidBraced;
            foreach (string own in OwnProgIds)
            {
                try
                {
                    if (RestoreOrRemove(root, ProgIdShellExPath(own, LinkcoPdfThumbnailProvider.ThumbnailCategoryGuid), "PrevThumbProgId_" + own, false, clsid))
                    {
                        DeleteIfEmpty(root, @"Software\Classes\" + own + @"\ShellEx");
                        if (perUser)
                            DeleteIfEmpty(root, @"Software\Classes\" + own);
                    }
                }
                catch { }
            }
            try { RestoreOrRemove(root, SysPdfThumbnailPath, "PreviousSysPdfThumbnailProvider", false, clsid); } catch { }
            DeleteIfEmpty(root, @"Software\Classes\SystemFileAssociations\.pdf\ShellEx");
            try { root.DeleteSubKeyTree(@"Software\Classes\CLSID\" + clsid, false); } catch { }
            try { root.DeleteSubKeyTree(@"Software\Classes\" + LinkcoPdfThumbnailProvider.ProgIdName, false); } catch { }
        }

        private const string SysPdfThumbnailPath = @"Software\Classes\SystemFileAssociations\.pdf\ShellEx\" + LinkcoPdfThumbnailProvider.ThumbnailCategoryGuid;

        private static string ProgIdShellExPath(string progId)
        {
            return ProgIdShellExPath(progId, PreviewHandlerCategoryGuid);
        }

        private static string ProgIdShellExPath(string progId, string category)
        {
            return @"Software\Classes\" + progId + @"\ShellEx\" + category;
        }

        private static List<string> CandidatePdfProgIds(bool perUser)
        {
            List<string> candidates = new List<string>();
            foreach (string p in OwnProgIds)
                AddUnique(candidates, p);
            foreach (string p in KnownPdfProgIds)
                AddUnique(candidates, p);
            if (perUser)
                AddUnique(candidates, ReadUserChoiceProgId(".pdf"));
            AddUnique(candidates, ReadClassesRootDefault(".pdf"));

            List<string> result = new List<string>();
            foreach (string p in candidates)
            {
                if (ProgIdExists(p) && !IsSharedProgId(p))
                    result.Add(p);
            }
            return result;
        }

        private static List<string> PreviouslyTouchedProgIds(RegRoot root)
        {
            List<string> progIds = new List<string>();
            foreach (string p in KnownSharedProgIds)
                AddUnique(progIds, p);
            try
            {
                using (RegistryKey cfg = root.OpenSubKey(LinkcoConfigKey, false))
                {
                    if (cfg != null)
                    {
                        foreach (string name in cfg.GetValueNames())
                        {
                            if (name.StartsWith("PrevProgId_", StringComparison.Ordinal))
                                AddUnique(progIds, name.Substring("PrevProgId_".Length));
                        }
                    }
                }
            }
            catch { }
            return progIds;
        }

        private static void AddUnique(List<string> list, string value)
        {
            if (string.IsNullOrEmpty(value) || value.IndexOf('\\') >= 0)
                return;
            foreach (string existing in list)
            {
                if (string.Equals(existing, value, StringComparison.OrdinalIgnoreCase))
                    return;
            }
            list.Add(value);
        }

        private static bool ProgIdExists(string progId)
        {
            try
            {
                using (RegistryKey hkcr = PreviewEnvironment.OpenHive(RegistryHive.ClassesRoot))
                using (RegistryKey k = hkcr.OpenSubKey(progId, false))
                {
                    return k != null;
                }
            }
            catch
            {
                return false;
            }
        }

        private static string ReadClassesRootDefault(string extension)
        {
            try
            {
                using (RegistryKey hkcr = PreviewEnvironment.OpenHive(RegistryHive.ClassesRoot))
                using (RegistryKey k = hkcr.OpenSubKey(extension, false))
                {
                    return k == null ? null : k.GetValue("") as string;
                }
            }
            catch
            {
                return null;
            }
        }

        private static string ReadUserChoiceProgId(string extension)
        {
            try
            {
                using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
                {
                    return ReadUserChoiceProgId(hkcu, extension);
                }
            }
            catch
            {
                return null;
            }
        }

        /// <summary>The default app's ProgID for <paramref name="extension"/> of the user whose hive <paramref name="hkcu"/> is.</summary>
        private static string ReadUserChoiceProgId(RegRoot hkcu, string extension)
        {
            string basePath = @"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\" + extension;
            try
            {
                using (RegistryKey latest = hkcu.OpenSubKey(basePath + @"\UserChoiceLatest", false))
                {
                    if (latest != null)
                    {
                        string p = latest.GetValue("ProgId") as string;
                        if (string.IsNullOrEmpty(p))
                        {
                            using (RegistryKey sub = latest.OpenSubKey("ProgId", false))
                            {
                                if (sub != null)
                                    p = sub.GetValue("ProgId") as string;
                            }
                        }
                        if (!string.IsNullOrEmpty(p))
                            return p;
                    }
                }
                using (RegistryKey uc = hkcu.OpenSubKey(basePath + @"\UserChoice", false))
                {
                    if (uc != null)
                        return uc.GetValue("ProgId") as string;
                }
            }
            catch { }
            return null;
        }

        /// <summary>
        /// True when a ProgID also serves non-PDF types (e.g. a browser's HTML ProgID): registering a
        /// PDF previewer on it would break previews of web pages and images.
        /// </summary>
        private static bool IsSharedProgId(string progId)
        {
            if (string.IsNullOrEmpty(progId))
                return true;
            foreach (string own in OwnProgIds)
            {
                if (string.Equals(own, progId, StringComparison.OrdinalIgnoreCase))
                    return false;
            }

            string lower = progId.ToLowerInvariant();
            if (lower.Contains("htm") || lower.Contains("url") || lower.StartsWith("operastable", StringComparison.Ordinal))
                return true;
            foreach (string s in KnownSharedProgIds)
            {
                if (lower.StartsWith(s.ToLowerInvariant(), StringComparison.Ordinal))
                    return true;
            }

            foreach (string ext in NonPdfExtensions)
            {
                if (string.Equals(ReadClassesRootDefault(ext), progId, StringComparison.OrdinalIgnoreCase))
                    return true;
            }

            try
            {
                using (RegistryKey hkcu = PreviewEnvironment.OpenHive(RegistryHive.CurrentUser))
                using (RegistryKey fileExts = hkcu.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts", false))
                {
                    if (fileExts != null)
                    {
                        int scanned = 0;
                        foreach (string ext in fileExts.GetSubKeyNames())
                        {
                            if (++scanned > 4000)
                                break;
                            if (string.Equals(ext, ".pdf", StringComparison.OrdinalIgnoreCase))
                                continue;
                            using (RegistryKey uc = fileExts.OpenSubKey(ext + @"\UserChoice", false))
                            {
                                if (uc != null && string.Equals(uc.GetValue("ProgId") as string, progId, StringComparison.OrdinalIgnoreCase))
                                    return true;
                            }
                        }
                    }
                }
            }
            catch { }

            return false;
        }

        private static void BackupAndSetShellEx(RegRoot root, string subKeyPath, string backupValueName)
        {
            BackupAndSetShellEx(root, subKeyPath, backupValueName, ClsidBraced);
        }

        /// <summary>Points a ShellEx handler key at <paramref name="ourClsid"/>, backing up the previous handler once.</summary>
        private static void BackupAndSetShellEx(RegRoot root, string subKeyPath, string backupValueName, string ourClsid)
        {
            string existing = null;
            using (RegistryKey k = root.OpenSubKey(subKeyPath, false))
            {
                if (k != null)
                    existing = k.GetValue("") as string;
            }
            if (!string.IsNullOrEmpty(existing) && !string.Equals(existing, ourClsid, StringComparison.OrdinalIgnoreCase))
            {
                using (RegistryKey cfg = root.CreateSubKey(LinkcoConfigKey))
                {
                    if (cfg.GetValue(backupValueName) == null)
                        cfg.SetValue(backupValueName, existing);
                }
            }
            using (RegistryKey k = root.CreateSubKey(subKeyPath))
            {
                k.SetValue("", ourClsid);
            }
        }

        private static void UnregisterFromRoot(RegRoot root, bool perUser)
        {
            using (RegistryKey handlers = root.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers", true))
            {
                if (handlers != null)
                    handlers.DeleteValue(ClsidBraced, false);
            }

            List<string> progIds = new List<string>();
            foreach (string p in OwnProgIds)
                AddUnique(progIds, p);
            foreach (string p in KnownPdfProgIds)
                AddUnique(progIds, p);
            foreach (string p in PreviouslyTouchedProgIds(root))
                AddUnique(progIds, p);
            if (perUser)
            {
                AddUnique(progIds, ReadUserChoiceProgId(root, ".pdf"));
                // Default apps come and go; find every ProgID of this user still pointing at us.
                foreach (string p in ProgIdsPointingAtUs(root))
                    AddUnique(progIds, p);
            }
            AddUnique(progIds, ReadClassesRootDefault(".pdf"));

            foreach (string progId in progIds)
            {
                try { RestoreOrRemoveShellEx(root, progId, perUser); } catch { }
            }

            UnregisterThumbnailProvider(root, perUser);

            RestoreOrRemove(root, @"Software\Classes\.pdf\ShellEx\" + PreviewHandlerCategoryGuid, "PreviousPdfPreviewHandler", true);
            RestoreOrRemove(root, @"Software\Classes\SystemFileAssociations\.pdf\ShellEx\" + PreviewHandlerCategoryGuid, "PreviousSysPdfPreviewHandler", false);
            DeleteIfEmpty(root, @"Software\Classes\.pdf\ShellEx");
            DeleteIfEmpty(root, @"Software\Classes\SystemFileAssociations\.pdf\ShellEx");

            root.DeleteSubKeyTree(@"Software\Classes\CLSID\" + ClsidBraced, false);
            root.DeleteSubKeyTree(@"Software\Classes\" + HandlerProgId, false);
            try { root.DeleteSubKeyTree(@"Software\Classes\AppID\" + LinkcoAppId, false); } catch { }

            using (RegistryKey cfg = root.OpenSubKey(LinkcoConfigKey, true))
            {
                if (cfg != null)
                {
                    cfg.DeleteValue("PreviewHandlerDll", false);
                    cfg.DeleteValue("PreviewHandlerDllKey", false);
                    cfg.DeleteValue("PreviewHandlerSchema", false);
                    cfg.DeleteValue("PreviewHandlerDllStamp", false);
                    cfg.DeleteValue("PreviewHandlerUserChoice", false);
                }
            }
        }

        /// <summary>
        /// Removes the per-user registration from one user's registry (Software and Classes may be
        /// separate hives). Returns false, touching nothing, when that user has none.
        /// </summary>
        internal static bool CleanUserHive(RegistryKey software, RegistryKey classes)
        {
            return CleanUserRoot(new RegRoot(software, classes));
        }

        private static bool CleanUserRoot(RegRoot root)
        {
            if (!HasPerUserRegistration(root))
                return false;
            UnregisterFromRoot(root, true);
            return true;
        }

        private static bool HasPerUserRegistration(RegRoot root)
        {
            foreach (string clsid in new string[] { ClsidBraced, LinkcoPdfThumbnailProvider.ClsidBraced })
            {
                using (RegistryKey k = root.OpenSubKey(@"Software\Classes\CLSID\" + clsid, false))
                {
                    if (k != null)
                        return true;
                }
            }
            using (RegistryKey cfg = root.OpenSubKey(LinkcoConfigKey, false))
            {
                if (cfg != null)
                {
                    foreach (string name in cfg.GetValueNames())
                    {
                        if (name.StartsWith("Prev", StringComparison.Ordinal))
                            return true; // PreviewHandler* and the Prev*/Previous* backups
                    }
                }
            }
            return ProgIdsPointingAtUs(root).Count > 0;
        }

        /// <summary>ProgIDs in <paramref name="root"/>'s classes whose preview handler is ours.</summary>
        private static List<string> ProgIdsPointingAtUs(RegRoot root)
        {
            List<string> result = new List<string>();
            string[] names;
            try
            {
                names = root.ClassesSubKeyNames();
            }
            catch
            {
                return result;
            }
            int scanned = 0;
            foreach (string name in names)
            {
                if (++scanned > 100000)
                    break;
                // Extensions (.pdf is handled with its own backup) and CLSID/AppID/... containers aren't ProgIDs.
                if (name.Length == 0 || name[0] == '.' || name.IndexOf('\\') >= 0 ||
                    string.Equals(name, "CLSID", StringComparison.OrdinalIgnoreCase) ||
                    string.Equals(name, "SystemFileAssociations", StringComparison.OrdinalIgnoreCase))
                    continue;
                try
                {
                    using (RegistryKey k = root.OpenSubKey(ProgIdShellExPath(name), false))
                    {
                        if (k != null && string.Equals(k.GetValue("") as string, ClsidBraced, StringComparison.OrdinalIgnoreCase))
                            result.Add(name);
                    }
                }
                catch { }
            }
            return result;
        }

        private static void RestoreOrRemoveShellEx(RegRoot root, string progId, bool perUser)
        {
            bool removed = RestoreOrRemove(root, ProgIdShellExPath(progId), "PrevProgId_" + progId, false);
            if (removed)
            {
                DeleteIfEmpty(root, @"Software\Classes\" + progId + @"\ShellEx");
                // Only per-user: an empty HKCU ProgID key would otherwise hide nothing but is clutter.
                if (perUser)
                    DeleteIfEmpty(root, @"Software\Classes\" + progId);
            }
        }

        /// <summary>Restores the previous handler (or removes ours). Returns true when the key was deleted.</summary>
        private static bool RestoreOrRemove(RegRoot root, string subKeyPath, string backupValueName, bool edgeFallback)
        {
            return RestoreOrRemove(root, subKeyPath, backupValueName, edgeFallback, ClsidBraced);
        }

        private static bool RestoreOrRemove(RegRoot root, string subKeyPath, string backupValueName, bool edgeFallback, string ourClsid)
        {
            string current = null;
            using (RegistryKey k = root.OpenSubKey(subKeyPath, false))
            {
                if (k != null)
                    current = k.GetValue("") as string;
            }

            string backup = null;
            using (RegistryKey cfg = root.OpenSubKey(LinkcoConfigKey, true))
            {
                if (cfg != null)
                {
                    backup = cfg.GetValue(backupValueName) as string;
                    if (string.Equals(current, ourClsid, StringComparison.OrdinalIgnoreCase) || current == null)
                        cfg.DeleteValue(backupValueName, false);
                }
            }

            if (!string.Equals(current, ourClsid, StringComparison.OrdinalIgnoreCase))
                return false;

            string restore = null;
            if (!string.IsNullOrEmpty(backup) && ClsidExists(root, backup))
                restore = backup;
            else if (edgeFallback && root.Name.StartsWith("HKEY_LOCAL_MACHINE", StringComparison.OrdinalIgnoreCase) && ClsidExists(root, EdgePreviewHandlerClsid))
                restore = EdgePreviewHandlerClsid;

            if (restore != null)
            {
                using (RegistryKey k = root.CreateSubKey(subKeyPath))
                {
                    k.SetValue("", restore);
                }
                return false;
            }

            root.DeleteSubKeyTree(subKeyPath, false);
            return true;
        }

        private static void DeleteIfEmpty(RegRoot root, string subKeyPath)
        {
            try
            {
                bool empty;
                using (RegistryKey k = root.OpenSubKey(subKeyPath, false))
                {
                    if (k == null)
                        return;
                    empty = k.SubKeyCount == 0 && k.ValueCount == 0;
                }
                if (empty)
                    root.DeleteSubKey(subKeyPath, false);
            }
            catch { }
        }

        /// <summary>
        /// True when <paramref name="clsid"/> is registered machine-wide or in <paramref name="root"/>
        /// (whose user may not be the one running this code).
        /// </summary>
        private static bool ClsidExists(RegRoot root, string clsid)
        {
            try
            {
                using (RegistryKey k = root.OpenSubKey(@"Software\Classes\CLSID\" + clsid, false))
                {
                    if (k != null)
                        return true;
                }
                using (RegistryKey hklm = PreviewEnvironment.OpenHive(RegistryHive.LocalMachine))
                using (RegistryKey k = hklm.OpenSubKey(@"Software\Classes\CLSID\" + clsid, false))
                {
                    return k != null;
                }
            }
            catch
            {
                return false;
            }
        }

        /// <summary>Restarts the preview host so Explorer picks up a new or updated handler DLL.</summary>
        private static void StopStalePrevHost()
        {
            try
            {
                int selfPid = Process.GetCurrentProcess().Id;
                foreach (Process p in Process.GetProcessesByName("prevhost"))
                {
                    try
                    {
                        if (p.Id != selfPid)
                        {
                            p.Kill();
                            p.WaitForExit(2000);
                        }
                    }
                    catch { }
                    finally
                    {
                        try { p.Dispose(); } catch { }
                    }
                }
            }
            catch { }
        }
    }

    /// <summary>
    /// File Explorer thumbnails (Medium/Large/Extra large icons, Tiles, Content views) for PDF files:
    /// the first page, rendered by pdfcraft-cli.exe exactly like the preview pane. Windows runs
    /// thumbnail providers in its isolated thumbnail process (dllhost.exe) and hands them a stream;
    /// it is copied to a private temp file so the PDF is never kept locked. A damaged or hostile PDF
    /// can only crash or hang the short-lived child process, which is killed after a timeout.
    /// </summary>
    [ComVisible(true)]
    [ClassInterface(ClassInterfaceType.None)]
    [ProgId(LinkcoPdfThumbnailProvider.ProgIdName)]
    [Guid(LinkcoPdfThumbnailProvider.ClsidString)]
    public sealed class LinkcoPdfThumbnailProvider : IThumbnailProvider, IInitializeWithStream, IInitializeWithFile
    {
        public const string ClsidString = "3D8CDE4B-E969-481F-BEB0-5E3B98287416";
        public const string ClsidBraced = "{3D8CDE4B-E969-481F-BEB0-5E3B98287416}";
        public const string ProgIdName = "LinkcoPDFEditor.ThumbnailProvider";
        public const string ProviderName = "Linkco PDF Thumbnail Provider";
        public const string ThumbnailCategoryGuid = "{e357fccd-a995-4576-b01f-234630154e96}";

        /// <summary>Explorer asks for at most 2560 px (Windows 11 "Extra large icons" at high DPI).</summary>
        private const int MaxThumbnailPx = 2560;
        private const int TimeoutMs = 8000;
        private const int S_OK = 0;
        private const int E_FAIL = unchecked((int)0x80004005);
        private const int E_UNEXPECTED = unchecked((int)0x8000FFFF);
        private const uint WTSAT_RGB = 1;

        private string _pdfPath;
        private string _tempPath;

        public LinkcoPdfThumbnailProvider()
        {
            try { PreviewLog.InstallCrashLogger(); } catch { }
            try { PreviewEnvironment.SweepStaleFilesOnce(); } catch { }
        }

        public void Initialize(string pszFilePath, uint grfMode)
        {
            ReleaseTemp();
            _pdfPath = pszFilePath;
        }

        public void Initialize(IStream pstream, uint grfMode)
        {
            ReleaseTemp();
            try
            {
                string displayName;
                _tempPath = PreviewEnvironment.CopyStreamToTempFile(pstream, "thumb-", out displayName);
                _pdfPath = _tempPath;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("Thumbnail IInitializeWithStream.Initialize", ex);
                throw;
            }
        }

        public int GetThumbnail(uint cx, out IntPtr phbmp, out uint pdwAlpha)
        {
            phbmp = IntPtr.Zero;
            pdwAlpha = WTSAT_RGB;
            try
            {
                if (string.IsNullOrEmpty(_pdfPath) || cx == 0)
                    return E_UNEXPECTED;
                string cli = PreviewEnvironment.LocateCliExecutable();
                if (cli == null)
                {
                    PreviewLog.Write("thumbnail: pdfcraft-cli.exe was not found");
                    return E_FAIL;
                }

                int size = (int)Math.Min(cx, (uint)MaxThumbnailPx);
                using (Bitmap page = RenderFirstPage(cli, _pdfPath, size))
                {
                    if (page == null)
                        return E_FAIL;
                    Bitmap fitted = FitWithin(page, size);
                    try
                    {
                        phbmp = (fitted ?? page).GetHbitmap(Color.White);
                    }
                    finally
                    {
                        if (fitted != null)
                            fitted.Dispose();
                    }
                }
                return phbmp == IntPtr.Zero ? E_FAIL : S_OK;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("GetThumbnail", ex);
                return E_FAIL;
            }
            finally
            {
                ReleaseTemp();
            }
        }

        private void ReleaseTemp()
        {
            if (_tempPath != null)
            {
                PreviewEnvironment.DeleteFileSoon(_tempPath);
                if (string.Equals(_pdfPath, _tempPath, StringComparison.OrdinalIgnoreCase))
                    _pdfPath = null;
                _tempPath = null;
            }
        }

        /// <summary>Page 1 fitted into a size x size box, or null (password-protected, damaged, timeout...).</summary>
        private static Bitmap RenderFirstPage(string cli, string pdf, int size)
        {
            string px = Math.Max(1, size).ToString(CultureInfo.InvariantCulture);
            string outPng = Path.Combine(
                PreviewEnvironment.TempDir,
                "thumb-" + Process.GetCurrentProcess().Id.ToString(CultureInfo.InvariantCulture) + "-" + Guid.NewGuid().ToString("N") + ".png");
            try
            {
                string arguments = "preview " + PreviewPaneControl.QuoteArg(pdf) + " --page 1 --width " + px + " --max-px " + px +
                    " --out " + PreviewPaneControl.QuoteArg(outPng);
                string stdout;
                using (CliProcess proc = CliProcess.Start(cli, arguments, "thumb"))
                {
                    if (!proc.WaitForExit(TimeoutMs))
                    {
                        proc.Kill();
                        proc.WaitForExit(2000);
                        PreviewLog.Write("thumbnail timed out after " + TimeoutMs.ToString(CultureInfo.InvariantCulture) + " ms");
                        return null;
                    }
                    stdout = proc.ReadStdout();
                }

                string status = FindStatusLine(stdout);
                if (status == null || !status.StartsWith("STATUS\tOK\t", StringComparison.Ordinal))
                {
                    PreviewLog.Write("thumbnail not rendered: " + (status ?? "(no status)").Replace('\t', ' '));
                    return null;
                }
                if (!File.Exists(outPng))
                    return null;
                return PreviewPaneControl.LoadPremultiplied(outPng);
            }
            finally
            {
                PreviewEnvironment.DeleteFileSoon(outPng);
            }
        }

        internal static string FindStatusLine(string stdout)
        {
            using (StringReader sr = new StringReader(stdout ?? ""))
            {
                string line;
                while ((line = sr.ReadLine()) != null)
                {
                    if (line.StartsWith("STATUS\t", StringComparison.Ordinal))
                        return line;
                }
            }
            return null;
        }

        /// <summary>A copy scaled down to fit a size x size box, or null when it already fits.</summary>
        internal static Bitmap FitWithin(Bitmap source, int size)
        {
            int w = source.Width;
            int h = source.Height;
            if (w <= size && h <= size)
                return null;
            double scale = Math.Min((double)size / w, (double)size / h);
            int nw = Math.Max(1, Math.Min(size, (int)Math.Round(w * scale)));
            int nh = Math.Max(1, Math.Min(size, (int)Math.Round(h * scale)));
            Bitmap dst = new Bitmap(nw, nh, PixelFormat.Format32bppPArgb);
            try
            {
                using (Graphics g = Graphics.FromImage(dst))
                using (ImageAttributes attrs = new ImageAttributes())
                {
                    g.Clear(Color.White);
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    g.CompositingQuality = CompositingQuality.HighQuality;
                    attrs.SetWrapMode(WrapMode.TileFlipXY); // no dark fringe at the edges
                    g.DrawImage(source, new Rectangle(0, 0, nw, nh), 0, 0, w, h, GraphicsUnit.Pixel, attrs);
                }
                return dst;
            }
            catch
            {
                dst.Dispose();
                throw;
            }
        }
    }

    /// <summary>One page raster request handed to the worker thread.</summary>
    internal sealed class RenderJob
    {
        public int Generation;
        public string PdfPath;
        public string CliPath;
        public int Page;
        public int Width;
    }

    /// <summary>Outcome of a page render, marshalled back to the UI thread.</summary>
    internal sealed class RenderResult
    {
        public int Generation;
        public int Page;
        public int RequestedWidth;
        public int TotalPages;
        public Bitmap Bitmap;
        public string Code;
        public string Message;
        public bool Cancelled;

        public void DisposeBitmap()
        {
            if (Bitmap != null)
            {
                try { Bitmap.Dispose(); } catch { }
                Bitmap = null;
            }
        }
    }

    internal sealed class CachedPage
    {
        public Bitmap Bitmap;
        public int RequestedWidth;

        public long Bytes { get { return Bitmap == null ? 0 : 4L * Bitmap.Width * Bitmap.Height; } }
    }

    /// <summary>
    /// The preview pane: a single custom-painted child window with the Linkco toolbar (page
    /// navigation, zoom) and the page raster. All state is touched on the preview host's UI thread
    /// only; rendering happens on a worker thread and is posted back with BeginInvoke.
    /// </summary>
    internal sealed class PreviewPaneControl : Control
    {
        private enum ToolButton { None, Prev, Next, ZoomOut, Fit, ZoomIn }

        private const int MaxRasterPx = 4096;
        private const int RenderTimeoutMs = 30000;
        /// <summary>After this long without a page, "Loading page N…" says the page is slow.</summary>
        private const int SlowNoticeMs = 8000;
        /// <summary>
        /// The watchdog gives up this long after the engine's own timeout, so the pane can never stay
        /// on "Loading…" forever, whatever happened to the render (lost result, stuck host, …).
        /// </summary>
        private const int WatchdogGraceMs = 10000;
        private const long CacheBudgetBytes = 96L * 1024 * 1024;
        private const float MinZoom = 0.25f;
        private const float MaxZoom = 4.0f;

        private static readonly Color BrandNavy = Color.FromArgb(1, 19, 28);
        private static readonly Color BrandRed = Color.FromArgb(242, 36, 36);

        private readonly object _sync = new object();
        private int _generation;
        private bool _shutdown;
        private CliProcess _activeProcess;
        /// <summary>The newest finished render, waiting for the UI thread (guarded by _sync).</summary>
        private RenderResult _completed;

        private readonly Dictionary<int, CachedPage> _cache = new Dictionary<int, CachedPage>();
        private readonly LinkedList<int> _lru = new LinkedList<int>();
        private readonly System.Windows.Forms.Timer _rerenderTimer;
        private readonly System.Windows.Forms.Timer _watchdog;
        private int _loadingSince;
        private int _loadingPage;
        private bool _loadingMessage;
        private bool _slowNoticeShown;
        private int _retryPage;

        private string _pdfPath;
        private string _displayName;
        private int _currentPage = 1;
        private int _totalPages;
        private float _zoom = 1.0f;
        private bool _fitWidth = true;
        private int _scrollX;
        private int _scrollY;
        private bool _loading;
        private string _message = "Select a PDF document to preview.";
        private bool _messageIsError;

        private ToolButton _hover = ToolButton.None;
        private ToolButton _pressed = ToolButton.None;
        private bool _dragging;
        private Point _dragStart;
        private int _dragScrollX;
        private int _dragScrollY;

        private Color _back = Color.FromArgb(242, 243, 245);
        private Color _text = Color.FromArgb(70, 76, 86);
        private bool _textFromHost;
        private string _fontFace = "Segoe UI";
        private float _scale = 1.0f;
        private Font _statusFont;
        private Font _labelFont;
        private Font _buttonFont;
        private Bitmap _display;
        private Bitmap _displaySource;

        public PreviewPaneControl()
        {
            SetStyle(
                ControlStyles.AllPaintingInWmPaint |
                ControlStyles.UserPaint |
                ControlStyles.OptimizedDoubleBuffer |
                ControlStyles.ResizeRedraw |
                ControlStyles.Selectable,
                true);
            DoubleBuffered = true;
            BackColor = _back;
            TabStop = true;
            AccessibleName = "Linkco PDF Preview";
            AccessibleRole = AccessibleRole.Graphic;

            _rerenderTimer = new System.Windows.Forms.Timer();
            _rerenderTimer.Interval = 250;
            _rerenderTimer.Tick += OnRerenderTimer;

            _watchdog = new System.Windows.Forms.Timer();
            _watchdog.Interval = 500;
            _watchdog.Tick += OnWatchdogTick;
        }

        // ------------------------------------------------------------------ public API (UI thread)

        public void ApplyTheme(Color? background, Color? text, string fontFace)
        {
            if (background.HasValue)
            {
                _back = background.Value;
                BackColor = _back;
            }
            if (text.HasValue)
            {
                _text = text.Value;
                _textFromHost = true;
            }
            else if (!_textFromHost)
            {
                _text = IsDark(_back) ? Color.FromArgb(220, 224, 230) : Color.FromArgb(70, 76, 86);
            }
            if (!string.IsNullOrEmpty(fontFace) && !string.Equals(fontFace, _fontFace, StringComparison.OrdinalIgnoreCase))
            {
                _fontFace = fontFace;
                DisposeFonts();
            }
            Invalidate();
        }

        public void RefreshDpi()
        {
            float newScale = 1.0f;
            int dpi = IsHandleCreated ? NativeMethods.GetWindowDpi(Handle) : 0;
            if (dpi > 0)
            {
                newScale = dpi / 96.0f;
            }
            else
            {
                try
                {
                    using (Graphics g = CreateGraphics())
                        newScale = g.DpiX / 96.0f;
                }
                catch { }
            }
            newScale = Math.Max(1.0f, Math.Min(4.0f, newScale));
            if (Math.Abs(newScale - _scale) > 0.01f)
            {
                _scale = newScale;
                DisposeFonts();
                Invalidate();
            }
        }

        public void LoadDocument(string filePath, string displayName, string initError)
        {
            CancelRendering();
            ClearCache();
            _pdfPath = filePath;
            _displayName = displayName;
            _currentPage = 1;
            _totalPages = 0;
            _fitWidth = true;
            _zoom = 1.0f;
            _scrollX = 0;
            _scrollY = 0;

            if (!string.IsNullOrEmpty(initError))
            {
                ShowMessage(initError, true);
                return;
            }
            if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            {
                ShowMessage("The selected PDF document could not be found.", true);
                return;
            }
            RequestRender(1);
        }

        public void ShowMessage(string message, bool isError)
        {
            _loading = false;
            _loadingMessage = false;
            _retryPage = 0;
            _message = message;
            _messageIsError = isError;
            Invalidate();
        }

        /// <summary>Cancels the render in flight (kills pdfcraft-cli.exe) without tearing down the pane.</summary>
        public void CancelRendering()
        {
            RenderResult dropped;
            lock (_sync)
            {
                _generation++;
                KillActiveProcessLocked();
                dropped = _completed;
                _completed = null;
            }
            if (dropped != null)
                dropped.DisposeBitmap();
            _loading = false;
            try { _watchdog.Stop(); } catch { }
            if (_loadingMessage)
            {
                // Never leave a "Loading page N…" behind for a render that no longer exists.
                _loadingMessage = false;
                _message = null;
                Invalidate();
            }
        }

        public void Shutdown()
        {
            lock (_sync)
            {
                _shutdown = true;
                _generation++;
                KillActiveProcessLocked();
            }
            try { _rerenderTimer.Stop(); } catch { }
            try { _watchdog.Stop(); } catch { }
            RenderResult dropped;
            lock (_sync)
            {
                dropped = _completed;
                _completed = null;
            }
            if (dropped != null)
                dropped.DisposeBitmap();
            ClearCache();
        }

        // ------------------------------------------------------------------ rendering

        private CachedPage CurrentRaster
        {
            get
            {
                CachedPage c;
                return _cache.TryGetValue(_currentPage, out c) ? c : null;
            }
        }

        private int ComputeRequestWidth()
        {
            int clientW = ClientSize.Width;
            if (clientW < 50)
                clientW = 800;
            int canvasW = Math.Max(1, clientW - 2 * PageMargin);
            float z = _fitWidth ? 1.0f : _zoom;
            int w = (int)Math.Ceiling(canvasW * z);
            w = ((w + 127) / 128) * 128;
            return Math.Max(256, Math.Min(MaxRasterPx, w));
        }

        private int PageMargin
        {
            get { return (int)Math.Round(12 * _scale); }
        }

        private void RequestRender(int page)
        {
            if (string.IsNullOrEmpty(_pdfPath))
                return;

            string cli = PreviewEnvironment.LocateCliExecutable();
            if (string.IsNullOrEmpty(cli))
            {
                PreviewLog.Write("pdfcraft-cli.exe not found");
                ShowMessage("The Linkco PDF preview engine (pdfcraft-cli.exe) was not found. Please reinstall Linkco PDF Editor.", true);
                return;
            }

            int generation;
            lock (_sync)
            {
                if (_shutdown)
                    return;
                _generation++;
                generation = _generation;
                KillActiveProcessLocked();
            }

            RenderJob job = new RenderJob
            {
                Generation = generation,
                PdfPath = _pdfPath,
                CliPath = cli,
                Page = page,
                Width = ComputeRequestWidth()
            };

            _loading = true;
            _loadingSince = Environment.TickCount;
            _loadingPage = page;
            _slowNoticeShown = false;
            _retryPage = 0;
            if (CurrentRaster == null)
            {
                _message = "Loading page " + page.ToString(CultureInfo.CurrentCulture) + "\u2026";
                _messageIsError = false;
                _loadingMessage = true;
            }
            _watchdog.Stop();
            _watchdog.Start();
            Invalidate();

            ThreadPool.QueueUserWorkItem(RenderWorker, job);
        }

        private void RenderWorker(object state)
        {
            // An exception escaping a thread-pool thread terminates the preview host.
            try { RenderWorkerCore(state); }
            catch (Exception ex) { PreviewLog.Error("RenderWorker (the watchdog will give up on this page)", ex); }
        }

        [MethodImpl(MethodImplOptions.NoInlining)]
        private void RenderWorkerCore(object state)
        {
            RenderJob job = (RenderJob)state;
            RenderResult result;
            try
            {
                result = RunRender(job);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("RenderWorker", ex);
                result = new RenderResult { Generation = job.Generation, Page = job.Page, Code = "ERROR", Message = ex.Message };
            }

            if (result.Cancelled)
            {
                result.DisposeBitmap();
                return;
            }
            DeliverResult(result);
        }

        /// <summary>
        /// Hands a finished render to the UI thread. The result is parked in a slot that the UI
        /// thread drains, both when the BeginInvoke nudge arrives and on every watchdog tick, so a
        /// lost or failed BeginInvoke (window handle not ready, recreated, …) can't strand the pane
        /// on "Loading…".
        /// </summary>
        private void DeliverResult(RenderResult result)
        {
            RenderResult discard;
            bool parked;
            lock (_sync)
            {
                parked = !_shutdown && result.Generation == _generation;
                if (parked)
                {
                    discard = _completed;
                    _completed = result;
                }
                else
                {
                    discard = result;
                }
            }
            if (discard != null)
                discard.DisposeBitmap();
            if (!parked)
                return;
            try
            {
                if (!IsDisposed && IsHandleCreated)
                    BeginInvoke(new MethodInvoker(DrainCompleted));
            }
            catch (Exception ex)
            {
                PreviewLog.Error("DeliverResult (the watchdog will deliver it)", ex);
            }
        }

        /// <summary>UI thread: applies the parked render result, if any.</summary>
        private void DrainCompleted()
        {
            RenderResult r;
            lock (_sync)
            {
                r = _completed;
                _completed = null;
            }
            if (r == null)
                return;
            try
            {
                OnRenderComplete(r);
            }
            catch (Exception ex)
            {
                // An exception here would reach WinForms' thread-exception handler, which in the preview
                // host means an error dialog or a dead host ("an error in the Linkco PDF Preview Handler").
                PreviewLog.Error("OnRenderComplete", ex);
                r.DisposeBitmap();
                try { ShowMessage("This PDF document could not be previewed.", true); } catch { }
            }
        }

        private void OnWatchdogTick(object sender, EventArgs e)
        {
            try
            {
                WatchdogTick();
            }
            catch (Exception ex)
            {
                PreviewLog.Error("Watchdog", ex);
            }
        }

        private void WatchdogTick()
        {
            if (IsDisposed)
                return;
            DrainCompleted();
            if (!_loading)
            {
                _watchdog.Stop();
                return;
            }

            int elapsed = unchecked(Environment.TickCount - _loadingSince);
            string pageText = _loadingPage.ToString(CultureInfo.CurrentCulture);
            if (elapsed > RenderTimeoutMs + WatchdogGraceMs)
            {
                PreviewLog.Write("watchdog: page " + pageText + " still loading after " + elapsed.ToString(CultureInfo.InvariantCulture) + " ms; giving up");
                int page = _loadingPage;
                CancelRendering();
                if (page == _currentPage && CurrentRaster != null)
                {
                    Invalidate();
                    return;
                }
                ShowMessage("The preview did not finish loading. Click to try again, or open the document in Linkco PDF Editor.", true);
                _retryPage = page;
                return;
            }

            if (!_slowNoticeShown && elapsed > SlowNoticeMs && _loadingMessage)
            {
                _slowNoticeShown = true;
                _message = "Still loading page " + pageText + "\u2026 Large or complex pages can take a little longer.";
                Invalidate();
            }
        }

        private bool IsCurrent(int generation)
        {
            lock (_sync)
            {
                return !_shutdown && generation == _generation;
            }
        }

        private void KillActiveProcessLocked()
        {
            CliProcess p = _activeProcess;
            _activeProcess = null;
            if (p != null)
                p.Kill();
        }

        private RenderResult RunRender(RenderJob job)
        {
            RenderResult res = new RenderResult { Generation = job.Generation, Page = job.Page, RequestedWidth = job.Width };
            string outPng = Path.Combine(
                PreviewEnvironment.TempDir,
                "page-" + Process.GetCurrentProcess().Id.ToString(CultureInfo.InvariantCulture) + "-" + Guid.NewGuid().ToString("N") + ".png");

            string stdout = "";
            string stderr = "";
            bool timedOut = false;
            Stopwatch clock = Stopwatch.StartNew();
            string what = "page " + job.Page.ToString(CultureInfo.InvariantCulture) + " at " + job.Width.ToString(CultureInfo.InvariantCulture) + " px";

            if (!IsCurrent(job.Generation))
            {
                res.Cancelled = true;
                return res;
            }

            try
            {
                string arguments = "preview " + QuoteArg(job.PdfPath) +
                    " --page " + job.Page.ToString(CultureInfo.InvariantCulture) +
                    " --width " + job.Width.ToString(CultureInfo.InvariantCulture) +
                    " --max-px " + MaxRasterPx.ToString(CultureInfo.InvariantCulture) +
                    " --out " + QuoteArg(outPng);

                using (CliProcess proc = CliProcess.Start(job.CliPath, arguments, "page"))
                {
                    try
                    {
                        lock (_sync)
                        {
                            if (_shutdown || job.Generation != _generation)
                            {
                                proc.Kill();
                                res.Cancelled = true;
                                return res;
                            }
                            _activeProcess = proc;
                        }

                        if (!proc.WaitForExit(RenderTimeoutMs))
                        {
                            timedOut = true;
                            proc.Kill();
                            proc.WaitForExit(2000);
                        }
                        else
                        {
                            stdout = proc.ReadStdout();
                            stderr = proc.ReadStderr();
                        }
                    }
                    finally
                    {
                        lock (_sync)
                        {
                            if (_activeProcess == proc)
                                _activeProcess = null;
                        }
                    }
                }

                if (!IsCurrent(job.Generation))
                {
                    res.Cancelled = true;
                    return res;
                }

                if (timedOut)
                {
                    PreviewLog.Write("render " + what + " timed out after " + RenderTimeoutMs.ToString(CultureInfo.InvariantCulture) + " ms");
                    res.Code = "TIMEOUT";
                    res.Message = "This page is taking too long to preview. Click to try again, or open the document in Linkco PDF Editor.";
                    return res;
                }

                ParseStatus(stdout, stderr, outPng, res);
                PreviewLog.Write("render " + what + ": " + (res.Code ?? "?") + " in " + clock.ElapsedMilliseconds.ToString(CultureInfo.InvariantCulture) + " ms");
            }
            catch (Exception ex)
            {
                PreviewLog.Error("RunRender (" + job.CliPath + ")", ex);
                res.Code = "ERROR";
                res.Message = "The Linkco PDF preview engine could not be started.";
            }
            finally
            {
                PreviewEnvironment.DeleteFileSoon(outPng);
            }
            return res;
        }

        private static void ParseStatus(string stdout, string stderr, string outPng, RenderResult res)
        {
            string statusLine = null;
            using (StringReader sr = new StringReader(stdout ?? ""))
            {
                string line;
                while ((line = sr.ReadLine()) != null)
                {
                    if (line.StartsWith("STATUS\t", StringComparison.Ordinal))
                    {
                        statusLine = line;
                        break;
                    }
                }
            }

            if (statusLine == null)
            {
                string err = LastLine(stderr);
                PreviewLog.Write("preview engine produced no status: " + (err ?? "(no output)"));
                res.Code = "ERROR";
                res.Message = "This PDF document could not be previewed. It may be damaged or use unsupported features.";
                return;
            }

            string[] parts = statusLine.Split('\t');
            res.Code = parts.Length > 1 ? parts[1] : "ERROR";
            res.TotalPages = parts.Length > 2 ? ParseInt(parts[2], 0) : 0;

            switch (res.Code)
            {
                case "OK":
                    res.Page = parts.Length > 3 ? ParseInt(parts[3], res.Page) : res.Page;
                    if (!File.Exists(outPng))
                    {
                        res.Code = "ERROR";
                        res.Message = "The page image could not be created.";
                        return;
                    }
                    try
                    {
                        res.Bitmap = LoadPremultiplied(outPng);
                    }
                    catch (Exception ex)
                    {
                        PreviewLog.Error("LoadPremultiplied", ex);
                        res.Code = "ERROR";
                        res.Message = "The page image could not be loaded.";
                    }
                    return;
                case "PASSWORD_REQUIRED":
                    res.Message = "This PDF document is password-protected. Open it in Linkco PDF Editor to enter the password.";
                    return;
                case "EMPTY_PDF":
                    res.Message = "This PDF document contains no pages.";
                    return;
                case "IO_ERROR":
                    res.Message = "The PDF document could not be read. It may be open in another program or no longer available.";
                    return;
                case "INVALID_PDF":
                    res.Message = "This file is not a valid PDF document, or it is damaged.";
                    return;
                case "RENDER_ERROR":
                    res.Message = "This page could not be previewed. Open the document in Linkco PDF Editor to view it.";
                    return;
                default:
                    res.Message = parts.Length > 3 && !string.IsNullOrEmpty(parts[3])
                        ? parts[3]
                        : "This PDF document could not be previewed.";
                    return;
            }
        }

        internal static Bitmap LoadPremultiplied(string path)
        {
            byte[] bytes = File.ReadAllBytes(path);
            using (MemoryStream ms = new MemoryStream(bytes))
            using (Image img = Image.FromStream(ms, false, true))
            {
                Bitmap bmp = new Bitmap(img.Width, img.Height, PixelFormat.Format32bppPArgb);
                try
                {
                    using (Graphics g = Graphics.FromImage(bmp))
                    {
                        g.Clear(Color.White);
                        g.CompositingMode = CompositingMode.SourceOver;
                        g.InterpolationMode = InterpolationMode.NearestNeighbor;
                        g.DrawImage(img, new Rectangle(0, 0, img.Width, img.Height));
                    }
                    return bmp;
                }
                catch
                {
                    bmp.Dispose();
                    throw;
                }
            }
        }

        private void OnRenderComplete(RenderResult r)
        {
            if (IsDisposed || r.Cancelled || !IsCurrent(r.Generation))
            {
                r.DisposeBitmap();
                return;
            }

            _loading = false;
            _loadingMessage = false;
            if (r.TotalPages > 0)
                _totalPages = r.TotalPages;

            if (r.Bitmap != null)
            {
                StoreRaster(r.Page, r.Bitmap, r.RequestedWidth);
                r.Bitmap = null;
                if (_currentPage != r.Page)
                {
                    _currentPage = r.Page;
                    _scrollX = 0;
                    _scrollY = 0;
                }
                _message = null;
                _messageIsError = false;
                ClampScroll();
                Invalidate();
                if (ComputeRequestWidth() > r.RequestedWidth)
                    ScheduleSharperRaster();
                return;
            }

            PreviewLog.Write("preview status " + (r.Code ?? "?") + " for page " + r.Page.ToString(CultureInfo.InvariantCulture));
            if (r.Page == _currentPage && CurrentRaster != null)
            {
                // A sharper re-render failed (e.g. timed out at a high zoom): keep showing what we have.
                Invalidate();
                return;
            }
            ShowMessage(r.Message ?? "This PDF document could not be previewed.", true);
            if (r.Code == "TIMEOUT" || r.Code == "ERROR")
                _retryPage = r.Page;
        }

        private void StoreRaster(int page, Bitmap bmp, int requestedWidth)
        {
            CachedPage old;
            if (_cache.TryGetValue(page, out old))
            {
                if (_displaySource == old.Bitmap)
                    DisposeDisplay();
                if (old.Bitmap != null)
                    old.Bitmap.Dispose();
                _lru.Remove(page);
            }
            _cache[page] = new CachedPage { Bitmap = bmp, RequestedWidth = requestedWidth };
            _lru.AddFirst(page);

            long total = 0;
            foreach (CachedPage c in _cache.Values)
                total += c.Bytes;
            while (total > CacheBudgetBytes && _lru.Count > 1)
            {
                int victim = _lru.Last.Value;
                if (victim == _currentPage || victim == page)
                {
                    // Never evict what is (about to be) on screen; move it to the front instead.
                    _lru.RemoveLast();
                    _lru.AddFirst(victim);
                    bool onlyVisibleLeft = true;
                    foreach (int p in _lru)
                    {
                        if (p != _currentPage && p != page)
                        {
                            onlyVisibleLeft = false;
                            break;
                        }
                    }
                    if (onlyVisibleLeft)
                        break;
                    continue;
                }
                total -= _cache[victim].Bytes;
                RemoveRaster(victim);
            }
        }

        private void RemoveRaster(int page)
        {
            CachedPage c;
            if (_cache.TryGetValue(page, out c))
            {
                if (_displaySource == c.Bitmap)
                    DisposeDisplay();
                _cache.Remove(page);
                _lru.Remove(page);
                if (c.Bitmap != null)
                    c.Bitmap.Dispose();
            }
        }

        private void ClearCache()
        {
            DisposeDisplay();
            foreach (CachedPage c in _cache.Values)
            {
                if (c.Bitmap != null)
                {
                    try { c.Bitmap.Dispose(); } catch { }
                }
            }
            _cache.Clear();
            _lru.Clear();
        }

        private void Touch(int page)
        {
            if (_lru.Remove(page))
                _lru.AddFirst(page);
        }

        private void GoToPage(int page)
        {
            if (_totalPages <= 0)
                return;
            int target = Math.Max(1, Math.Min(_totalPages, page));
            if (target == _currentPage && CurrentRaster != null)
                return;

            _currentPage = target;
            _scrollX = 0;
            _scrollY = 0;
            CachedPage cached = CurrentRaster;
            if (cached != null && cached.RequestedWidth >= ComputeRequestWidth())
            {
                CancelRendering();
                Touch(target);
                _message = null;
                Invalidate();
                return;
            }
            RequestRender(target);
        }

        private void ScheduleSharperRaster()
        {
            _rerenderTimer.Stop();
            _rerenderTimer.Start();
        }

        private void OnRerenderTimer(object sender, EventArgs e)
        {
            try
            {
                _rerenderTimer.Stop();
                if (IsDisposed || _loading || string.IsNullOrEmpty(_pdfPath) || _totalPages <= 0)
                    return;
                CachedPage c = CurrentRaster;
                if (c != null && ComputeRequestWidth() > c.RequestedWidth)
                    RequestRender(_currentPage);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("OnRerenderTimer", ex);
            }
        }

        // ------------------------------------------------------------------ layout

        private int Px(float v)
        {
            return (int)Math.Round(v * _scale);
        }

        private int ToolbarHeight
        {
            get { return Px(36); }
        }

        private void LayoutToolbar(out Rectangle prev, out Rectangle label, out Rectangle next,
                                   out Rectangle zoomOut, out Rectangle fit, out Rectangle zoomIn, out bool showZoom)
        {
            int top = Px(6);
            int h = Math.Max(1, ToolbarHeight - Px(12));
            int bw = Px(28);
            int gap = Px(4);
            int pad = Px(8);
            int clientW = ClientSize.Width;

            int fitW = Px(38);
            int zoomGroupW = bw + gap + fitW + gap + bw;
            int labelW = Px(120);
            int leftW = bw + gap + labelW + gap + bw;

            showZoom = clientW >= pad + leftW + Px(16) + zoomGroupW + pad;
            if (!showZoom)
            {
                // Narrow pane: shrink the page label, keep navigation.
                labelW = Math.Max(Px(60), clientW - 2 * pad - 2 * (bw + gap));
            }

            int x = pad;
            prev = new Rectangle(x, top, bw, h);
            x += bw + gap;
            label = new Rectangle(x, top, labelW, h);
            x += labelW + gap;
            next = new Rectangle(x, top, bw, h);

            int rx = clientW - pad - bw;
            zoomIn = new Rectangle(rx, top, bw, h);
            rx -= gap + fitW;
            fit = new Rectangle(rx, top, fitW, h);
            rx -= gap + bw;
            zoomOut = new Rectangle(rx, top, bw, h);
        }

        private ToolButton HitTest(Point pt)
        {
            Rectangle prev, label, next, zoomOut, fit, zoomIn;
            bool showZoom;
            LayoutToolbar(out prev, out label, out next, out zoomOut, out fit, out zoomIn, out showZoom);
            if (prev.Contains(pt)) return ToolButton.Prev;
            if (next.Contains(pt)) return ToolButton.Next;
            if (showZoom)
            {
                if (zoomOut.Contains(pt)) return ToolButton.ZoomOut;
                if (fit.Contains(pt)) return ToolButton.Fit;
                if (zoomIn.Contains(pt)) return ToolButton.ZoomIn;
            }
            return ToolButton.None;
        }

        private bool IsEnabled(ToolButton b)
        {
            bool hasPage = CurrentRaster != null;
            switch (b)
            {
                case ToolButton.Prev: return _totalPages > 0 && _currentPage > 1;
                case ToolButton.Next: return _totalPages > 0 && _currentPage < _totalPages;
                case ToolButton.ZoomOut: return hasPage && (_fitWidth || _zoom > MinZoom + 0.001f);
                case ToolButton.Fit: return hasPage && !_fitWidth;
                case ToolButton.ZoomIn: return hasPage && (_fitWidth || _zoom < MaxZoom - 0.001f);
                default: return false;
            }
        }

        /// <summary>Page rectangle in client coordinates for the current raster, zoom and scroll.</summary>
        private bool GetPageGeometry(out Rectangle pageRect, out int maxScrollX, out int maxScrollY)
        {
            pageRect = Rectangle.Empty;
            maxScrollX = 0;
            maxScrollY = 0;
            CachedPage c = CurrentRaster;
            if (c == null || c.Bitmap == null || c.Bitmap.Width <= 0 || c.Bitmap.Height <= 0)
                return false;

            int m = PageMargin;
            int clientW = ClientSize.Width;
            int canvasTop = ToolbarHeight;
            int canvasH = Math.Max(1, ClientSize.Height - canvasTop);
            int fitW = Math.Max(Px(40), clientW - 2 * m);
            int pageW = _fitWidth ? fitW : Math.Max(Px(40), (int)Math.Round(fitW * _zoom));
            int pageH = Math.Max(1, (int)Math.Round((double)c.Bitmap.Height * pageW / c.Bitmap.Width));

            maxScrollX = Math.Max(0, pageW + 2 * m - clientW);
            maxScrollY = Math.Max(0, pageH + 2 * m - canvasH);
            int sx = Math.Max(0, Math.Min(maxScrollX, _scrollX));
            int sy = Math.Max(0, Math.Min(maxScrollY, _scrollY));

            int x = maxScrollX > 0 ? m - sx : (clientW - pageW) / 2;
            int y = canvasTop + m - sy;
            pageRect = new Rectangle(x, y, pageW, pageH);
            return true;
        }

        private void ClampScroll()
        {
            Rectangle page;
            int maxX, maxY;
            if (!GetPageGeometry(out page, out maxX, out maxY))
            {
                _scrollX = 0;
                _scrollY = 0;
                return;
            }
            _scrollX = Math.Max(0, Math.Min(maxX, _scrollX));
            _scrollY = Math.Max(0, Math.Min(maxY, _scrollY));
        }

        // ------------------------------------------------------------------ zoom & scrolling

        private void SetZoom(bool fit, float zoom)
        {
            float oldW = _fitWidth ? 1.0f : _zoom;
            _fitWidth = fit;
            _zoom = Math.Max(MinZoom, Math.Min(MaxZoom, zoom));
            float newW = _fitWidth ? 1.0f : _zoom;
            if (oldW > 0)
            {
                // Keep the vertical position roughly in place.
                _scrollY = (int)Math.Round(_scrollY * (newW / oldW));
                _scrollX = (int)Math.Round(_scrollX * (newW / oldW));
            }
            ClampScroll();
            Invalidate();
            ScheduleSharperRaster();
        }

        private void ZoomBy(int direction)
        {
            float current = _fitWidth ? 1.0f : _zoom;
            float next = direction > 0 ? current * 1.25f : current / 1.25f;
            if (Math.Abs(next - 1.0f) < 0.06f)
            {
                SetZoom(true, 1.0f);
                return;
            }
            SetZoom(false, next);
        }

        private void ScrollBy(int dx, int dy, bool flipPagesAtEdges)
        {
            Rectangle page;
            int maxX, maxY;
            if (!GetPageGeometry(out page, out maxX, out maxY))
                return;

            if (flipPagesAtEdges && dy != 0)
            {
                if (dy > 0 && _scrollY >= maxY && _currentPage < _totalPages)
                {
                    GoToPage(_currentPage + 1);
                    return;
                }
                if (dy < 0 && _scrollY <= 0 && _currentPage > 1)
                {
                    int prevPage = _currentPage - 1;
                    GoToPage(prevPage);
                    _scrollY = int.MaxValue / 2; // show the bottom of the previous page
                    ClampScroll();
                    return;
                }
            }

            _scrollX = Math.Max(0, Math.Min(maxX, _scrollX + dx));
            _scrollY = Math.Max(0, Math.Min(maxY, _scrollY + dy));
            Invalidate();
        }

        private void Activate(ToolButton b)
        {
            if (!IsEnabled(b))
                return;
            switch (b)
            {
                case ToolButton.Prev: GoToPage(_currentPage - 1); break;
                case ToolButton.Next: GoToPage(_currentPage + 1); break;
                case ToolButton.ZoomOut: ZoomBy(-1); break;
                case ToolButton.ZoomIn: ZoomBy(+1); break;
                case ToolButton.Fit: SetZoom(true, 1.0f); break;
            }
        }

        // ------------------------------------------------------------------ input

        protected override bool IsInputKey(Keys keyData)
        {
            switch (keyData & Keys.KeyCode)
            {
                case Keys.Left:
                case Keys.Right:
                case Keys.Up:
                case Keys.Down:
                case Keys.PageUp:
                case Keys.PageDown:
                case Keys.Home:
                case Keys.End:
                    return true;
            }
            return base.IsInputKey(keyData);
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            base.OnKeyDown(e);
            int line = Px(48);
            int pageStep = Math.Max(line, ClientSize.Height - ToolbarHeight - Px(48));
            bool handled = true;
            switch (e.KeyCode)
            {
                case Keys.Left:
                    if (e.Control) GoToPage(_currentPage - 1); else ScrollBy(-line, 0, false);
                    break;
                case Keys.Right:
                    if (e.Control) GoToPage(_currentPage + 1); else ScrollBy(line, 0, false);
                    break;
                case Keys.Up: ScrollBy(0, -line, true); break;
                case Keys.Down: ScrollBy(0, line, true); break;
                case Keys.PageUp: ScrollBy(0, -pageStep, true); break;
                case Keys.PageDown:
                case Keys.Space:
                    ScrollBy(0, pageStep, true);
                    break;
                case Keys.Home: GoToPage(1); break;
                case Keys.End: GoToPage(_totalPages); break;
                case Keys.Add:
                case Keys.Oemplus:
                    ZoomBy(+1);
                    break;
                case Keys.Subtract:
                case Keys.OemMinus:
                    ZoomBy(-1);
                    break;
                case Keys.D0:
                case Keys.NumPad0:
                    SetZoom(true, 1.0f);
                    break;
                default:
                    handled = false;
                    break;
            }
            if (handled)
            {
                e.Handled = true;
                e.SuppressKeyPress = true;
            }
        }

        protected override void OnMouseDown(MouseEventArgs e)
        {
            base.OnMouseDown(e);
            try { Focus(); } catch { }
            if (e.Button != MouseButtons.Left)
                return;

            ToolButton b = HitTest(e.Location);
            if (b != ToolButton.None)
            {
                _pressed = b;
                Capture = true;
                Invalidate();
                return;
            }

            if (_retryPage > 0 && !_loading && CurrentRaster == null && e.Y > ToolbarHeight)
            {
                int page = _retryPage;
                _retryPage = 0;
                PreviewLog.Write("retry requested for page " + page.ToString(CultureInfo.InvariantCulture));
                RequestRender(page);
                return;
            }

            if (e.Y > ToolbarHeight && CurrentRaster != null)
            {
                _dragging = true;
                _dragStart = e.Location;
                _dragScrollX = _scrollX;
                _dragScrollY = _scrollY;
                Capture = true;
                Cursor = Cursors.SizeAll;
            }
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            base.OnMouseMove(e);
            if (_dragging)
            {
                _scrollX = _dragScrollX - (e.X - _dragStart.X);
                _scrollY = _dragScrollY - (e.Y - _dragStart.Y);
                ClampScroll();
                Invalidate();
                return;
            }

            ToolButton hover = HitTest(e.Location);
            if (hover != _hover)
            {
                _hover = hover;
                Invalidate(new Rectangle(0, 0, ClientSize.Width, ToolbarHeight));
            }
            bool retry = _retryPage > 0 && !_loading && CurrentRaster == null && e.Y > ToolbarHeight;
            Cursor = ((hover != ToolButton.None && IsEnabled(hover)) || retry) ? Cursors.Hand : Cursors.Default;
        }

        protected override void OnMouseUp(MouseEventArgs e)
        {
            base.OnMouseUp(e);
            if (_dragging)
            {
                _dragging = false;
                Capture = false;
                Cursor = Cursors.Default;
                return;
            }
            if (_pressed != ToolButton.None)
            {
                ToolButton pressed = _pressed;
                _pressed = ToolButton.None;
                Capture = false;
                if (HitTest(e.Location) == pressed)
                    Activate(pressed);
                Invalidate();
            }
        }

        protected override void OnMouseLeave(EventArgs e)
        {
            base.OnMouseLeave(e);
            if (_hover != ToolButton.None)
            {
                _hover = ToolButton.None;
                Invalidate(new Rectangle(0, 0, ClientSize.Width, ToolbarHeight));
            }
        }

        protected override void OnMouseWheel(MouseEventArgs e)
        {
            base.OnMouseWheel(e);
            int notches = e.Delta / 120;
            if (notches == 0)
                notches = Math.Sign(e.Delta);
            if ((ModifierKeys & Keys.Control) == Keys.Control)
            {
                if (CurrentRaster != null)
                    ZoomBy(notches > 0 ? +1 : -1);
                return;
            }
            int step = Px(64) * Math.Abs(notches);
            if ((ModifierKeys & Keys.Shift) == Keys.Shift)
                ScrollBy(notches > 0 ? -step : step, 0, false);
            else
                ScrollBy(0, notches > 0 ? -step : step, true);
        }

        protected override void OnResize(EventArgs e)
        {
            base.OnResize(e);
            ClampScroll();
            Invalidate();
            if (CurrentRaster != null)
                ScheduleSharperRaster();
        }

        protected override void OnHandleCreated(EventArgs e)
        {
            base.OnHandleCreated(e);
            RefreshDpi();
        }

        protected override void WndProc(ref Message m)
        {
            // Mouse, keyboard, size and paint handlers all run from here. An exception must never escape
            // into the preview host's message loop (WinForms would show a dialog or end prevhost.exe).
            try
            {
                if (m.Msg == NativeMethods.WM_DPICHANGED_AFTERPARENT)
                {
                    RefreshDpi();
                    ClampScroll();
                    Invalidate();
                    ScheduleSharperRaster();
                }
                base.WndProc(ref m);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("WndProc 0x" + m.Msg.ToString("X4", CultureInfo.InvariantCulture), ex);
            }
        }

        // ------------------------------------------------------------------ painting

        private static bool IsDark(Color c)
        {
            return (0.299 * c.R + 0.587 * c.G + 0.114 * c.B) < 128;
        }

        private void EnsureFonts()
        {
            if (_statusFont != null)
                return;
            string face = string.IsNullOrEmpty(_fontFace) ? "Segoe UI" : _fontFace;
            try
            {
                _statusFont = new Font(face, 13f * _scale, FontStyle.Regular, GraphicsUnit.Pixel);
                _labelFont = new Font(face, 12f * _scale, FontStyle.Bold, GraphicsUnit.Pixel);
                _buttonFont = new Font(face, 13f * _scale, FontStyle.Bold, GraphicsUnit.Pixel);
            }
            catch
            {
                DisposeFonts();
                _statusFont = new Font(FontFamily.GenericSansSerif, 13f * _scale, FontStyle.Regular, GraphicsUnit.Pixel);
                _labelFont = new Font(FontFamily.GenericSansSerif, 12f * _scale, FontStyle.Bold, GraphicsUnit.Pixel);
                _buttonFont = new Font(FontFamily.GenericSansSerif, 13f * _scale, FontStyle.Bold, GraphicsUnit.Pixel);
            }
        }

        private void DisposeFonts()
        {
            if (_statusFont != null) { _statusFont.Dispose(); _statusFont = null; }
            if (_labelFont != null) { _labelFont.Dispose(); _labelFont = null; }
            if (_buttonFont != null) { _buttonFont.Dispose(); _buttonFont = null; }
        }

        protected override void OnPaintBackground(PaintEventArgs e)
        {
            // Everything is painted in OnPaint (double-buffered) to avoid flicker.
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            try
            {
                PaintPane(e.Graphics);
            }
            catch (Exception ex)
            {
                PreviewLog.Error("OnPaint", ex);
            }
        }

        private void PaintPane(Graphics g)
        {
            EnsureFonts();
            Rectangle client = ClientRectangle;
            int toolbarH = ToolbarHeight;

            using (SolidBrush bg = new SolidBrush(_back))
                g.FillRectangle(bg, client);

            Rectangle canvas = new Rectangle(0, toolbarH, client.Width, Math.Max(1, client.Height - toolbarH));
            g.SetClip(canvas);

            Rectangle pageRect;
            int maxX, maxY;
            CachedPage raster = CurrentRaster;
            if (raster != null && GetPageGeometry(out pageRect, out maxX, out maxY))
            {
                bool dark = IsDark(_back);
                using (SolidBrush shadow = new SolidBrush(Color.FromArgb(dark ? 140 : 40, 0, 0, 0)))
                    g.FillRectangle(shadow, new Rectangle(pageRect.X + Px(2), pageRect.Y + Px(3), pageRect.Width, pageRect.Height));

                g.FillRectangle(Brushes.White, pageRect);
                g.CompositingQuality = CompositingQuality.HighSpeed;
                Bitmap display = GetDisplayBitmap(raster.Bitmap, pageRect.Size);
                if (display != null)
                {
                    // Pre-scaled once: scrolling and repaints are a plain 1:1 blit.
                    g.InterpolationMode = InterpolationMode.NearestNeighbor;
                    g.PixelOffsetMode = PixelOffsetMode.Half;
                    g.DrawImageUnscaled(display, pageRect.X, pageRect.Y);
                }
                else
                {
                    g.InterpolationMode = _dragging ? InterpolationMode.Bilinear : InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    g.DrawImage(raster.Bitmap, pageRect);
                }

                using (Pen border = new Pen(dark ? Color.FromArgb(70, 76, 86) : Color.FromArgb(200, 205, 212), 1f))
                    g.DrawRectangle(border, pageRect.X, pageRect.Y, pageRect.Width - 1, pageRect.Height - 1);

                PaintScrollIndicators(g, canvas, pageRect, maxX, maxY);
            }
            else
            {
                PaintMessage(g, canvas);
            }

            g.ResetClip();
            PaintToolbar(g, client.Width, toolbarH);
        }

        /// <summary>
        /// The page raster scaled to its on-screen size (high-quality, cached), or null to draw the
        /// raster directly (when it already has that size or the scaled copy would be too large).
        /// </summary>
        private Bitmap GetDisplayBitmap(Bitmap source, Size size)
        {
            if (source == null || size.Width <= 0 || size.Height <= 0)
                return null;
            if (source.Width == size.Width && source.Height == size.Height)
                return source;
            if ((long)size.Width * size.Height > 12L * 1024 * 1024)
                return null;
            if (_display != null && _displaySource == source && _display.Width == size.Width && _display.Height == size.Height)
                return _display;

            DisposeDisplay();
            try
            {
                Bitmap scaled = new Bitmap(size.Width, size.Height, PixelFormat.Format32bppPArgb);
                using (Graphics g = Graphics.FromImage(scaled))
                {
                    g.Clear(Color.White);
                    g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                    g.PixelOffsetMode = PixelOffsetMode.HighQuality;
                    g.CompositingQuality = CompositingQuality.HighQuality;
                    using (ImageAttributes attrs = new ImageAttributes())
                    {
                        attrs.SetWrapMode(WrapMode.TileFlipXY); // no grey fringe at the page edges
                        g.DrawImage(source, new Rectangle(0, 0, size.Width, size.Height), 0, 0, source.Width, source.Height, GraphicsUnit.Pixel, attrs);
                    }
                }
                _display = scaled;
                _displaySource = source;
                return _display;
            }
            catch (Exception ex)
            {
                PreviewLog.Error("GetDisplayBitmap", ex);
                DisposeDisplay();
                return null;
            }
        }

        private void DisposeDisplay()
        {
            if (_display != null)
            {
                try { _display.Dispose(); } catch { }
                _display = null;
            }
            _displaySource = null;
        }

        private void PaintScrollIndicators(Graphics g, Rectangle canvas, Rectangle pageRect, int maxX, int maxY)
        {
            int thickness = Math.Max(3, Px(4));
            using (SolidBrush thumb = new SolidBrush(Color.FromArgb(110, 128, 128, 128)))
            {
                if (maxY > 0)
                {
                    int trackH = canvas.Height - 2 * Px(4);
                    int total = pageRect.Height + 2 * PageMargin;
                    int thumbH = Math.Max(Px(24), (int)((long)trackH * canvas.Height / Math.Max(1, total)));
                    int thumbY = canvas.Top + Px(4) + (int)((long)(trackH - thumbH) * Math.Min(_scrollY, maxY) / maxY);
                    g.FillRectangle(thumb, canvas.Right - thickness - Px(2), thumbY, thickness, thumbH);
                }
                if (maxX > 0)
                {
                    int trackW = canvas.Width - 2 * Px(4);
                    int total = pageRect.Width + 2 * PageMargin;
                    int thumbW = Math.Max(Px(24), (int)((long)trackW * canvas.Width / Math.Max(1, total)));
                    int thumbX = canvas.Left + Px(4) + (int)((long)(trackW - thumbW) * Math.Min(_scrollX, maxX) / maxX);
                    g.FillRectangle(thumb, thumbX, canvas.Bottom - thickness - Px(2), thumbW, thickness);
                }
            }
        }

        private void PaintMessage(Graphics g, Rectangle canvas)
        {
            string msg = !string.IsNullOrEmpty(_message) ? _message : (_loading ? "Loading\u2026" : "Select a PDF document to preview.");
            int pad = Px(20);
            Rectangle textRect = new Rectangle(canvas.Left + pad, canvas.Top + pad, Math.Max(Px(40), canvas.Width - 2 * pad), Math.Max(Px(40), canvas.Height - 2 * pad));
            g.TextRenderingHint = TextRenderingHint.ClearTypeGridFit;

            using (StringFormat sf = new StringFormat())
            {
                sf.Alignment = StringAlignment.Center;
                sf.LineAlignment = StringAlignment.Center;
                sf.Trimming = StringTrimming.EllipsisWord;

                string full = msg;
                if (!string.IsNullOrEmpty(_displayName) && (_messageIsError || _loading))
                    full = _displayName + "\n\n" + msg;

                using (SolidBrush textBrush = new SolidBrush(_text))
                    g.DrawString(full, _statusFont, textBrush, textRect, sf);

                if (_messageIsError)
                {
                    SizeF size = g.MeasureString(full, _statusFont, textRect.Width, sf);
                    int lineW = Px(36);
                    int lineY = textRect.Top + (int)((textRect.Height + size.Height) / 2) + Px(10);
                    using (Pen accent = new Pen(BrandRed, Math.Max(2f, 2f * _scale)))
                        g.DrawLine(accent, textRect.Left + (textRect.Width - lineW) / 2, lineY, textRect.Left + (textRect.Width + lineW) / 2, lineY);
                }
            }
        }

        private void PaintToolbar(Graphics g, int width, int height)
        {
            Rectangle bar = new Rectangle(0, 0, width, height);
            using (SolidBrush navy = new SolidBrush(BrandNavy))
                g.FillRectangle(navy, bar);
            using (Pen accent = new Pen(BrandRed, Math.Max(2f, 2f * _scale)))
                g.DrawLine(accent, 0, height - 1, width, height - 1);

            Rectangle prev, label, next, zoomOut, fit, zoomIn;
            bool showZoom;
            LayoutToolbar(out prev, out label, out next, out zoomOut, out fit, out zoomIn, out showZoom);

            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            DrawButton(g, prev, "\u2039", ToolButton.Prev);
            DrawButton(g, next, "\u203A", ToolButton.Next);
            if (showZoom)
            {
                DrawButton(g, zoomOut, "\u2212", ToolButton.ZoomOut);
                DrawButton(g, fit, "Fit", ToolButton.Fit);
                DrawButton(g, zoomIn, "+", ToolButton.ZoomIn);
            }

            string text;
            if (_totalPages > 0)
                text = "Page " + _currentPage.ToString(CultureInfo.CurrentCulture) + " of " + _totalPages.ToString(CultureInfo.CurrentCulture);
            else
                text = "Linkco PDF Preview";
            if (_loading && CurrentRaster != null)
                text += " \u2026";

            using (StringFormat sf = new StringFormat())
            {
                sf.Alignment = StringAlignment.Center;
                sf.LineAlignment = StringAlignment.Center;
                sf.Trimming = StringTrimming.EllipsisCharacter;
                sf.FormatFlags = StringFormatFlags.NoWrap;
                g.DrawString(text, _labelFont, Brushes.White, label, sf);
            }
            g.SmoothingMode = SmoothingMode.None;
        }

        private void DrawButton(Graphics g, Rectangle r, string glyph, ToolButton id)
        {
            bool enabled = IsEnabled(id);
            bool hover = enabled && _hover == id;
            bool pressed = enabled && _pressed == id;

            Color fill = pressed ? BrandRed : (hover ? Color.FromArgb(32, 58, 78) : (enabled ? Color.FromArgb(18, 38, 54) : Color.FromArgb(10, 26, 36)));
            Color border = enabled ? (hover || pressed ? BrandRed : Color.FromArgb(70, 92, 110)) : Color.FromArgb(40, 54, 64);
            Color fg = enabled ? Color.White : Color.FromArgb(105, 118, 130);

            using (GraphicsPath path = RoundedRect(r, Px(4)))
            {
                using (SolidBrush b = new SolidBrush(fill))
                    g.FillPath(b, path);
                using (Pen p = new Pen(border, 1f))
                    g.DrawPath(p, path);
            }
            using (SolidBrush tb = new SolidBrush(fg))
            using (StringFormat sf = new StringFormat())
            {
                sf.Alignment = StringAlignment.Center;
                sf.LineAlignment = StringAlignment.Center;
                sf.FormatFlags = StringFormatFlags.NoWrap;
                g.DrawString(glyph, _buttonFont, tb, r, sf);
            }
        }

        private static GraphicsPath RoundedRect(Rectangle r, int radius)
        {
            GraphicsPath path = new GraphicsPath();
            int d = Math.Max(1, Math.Min(radius * 2, Math.Min(r.Width, r.Height)));
            path.AddArc(r.X, r.Y, d, d, 180, 90);
            path.AddArc(r.Right - d - 1, r.Y, d, d, 270, 90);
            path.AddArc(r.Right - d - 1, r.Bottom - d - 1, d, d, 0, 90);
            path.AddArc(r.X, r.Bottom - d - 1, d, d, 90, 90);
            path.CloseFigure();
            return path;
        }

        // ------------------------------------------------------------------ helpers & teardown

        private static int ParseInt(string s, int fallback)
        {
            int v;
            return int.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture, out v) ? v : fallback;
        }

        private static string LastLine(string text)
        {
            if (string.IsNullOrEmpty(text))
                return null;
            string[] lines = text.Split(new char[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
            if (lines.Length == 0)
                return null;
            string last = lines[lines.Length - 1].Trim();
            return last.Length > 300 ? last.Substring(0, 300) : last;
        }

        /// <summary>Quotes one argument for the Windows (MSVC/Rust std) command-line parser.</summary>
        internal static string QuoteArg(string arg)
        {
            if (string.IsNullOrEmpty(arg))
                return "\"\"";
            StringBuilder sb = new StringBuilder(arg.Length + 2);
            sb.Append('"');
            int backslashes = 0;
            foreach (char c in arg)
            {
                if (c == '\\')
                {
                    backslashes++;
                    continue;
                }
                if (c == '"')
                {
                    sb.Append('\\', backslashes * 2 + 1);
                    sb.Append('"');
                }
                else
                {
                    sb.Append('\\', backslashes);
                    sb.Append(c);
                }
                backslashes = 0;
            }
            sb.Append('\\', backslashes * 2);
            sb.Append('"');
            return sb.ToString();
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                Shutdown();
                try
                {
                    _rerenderTimer.Tick -= OnRerenderTimer;
                    _rerenderTimer.Dispose();
                }
                catch { }
                DisposeFonts();
            }
            base.Dispose(disposing);
        }
    }
}
