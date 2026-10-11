// Hosts a registered preview handler the way File Explorer does, and reports every step.
//
// Explorer creates the handler out of process (CLSCTX_LOCAL_SERVER: COM starts the CLSID's AppID
// DllSurrogate, prevhost.exe), initializes it with a stream of the file, gives it a site
// (IPreviewHandlerFrame), a parent window and its colours/font, then calls DoPreview. When any of
// those calls fails, or the host process dies, Explorer shows "This file can't be previewed because
// of an error in the <name>". This program makes the same calls, prints each HRESULT, watches the
// surrogate process, and measures what the handler painted into the parent window.
//
//   csc /nologo /target:exe /platform:x64 /out:PreviewHostTest.exe /r:System.Windows.Forms.dll /r:System.Drawing.dll PreviewHostTest.cs
//   PreviewHostTest.exe <clsid> <file.pdf> <local|inproc> <seconds> [screenshot.png]
//
// Exit code 0 when every call succeeded and the host survived; 1 otherwise. C# 5 (Framework csc).

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace LinkcoPreviewHostTest
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int left, top, right, bottom;
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
        public int lfHeight, lfWidth, lfEscapement, lfOrientation, lfWeight;
        public byte lfItalic, lfUnderline, lfStrikeOut, lfCharSet, lfOutPrecision, lfClipPrecision, lfQuality, lfPitchAndFamily;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string lfFaceName;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct PREVIEWHANDLERFRAMEINFO
    {
        public IntPtr haccel;
        public uint cAccelEntries;
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("8895b1c6-b41f-4c1c-a562-0d564250836f")]
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

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("196bf9a5-b346-4ef0-aa1e-5dcdb76768b1")]
    public interface IPreviewHandlerVisuals
    {
        void SetBackgroundColor(uint color);
        void SetFont(ref LOGFONT plf);
        void SetTextColor(uint color);
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("fec87aaf-35f9-447a-adb7-20234491401a")]
    public interface IPreviewHandlerFrame
    {
        void GetWindowContext(out PREVIEWHANDLERFRAMEINFO pinfo);
        [PreserveSig]
        int TranslateAccelerator(ref MSG pmsg);
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("b824b49d-22ac-4161-ac8a-9916e8fa3f7f")]
    public interface IInitializeWithStream
    {
        void Initialize(IStream pstream, uint grfMode);
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("b7d14566-0509-4cce-a71f-0a554233bd9b")]
    public interface IInitializeWithFile
    {
        void Initialize([MarshalAs(UnmanagedType.LPWStr)] string pszFilePath, uint grfMode);
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("fc4801a3-2ba9-11cf-a229-00aa003d7352")]
    public interface IObjectWithSite
    {
        void SetSite([MarshalAs(UnmanagedType.IUnknown)] object pUnkSite);
        void GetSite(ref Guid riid, [MarshalAs(UnmanagedType.IUnknown)] out object ppvSite);
    }

    /// <summary>Explorer's side of the conversation: the site the handler talks back to.</summary>
    [ComVisible(true), ClassInterface(ClassInterfaceType.None)]
    public sealed class Frame : IPreviewHandlerFrame
    {
        public void GetWindowContext(out PREVIEWHANDLERFRAMEINFO pinfo)
        {
            pinfo = new PREVIEWHANDLERFRAMEINFO();
        }

        public int TranslateAccelerator(ref MSG pmsg)
        {
            return 1; // S_FALSE
        }
    }

    public static class Program
    {
        private const uint CLSCTX_INPROC_SERVER = 0x1;
        private const uint CLSCTX_LOCAL_SERVER = 0x4;
        private const uint STGM_READ = 0x0;
        private const uint STGM_SHARE_DENY_NONE = 0x40;

        [DllImport("ole32.dll")]
        private static extern int CoCreateInstance(ref Guid rclsid, IntPtr pUnkOuter, uint dwClsContext, ref Guid riid,
            [MarshalAs(UnmanagedType.IUnknown)] out object ppv);

        [DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
        private static extern int SHCreateStreamOnFileEx(string pszFile, uint grfMode, uint dwAttributes, bool fCreate,
            IntPtr pstmTemplate, out IStream ppstm);

        private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);

        [DllImport("user32.dll")]
        private static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc proc, IntPtr lParam);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetClassName(IntPtr hwnd, StringBuilder name, int max);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);

        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr hwnd);

        [DllImport("user32.dll")]
        private static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);

        [DllImport("user32.dll")]
        private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

        private static bool _ok = true;

        private static void Say(string line)
        {
            Console.WriteLine(line);
        }

        private static string Hr(Exception ex)
        {
            int hr = Marshal.GetHRForException(ex);
            return "FAILED 0x" + hr.ToString("X8") + " " + ex.GetType().Name + ": " + ex.Message.Trim();
        }

        private static bool Step(string name, Action action)
        {
            try
            {
                action();
                Say("  " + name + ": S_OK");
                return true;
            }
            catch (Exception ex)
            {
                _ok = false;
                Say("  " + name + ": " + Hr(ex));
                return false;
            }
        }

        private static List<int> HostPids()
        {
            List<int> pids = new List<int>();
            foreach (string name in new string[] { "prevhost", "dllhost" })
            {
                foreach (Process p in Process.GetProcessesByName(name))
                {
                    pids.Add(p.Id);
                    p.Dispose();
                }
            }
            return pids;
        }

        private static string DescribeChildren(IntPtr parent)
        {
            StringBuilder sb = new StringBuilder();
            int self = Process.GetCurrentProcess().Id;
            EnumChildWindows(parent, delegate(IntPtr h, IntPtr l)
            {
                StringBuilder cls = new StringBuilder(256);
                GetClassName(h, cls, cls.Capacity);
                uint pid;
                GetWindowThreadProcessId(h, out pid);
                string owner = pid == self ? "this process" : ProcessName((int)pid);
                sb.Append("    child window " + cls + " (" + owner + ", pid " + pid + (IsWindowVisible(h) ? ", visible" : ", hidden") + ")\n");
                return true;
            }, IntPtr.Zero);
            return sb.Length == 0 ? "    (no child windows: the handler never attached a window)\n" : sb.ToString();
        }

        private static string ProcessName(int pid)
        {
            try
            {
                using (Process p = Process.GetProcessById(pid))
                    return p.ProcessName + ".exe";
            }
            catch
            {
                return "exited process";
            }
        }

        /// <summary>Share of pixels in the parent's client area that differ from the plain background (screen copy).</summary>
        private static string Measure(Form form, string screenshot)
        {
            try
            {
                Rectangle client = form.RectangleToScreen(form.ClientRectangle);
                using (Bitmap bmp = new Bitmap(client.Width, client.Height, PixelFormat.Format32bppArgb))
                {
                    using (Graphics g = Graphics.FromImage(bmp))
                        g.CopyFromScreen(client.Location, Point.Empty, client.Size);
                    return Stats(bmp, screenshot);
                }
            }
            catch (Exception ex)
            {
                return "could not capture the window: " + ex.Message;
            }
        }

        /// <summary>
        /// What the handler's own window paints, asked of the window itself (PrintWindow): unlike a screen copy
        /// this also works on a desktop nobody is looking at, such as a CI runner's.
        /// </summary>
        private static string MeasureHandlerWindow(IntPtr parent, string screenshot)
        {
            IntPtr child = IntPtr.Zero;
            EnumChildWindows(parent, delegate(IntPtr h, IntPtr l)
            {
                child = h;
                return false;
            }, IntPtr.Zero);
            if (child == IntPtr.Zero)
                return "(no handler window)";
            try
            {
                RECT r;
                if (!GetWindowRect(child, out r) || r.right - r.left <= 0 || r.bottom - r.top <= 0)
                    return "(handler window has no size)";
                using (Bitmap bmp = new Bitmap(r.right - r.left, r.bottom - r.top, PixelFormat.Format32bppArgb))
                {
                    bool printed;
                    using (Graphics g = Graphics.FromImage(bmp))
                    {
                        IntPtr hdc = g.GetHdc();
                        try { printed = PrintWindow(child, hdc, 2 /* PW_RENDERFULLCONTENT */); }
                        finally { g.ReleaseHdc(hdc); }
                    }
                    return (printed ? "" : "PrintWindow failed; ") + bmp.Width + "x" + bmp.Height + ", " + Stats(bmp, screenshot);
                }
            }
            catch (Exception ex)
            {
                return "could not capture the handler window: " + ex.Message;
            }
        }

        private static string Stats(Bitmap bmp, string screenshot)
        {
            if (!string.IsNullOrEmpty(screenshot))
                bmp.Save(screenshot, ImageFormat.Png);
            Color bg = bmp.GetPixel(2, 2);
            int differ = 0, dark = 0, light = 0, total = 0;
            Dictionary<int, int> colours = new Dictionary<int, int>();
            for (int y = 0; y < bmp.Height; y += 4)
            {
                for (int x = 0; x < bmp.Width; x += 4)
                {
                    Color c = bmp.GetPixel(x, y);
                    total++;
                    if (Math.Abs(c.R - bg.R) + Math.Abs(c.G - bg.G) + Math.Abs(c.B - bg.B) > 24)
                        differ++;
                    if (c.R + c.G + c.B < 200)
                        dark++;
                    if (c.R + c.G + c.B > 690)
                        light++;
                    int key = c.ToArgb() & 0xF0F0F0;
                    int n;
                    colours.TryGetValue(key, out n);
                    colours[key] = n + 1;
                }
            }
            double t = Math.Max(1, total);
            return "background " + ColorTranslator.ToHtml(bg) + ", " + (100.0 * differ / t).ToString("F1") +
                "% of pixels differ from it, " + (100.0 * dark / t).ToString("F1") + "% dark, " +
                (100.0 * light / t).ToString("F1") + "% white (a rendered page), " + colours.Count + " colour buckets";
        }

        private static void Pump(int milliseconds)
        {
            Stopwatch sw = Stopwatch.StartNew();
            while (sw.ElapsedMilliseconds < milliseconds)
            {
                Application.DoEvents();
                Thread.Sleep(25);
            }
        }

        [STAThread]
        public static int Main(string[] args)
        {
            if (args.Length < 4)
            {
                Say("usage: PreviewHostTest <clsid> <file.pdf> <local|inproc> <seconds> [screenshot.png]");
                return 2;
            }
            Guid clsid = new Guid(args[0]);
            string file = args[1];
            bool local = args[2] != "inproc";
            int seconds = int.Parse(args[3]);
            string screenshot = args.Length > 4 ? args[4] : null;

            Say("Hosting " + clsid.ToString("B") + " " + (local ? "out of process (CLSCTX_LOCAL_SERVER, as Explorer)" : "in process (CLSCTX_INPROC_SERVER)") +
                " for " + file);

            Form form = new Form();
            form.Text = "Preview host test";
            form.StartPosition = FormStartPosition.Manual;
            form.Location = new Point(40, 40);
            form.ClientSize = new Size(640, 820);
            form.BackColor = Color.FromArgb(32, 32, 32);
            form.TopMost = true;
            form.Show();
            Pump(300);

            List<int> hostsBefore = HostPids();
            object instance = null;
            Guid iidUnknown = new Guid("00000000-0000-0000-C000-000000000046");
            Stopwatch clock = Stopwatch.StartNew();
            int hr = CoCreateInstance(ref clsid, IntPtr.Zero, local ? CLSCTX_LOCAL_SERVER : CLSCTX_INPROC_SERVER, ref iidUnknown, out instance);
            Say("  CoCreateInstance: 0x" + hr.ToString("X8") + (hr == 0 ? " S_OK" : " FAILED (" + new System.ComponentModel.Win32Exception(hr).Message + ")") +
                " in " + clock.ElapsedMilliseconds + " ms");
            if (hr != 0 || instance == null)
            {
                // CoCreateInstance only returns an HRESULT: ask the CLR in this process for the full
                // exception (which file it could not find or load, and the binding log).
                try
                {
                    Type t = Type.GetTypeFromCLSID(clsid, null, true);
                    object o = Activator.CreateInstance(t);
                    Say("  in-process Activator.CreateInstance: OK (" + o.GetType().AssemblyQualifiedName + ")");
                }
                catch (Exception ex)
                {
                    Exception e = ex;
                    while (e is System.Reflection.TargetInvocationException && e.InnerException != null)
                        e = e.InnerException;
                    Say("  in-process Activator.CreateInstance: " + e.GetType().FullName + " 0x" + Marshal.GetHRForException(e).ToString("X8"));
                    foreach (string l in e.ToString().Split('\n'))
                        Say("    " + l.TrimEnd());
                    System.IO.FileNotFoundException fnf = e as System.IO.FileNotFoundException;
                    if (fnf != null)
                        Say("    FileName: " + fnf.FileName + "\n    FusionLog: " + fnf.FusionLog);
                    System.IO.FileLoadException fle = e as System.IO.FileLoadException;
                    if (fle != null)
                        Say("    FileName: " + fle.FileName + "\n    FusionLog: " + fle.FusionLog);
                }
                return 1;
            }

            List<int> newHosts = HostPids();
            newHosts.RemoveAll(delegate(int p) { return hostsBefore.Contains(p); });
            foreach (int pid in newHosts)
                Say("  surrogate started: " + ProcessName(pid) + " pid " + pid);
            if (local && newHosts.Count == 0)
                Say("  surrogate: no new prevhost/dllhost process (an existing one was reused)");

            IPreviewHandler handler = instance as IPreviewHandler;
            if (handler == null)
            {
                if (!Marshal.IsComObject(instance))
                {
                    // In process, the CLR hands a .NET host the handler's .NET object itself rather than a COM
                    // wrapper, so this host's own copy of the interface does not match. Explorer never does this
                    // (it uses prevhost.exe, the out-of-process mode), so creating it is the whole test here.
                    Say("  created as a .NET object (" + instance.GetType().FullName + "); a .NET host cannot call it");
                    Say("  through its own interface copy in process - the out-of-process mode is what Explorer uses");
                    Say("RESULT: created in process");
                    return 0;
                }
                Say("  QueryInterface(IPreviewHandler): FAILED");
                return 1;
            }

            IInitializeWithStream withStream = instance as IInitializeWithStream;
            IInitializeWithFile withFile = instance as IInitializeWithFile;
            Say("  supports IInitializeWithStream: " + (withStream != null) + ", IInitializeWithFile: " + (withFile != null));
            if (withStream != null)
            {
                IStream stream;
                int shr = SHCreateStreamOnFileEx(file, STGM_READ | STGM_SHARE_DENY_NONE, 0, false, IntPtr.Zero, out stream);
                if (shr != 0)
                {
                    Say("  SHCreateStreamOnFileEx: 0x" + shr.ToString("X8"));
                    return 1;
                }
                Step("IInitializeWithStream.Initialize", delegate { withStream.Initialize(stream, STGM_READ); });
                Marshal.ReleaseComObject(stream);
            }
            else if (withFile != null)
            {
                Step("IInitializeWithFile.Initialize", delegate { withFile.Initialize(file, STGM_READ); });
            }

            IObjectWithSite site = instance as IObjectWithSite;
            if (site != null)
                Step("IObjectWithSite.SetSite", delegate { site.SetSite(new Frame()); });

            RECT rect = new RECT();
            rect.right = form.ClientSize.Width;
            rect.bottom = form.ClientSize.Height;
            Step("IPreviewHandler.SetWindow", delegate { handler.SetWindow(form.Handle, ref rect); });

            IPreviewHandlerVisuals visuals = instance as IPreviewHandlerVisuals;
            if (visuals != null)
            {
                Step("IPreviewHandlerVisuals.SetBackgroundColor", delegate { visuals.SetBackgroundColor(0x00202020); });
                Step("IPreviewHandlerVisuals.SetTextColor", delegate { visuals.SetTextColor(0x00FFFFFF); });
                LOGFONT font = new LOGFONT();
                font.lfHeight = -12;
                font.lfWeight = 400;
                font.lfFaceName = "Segoe UI";
                Step("IPreviewHandlerVisuals.SetFont", delegate { visuals.SetFont(ref font); });
            }

            clock.Restart();
            Step("IPreviewHandler.DoPreview", delegate { handler.DoPreview(); });
            Say("  DoPreview returned after " + clock.ElapsedMilliseconds + " ms");
            Step("IPreviewHandler.SetRect", delegate { handler.SetRect(ref rect); });

            Pump(seconds * 1000);

            foreach (int pid in newHosts)
            {
                bool alive;
                try
                {
                    using (Process p = Process.GetProcessById(pid))
                        alive = !p.HasExited;
                }
                catch
                {
                    alive = false;
                }
                if (!alive)
                    _ok = false;
                Say("  surrogate pid " + pid + " after " + seconds + " s: " + (alive ? "running" : "EXITED (crashed: Explorer would show the error)"));
            }
            Say("  windows inside the preview area:");
            Console.Write(DescribeChildren(form.Handle));
            Say("  painted (screen copy): " + Measure(form, screenshot));
            Say("  painted (handler window, PrintWindow): " + MeasureHandlerWindow(form.Handle,
                string.IsNullOrEmpty(screenshot) ? null : System.IO.Path.ChangeExtension(screenshot, ".window.png")));

            Step("IPreviewHandler.Unload", delegate { handler.Unload(); });
            try { Marshal.FinalReleaseComObject(instance); } catch { }
            form.Close();
            Say(_ok ? "RESULT: every call succeeded" : "RESULT: FAILED (Explorer shows \"an error in the preview handler\")");
            return _ok ? 0 : 1;
        }
    }
}
