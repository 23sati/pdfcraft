# Linkco PDF Editor v1.0.0 — Release Notes

**Product:** Linkco PDF Editor  
**Version:** `1.0.0`  
**Publisher:** Al Rawabet Commercial Services & Contracting Company W.L.L. (Linkco)  
**Developer:** Muhammad Bilal (Linkco)  
**Location:** Building 159, Street 220, Zone 24, P.O. Box 32282, Doha – State of Qatar (`www.linkco.com.qa`)

---

## What's new in v1.0.0

v1.0.0 is the first final release of Linkco PDF Editor. It contains everything from v0.6.0 below, plus the following changes.

### 1. Network printers on the Print screen
- **Network printers are recognised however they are connected:**
  - shared printers (`\\server\printer`);
  - Standard TCP/IP ports, whether named `IP_10.0.0.5` or (as current Windows names them) after the address or host, e.g. `10.0.0.5`, `10.0.0.5_1`, `printer.corp.local:9100`;
  - WSD, IPP/`http(s)://` and LPR ports;
  - any printer Windows itself reports as a network printer (`Win32_Printer.Network`).
  
  Before, only `IP_…` and `WSD…` ports and `\\server` names were recognised.
- **Printer list:** network printers are marked "· Network". A printer that can't print now shows its status in the interface language (Paused, Offline, Error).
- **Selected printer:** a line under the list shows "Network printer · Port: … · Driver: …", with the full text on hover.
- **Printer status warning:** this warning is now translated.
- **No printers found:** the dialog says so and suggests what to do: turn the printer on, connect it, then *Check again*, or save as PDF.

### 2. File Explorer preview for PDFs on network shares, mapped drives and OneDrive
- **Where previews already work:** the preview pane and thumbnails work for PDFs on network shares reached by server name (`\\fileserver\share`), on mapped drives that point at them, in OneDrive/cloud folders (Windows downloads online-only files for the preview) and on local disks.
- **Where Windows blocks them:** since the October 2025 Windows update (KB5066835), File Explorer blocks the preview pane for files in the *Internet* or *Restricted sites* security zone. Explorer does this before it loads any preview handler. Two kinds of file are affected:
  - files on shares reached by a dotted name (`\\files.corp.example.com\share`) or an IP address (`\\10.0.0.5\share`);
  - downloaded files that carry the Mark of the Web.
  
  This is a Windows security decision. Linkco PDF Editor does not switch it off for every file. Instead it helps you check and fix it per file or per server (PowerShell, no administrator rights):
  ```powershell
  $dll = "$env:ProgramFiles\Linkco\Linkco PDF Editor\LinkcoPdfPreviewHandler.dll"   # per-user install: $env:LOCALAPPDATA\Programs\Linkco\Linkco PDF Editor\…
  $t = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes($dll)).GetType('LinkcoPdfPreview.LinkcoPdfPreviewHandler')
  $t::DiagnoseFile('\\files.corp.example.com\share\drawing.pdf')   # location, security zone, Mark of the Web, and the fix
  $t::AllowNetworkPreview('files.corp.example.com')                 # list a server you trust as a Local intranet site (current user)
  $t::DisallowNetworkPreview('files.corp.example.com')              # undo
  Unblock-File -LiteralPath 'C:\Users\me\Downloads\drawing.pdf'     # a downloaded file you trust
  ```
- **`AllowNetworkPreview`** writes the same per-user entry as *Internet Options ▸ Security ▸ Local intranet ▸ Sites ▸ Advanced*, under `HKCU\…\Internet Settings\ZoneMap`. It accepts a server name, a `\\server\share` path or a mapped-drive path. Where the organisation sets the zones by policy, it changes nothing and tells you to ask IT.
- **Thumbnail temp files:** temporary thumbnail images left behind when Windows stops a slow thumbnail are now cleaned up like the preview's own.

### 3. File Explorer preview no longer keeps loading
- **Engine start:** the preview pane and thumbnails start the page engine (`pdfcraft-cli.exe`) directly, hidden, at below-normal priority. They no longer use .NET's process class. As a result:
  - Windows no longer shows the spinning "app starting" cursor each time you select a PDF;
  - waiting for the engine can't hang. Its output now goes to small temp files instead of pipes. Before, a pipe kept open by another process could leave the pane on "Loading…" forever;
  - the engine runs in a job object that ends it if the Windows preview host closes, so no orphaned `pdfcraft-cli.exe` processes are left behind.
- **A page can no longer stay on "Loading…":**
  - after 8 seconds the pane says the page is still loading;
  - a watchdog ends any render that hasn't finished 10 seconds after the engine's 30-second time limit;
  - a page that timed out or failed shows *Click to try again*.
  
  Finished pages also reach the pane even if a window message is lost.
- **Diagnostics:** `preview.log` (in `%USERPROFILE%\AppData\LocalLow\LinkcoPdfPreview`) now records each step with timings: stream copy, pane size, each render's result. `Diagnose()` (and `test-windows-integration.ps1`) now also:
  - starts the engine as the preview does, and reports whether it ran and how long it took;
  - says whether the preview host runs at low or normal integrity;
  - shows the last 15 log lines.

### 4. About and version
- **Help ▸ About** now credits the developer: "Developer: Muhammad Bilal (Linkco)", in all interface languages.
- **Version 1.0.0** is used everywhere: the app, CLI, preview handler (assembly `1.0.0.0`, matching the NSIS and MSI registrations), Setup EXE, MSI and portable ZIP.

### 5. Downloads
- `dist/release/LinkcoPDFEditorSetup.exe`
- `dist/release/LinkcoPDFEditorSetup-1.0.0-windows-x64.msi`
- `dist/release/LinkcoPDFEditor-1.0.0-windows-x64-portable.zip`

---

## What's new in v0.6.0

v0.6.0 brings in every change from the upstream PdfCraft 0.6.0 release (75 changes) while keeping the Linkco PDF Editor name, branding, Windows integration and installers.

### 1. Windows printing that keeps the window responsive
- The Print dialog opens straight away: printers (with their Ready / Paused / Offline / Error status) are listed in the background, and "Looking for printers…" shows until the list arrives. A printer you pick while it is still looking stays picked. The refresh button ("Check again") also runs in the background.
- **Print** closes the dialog at once. The sheets are rendered at the chosen 150 / 300 / 600 DPI and sent to the spooler on a worker thread, so a long document no longer freezes the window. "Sending to *printer*…" appears first, then the spooler's answer (or the error, for example an offline printer).
- Unchanged from v0.5.0: each sheet prints at 100% on the driver's closest paper size, with zero margins, landscape when the sheet is wide, hard-margin compensation and no console window.
- Upstream's alternative Windows print backend (`windows_print.ps1`) was not adopted, because Linkco's backend already covers the same behaviour plus printer status and the 3-tier printer detection.
- A scaled annotation appearance no longer prints larger than its rectangle.
- **Print dialog sized like a normal window:** the dialog now grows with the app window (to within 24 px of its edges, up to 1600 × 1200) instead of a fixed 820 px box with a 320 × 380 preview. The preview takes all the room beside the settings, which scroll when the window is short. In a 1400 × 900 window the preview pane is about 850 × 670 instead of 320 × 380. It is never smaller than before.
- **Sharp print preview:**
  - Pages are rendered at exactly the size of the larger preview.
  - Until that render arrives, the preview no longer shrinks a much larger on-screen page image into the pane. Shrinking it skipped pixels, because the renderer makes no mipmaps, so thin lines and small text broke up. The thumbnail is shown instead.
  - The paper and pages are placed on whole screen pixels, so text isn't blurred across two pixels.

### 2. Windows PDF file icon and licence files
- PDF files associated with Linkco PDF Editor now show a dedicated **PDF document icon** (a page with a PDF badge, in Linkco navy and crimson) instead of the app icon. It is embedded in `LinkcoPDFEditor.exe` as icon resource 2 and registered by the Setup EXE, the NSIS installer and the MSI (`DefaultIcon …,-2`).
- `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE` and `LICENSE-document.txt` (the icon's ISC notice) are now installed beside the executables by every installer and included in the portable ZIP.

### 3. Editing, pages and forms
- **Combine Files:** a thumbnail grid of every page from every file, where pages can be rotated, removed and reordered before combining.
- **Column select** (Alt/Option-drag, or *Edit ▸ Column select*), and selecting and moving several kinds of page objects together.
- Rectangle, oval and polygon comments get a fill colour; exported images keep their transparency (JPEG cut-outs included).
- Fill & Sign improvements; white or pale text is edited on a dark box instead of vanishing on white; **Reduce File Size** shrinks office scans, drops images nothing draws, and never saves a bigger copy.
- Docking: Tools and Inspector panels can be grouped as tabs, split beside the PDF or floated. *View ▸ Reset panel layout* restores the default.

### 4. Interface
- **Preferences ▸ Interface size** (Auto follows the desktop's text scaling).
- Keyboard shortcut key names are shown in the interface language.
- Precision touchpads scroll 1:1 with the fingers.
- **Polish** interface (17 languages in total), including the Linkco Home page strings.
- Portable mode keeps its own single-instance window, separate from an installed copy.

### 5. Fixes and hardening
- Passwords with non-ASCII characters open encrypted files; text files saved as UTF-16 create correct PDFs.
- Redaction pattern fixes; search handles the Turkish dotted/dotless i and runs of whitespace.
- OCR recognises €, and its model path is checked; about 30 smaller fixes and several hardening changes for hostile files.
- Agent and automation control: `ui.resize`, `ui.focus`, `ui.quit` (shows the save prompt when there are unsaved changes), `inspect focused=true` and `ui.dock`. These are only available when the app is started with `--control`.

### 6. Downloads
- `dist/release/LinkcoPDFEditorSetup.exe`
- `dist/release/LinkcoPDFEditorSetup-0.6.0-windows-x64.msi`
- `dist/release/LinkcoPDFEditor-0.6.0-windows-x64-portable.zip`

---

## Highlights in v0.5.0

### 1. Official Linkco Brand Identity & Enterprise Home Dashboard
- Rebranded desktop application, CLI, WebAssembly target, and installers to **Linkco PDF Editor** by **Al Rawabet Commercial Services & Contracting Company W.L.L. (Linkco)**.
- Designed multi-resolution application icons (`.svg`, `.ico`, `.icns`, and 16 px–1024 px `.png`) featuring the Linkco 4-node connected network emblem on deep navy (`#01131C`) and crimson red (`#F22424`).
- Updated the **Home** welcome view (`crates/ui-egui/src/home.rs`) with the Linkco corporate header banner, 8 verified **Recommended Tools** (`Organize pages`, `Edit a PDF`, `Combine files`, `Compress a PDF`, `Export a PDF`, `Scan & OCR`, `Fill & Sign`, `Protect a PDF`), an `Open file` card, `View all tools →` navigation to all 19 tool groups, pinned folders, recent files list (`File ▸ Open Recent`), session tab restoration (`Reopen last session`), and command palette search (`Ctrl+K` / `⌘K`).
- Removed all legacy branding, community chat links, and third-party promotional links.

### 2. Windows File Explorer PDF Preview Handler (`IPreviewHandler`)
- Added `LinkcoPdfPreviewHandler.dll` (`packaging/windows/PreviewHandler.cs`, CLSID `{D4E7B6A2-4C91-4E3A-9B12-7A8F5C3E1D20}`), an out-of-process Windows Shell `IPreviewHandler` COM server hosted by `prevhost.exe` (`{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}`).
- Added `pdfcraft-cli preview` (`apps/pdfcraft-cli/src/main.rs`) for fast single-page preview rendering with structured status reporting (`OK`, `PASSWORD_REQUIRED`, `EMPTY_PDF`, `INVALID_PDF`, `RENDER_ERROR`, `IO_ERROR`).
- Supports multi-page navigation (`Prev` / `Next`, `PgUp` / `PgDn`, `Home` / `End`), zoom (`−` / `+` / `Fit`, `Ctrl`+Wheel, click-drag pan), Windows Explorer Light/Dark theme adaptation, and non-destructive registration that backs up and restores any previously registered PDF preview handler on uninstall.
- **Production hardening:**
  - Pages are rendered at the Preview Pane's actual width × zoom (`pdfcraft-cli preview --width PX --max-px 4096`), DPI-aware, re-rendered when the pane grows, and capped so poster-sized pages can't exhaust memory in `prevhost.exe`.
  - Registration is per user (`HKCU`, no administrator rights) and covers `.pdf`, `SystemFileAssociations\.pdf`, Linkco's own ProgIDs and the user's current default PDF app (`UserChoice` / `UserChoiceLatest`), but never browser ProgIDs that also open web pages (`MSEdgeHTM`, `ChromeHTML`, …). Every value it replaces is backed up and restored on uninstall.
  - Linkco PDF Editor checks the registration in the background at every start and registers again only when the DLL was updated or moved or the default PDF app changed. Portable mode never writes to the registry. Set `LINKCO_NO_PREVIEW_REGISTRATION=1` to opt out.
  - **Non-ASCII install paths:** the start-up check that decides whether to register again compares the DLL path in an ASCII-safe form (`PreviewHandlerDllKey`), because `reg.exe` can't print most non-ASCII text. Before, a path containing e.g. an Arabic Windows user name (`C:\Users\محمد\AppData\Local\Programs\…`, the per-user install location) never matched, so the app re-registered — a hidden PowerShell plus an Explorer association refresh — on every start.
  - **Uninstall cleans up for every user.** The app registers per user, for each user who starts it, so the uninstallers remove that registration — restoring each user's previous preview and thumbnail handlers — from **every user profile on the PC**, not only the uninstalling user's, before deleting the DLL. Otherwise other accounts would be left with Explorer pointing at a deleted DLL (blank previews, even where another PDF app had a working one before). Signed-in users' registries are cleaned in place; the others are mounted from their `NTUSER.DAT`/`UsrClass.dat` for a moment and unmounted again. The MSI does this in a deferred custom action running as LocalSystem, the NSIS uninstaller and the machine-wide Setup EXE do it elevated; the Setup EXE also retries file deletes while Explorer releases the DLL. (The NSIS uninstaller's cleanup command was also shortened to fit NSIS's 1024-character string limit — the previous one was cut off and so never ran.)
  - `ThreadingModel=Apartment` (the handler hosts WinForms), `DisableLowILProcessIsolation=1` (the handler starts `pdfcraft-cli.exe` and reads the PDF by path), and Mark-of-the-Web is removed from the DLL and CLI, so downloaded builds load.
  - The handler logs to `%USERPROFILE%\AppData\LocalLow\LinkcoPdfPreview\preview.log` and cleans up its temporary PNGs.
- **PDF thumbnails in File Explorer:** the same DLL adds an `IThumbnailProvider` (CLSID `{3D8CDE4B-E969-481F-BEB0-5E3B98287416}`), so PDFs show their first page in Medium/Large/Extra large icon, Tiles and Content views — Windows has no built-in PDF thumbnails. It is registered on `SystemFileAssociations\.pdf` and Linkco's own ProgIDs only, which Explorer consults after the default app's ProgID and `.pdf`, so a thumbnail provider from Acrobat or another PDF app keeps priority. Windows runs it in its isolated thumbnail process; rendering happens in a short-lived `pdfcraft-cli.exe` (8 s timeout), and password-protected or damaged PDFs simply keep the normal icon. To turn thumbnails off, use Explorer's *Folder Options ▸ View ▸ Always show icons, never thumbnails*.
- **Troubleshooting:** run this in PowerShell to see which preview handler and thumbnail provider Explorer resolves for `.pdf`, the user's default PDF app, and both registrations:
  ```powershell
  $dll = "$env:ProgramFiles\Linkco\Linkco PDF Editor\LinkcoPdfPreviewHandler.dll"   # or dist\release\… for a dev build
  $t = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes($dll)).GetType('LinkcoPdfPreview.LinkcoPdfPreviewHandler')
  $t::Diagnose()
  $t::RegisterPreviewHandler($dll)    # re-register for the current user if it reports [NOT Linkco]
  ```

### 3. Windows Printer Detection, Native Driver Properties & High-Resolution Printing (150 / 300 / 600 DPI)
- Implemented 3-tier Windows printer enumeration in `crates/print/src/spool.rs`:
  1. WMI/CIM `Win32_Printer` (`Get-CimInstance` / `Get-WmiObject`)
  2. `.NET` `System.Drawing.Printing.PrinterSettings::InstalledPrinters`
  3. Windows Registry `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Devices` & `Windows /v Device`
- Detects local, USB, TCP/IP, WSD, and UNC network printers (plus driverless `lpstat -e` queues on Unix/CUPS), identifies the system default printer, and reports printer availability (`Ready`, `Printing`, `Paused`, `Offline`, `Error`) with an in-dialog refresh button and offline status warning.
- Added native printer driver `Properties…` (`rundll32 printui.dll,PrintUIEntry /e /n <printer>` on Windows; CUPS `lpoptions -l` PPD option editor on Unix).
- Upgraded Print dialog preview sharpness (per-page raster scaling; reworked in v0.6.0, see "Print dialog sized like a normal window") and added high-DPI sheet rendering (`Session::render_print_sheets` at 150 / 300 / 600 DPI, default 300 DPI) for crisp Windows print output via `.NET` `PrintDocument` with `HardMarginX/Y` compensation.

### 4. Content Editing, Multilingual Typography, Digital Signatures & Forms (`v0.4.0`–`v0.5.0`)
- **Direct Content Editing & Scripts (`crates/edit`, `crates/fonts`):** Multi-stream paragraph editing, grouped Form XObject figure selection and editing, Arabic shaping (`harfrust`) and bidirectional (`unicode-bidi`) Type 3 text rendering, Cyrillic/Greek/WinAnsi `0x80–0x9F` encoding, and Japanese CID/katakana font fallback.
- **Digital Signatures (`crates/sign`):** Native Windows Certificate Store (`CNG` via `rustls-cng`) signing alongside macOS Keychain and `.p12`/`.pfx` files, BER PKCS#12/PKCS#7 parsing, expanded RSA/PSS/ECDSA/Ed25519 algorithms, RFC 3161 `/DocTimeStamp` signature verification, and strict X.509 chain validation.
- **Forms, XFA & Fill & Sign (`crates/forms`, `crates/xfa`, `crates/ui-egui`):** Image signatures with automatic white-background removal and live corner resize handles, optional flatten Fill & Sign on save, localized date formats, bulk multi-field property editing, rotated `/MK /R` widget appearances, `/Opt` export values, and a full XFA FormCalc interpreter.
- **Rendering, Memory & GPU (`crates/cos`, `crates/render`, `apps/pdfcraft`):** Zero-copy file-backed PDF stream slices, zero-copy pixmap transfers, 1:1 texel page rasters with zoom tile retention, automatic OpenGL (`glow`) fallback on `wgpu` initialization failure, Windows primary-display GPU adapter selection, and Windows 11 multi-monitor DPI scaling fixes.
- **Localization (`crates/ui-egui/src/i18n`):** 16 interface languages — English, Arabic (`ar`, right-to-left), Japanese (`ja`), Brazilian Portuguese (`pt-br`), Spanish (`es`), Telugu (`te`), Czech (`cs`), Simplified Chinese (`zh-hans`), Traditional Chinese (`zh-hant`), French (`fr`), German (`de`), Italian (`it`), Russian (`ru`), Ukrainian (`uk`), Bulgarian (`bg`), and Hungarian (`hu`).

### 5. Windows Installer, Portable Mode & Build Automation
- Added `build.py` and `installer.py` for automated Windows release builds and packaging:
  - `dist/release/LinkcoPDFEditorSetup.exe` (supports both Administrator machine-wide installation to `C:\Program Files\Linkco\Linkco PDF Editor` and standard per-user installation to `%LOCALAPPDATA%\Programs\Linkco\Linkco PDF Editor`, Start Menu and optional Desktop shortcuts, Windows Installed Apps registration, and clean uninstallation preserving user settings by default).
  - `dist/release/LinkcoPDFEditorSetup-0.5.0-windows-x64.msi` (WiX Toolset v5 MSI package with per-machine scope guard and bundled OCR models).
  - `dist/release/LinkcoPDFEditor-0.5.0-windows-x64-portable.zip` (portable archive with `portable.txt` / `LinkcoPDFEditor.portable` marker support storing user data in `PdfCraftData\` beside the executable).

### 6. Arabic Interface, One Window for Every PDF, Redaction Lists & Signing Improvements
Brings in the latest upstream engine work (42 changes), adapted to Linkco PDF Editor.
- **Arabic interface (العربية):** A complete Arabic catalog (Edit ▸ Preferences… ▸ Language, Ctrl+, — or automatic on an Arabic Windows display language) with right-to-left menus and labels, Arabic plural forms and Arabic month and weekday names. Arabic glyphs come from Noto Sans Arabic (embedded by builds made with craft-fonts — `build.py` and `installer.py` now fetch it automatically) or from Windows' Segoe UI. Arabic file names and document titles are drawn in the correct order in tabs, recent files and Properties.
- **One window for every PDF (Windows):** Double-clicking PDFs in File Explorer, opening Outlook attachments or using Open With while Linkco PDF Editor is running now opens them as tabs in the existing window, instead of starting a new window each time. One instance runs per Windows sign-in (so Remote Desktop users stay separate); `LinkcoPDFEditor.exe --new-window file.pdf` still forces a separate window. The hand-off socket lives in `%LOCALAPPDATA%\Linkco PDF Editor\` beside the crash-recovery folder; portable copies always open their own window.
- **Sharper on mixed-DPI monitors:** The executable now declares Per-Monitor V2 DPI awareness in its Windows manifest, so moving the window between a laptop screen and an external monitor keeps text crisp.
- **Redaction:** Mark a whole list of words or phrases for redaction at once (one per line, or import a text file up to 1 MB), and stamp U.S. FOIA / Privacy Act exemption codes as redaction overlay text.
- **Digital signatures:** Sign and timestamp password-encrypted documents; smart cards and USB tokens in the Windows certificate store ask for their PIN; certificates whose key usage / extended key usage doesn't allow document signing, or that carry critical extensions the editor doesn't understand, are refused with a clear message; long digital ID names stay inside the picker.
- **Organize & create:** Build bookmarks from a tagged document's headings; Extract pages gets a File name field; images made into a PDF keep their ICC colour profile; print-sized TIFF images (up to 2 GiB of decoded data) can be converted.
- **Rendering accuracy:** Pages with `/UserUnit` (large-format engineering drawings) are shown at their real size in points, as in Acrobat; text rotated at any angle is selected and searched along its own baseline; PDF/A conversion no longer adds an sRGB output intent to CMYK documents.
- **Interface fixes:** Shortcuts show Ctrl (not ⌘) on Windows in the View menu, tooltips and Keyboard shortcuts; side panels keep their width after a window resize; the command palette closes on an outside click; when GitHub rate-limits update checks, Help ▸ Check for updates says when to try again.
- **Speed & robustness:** Obsolete in-flight page renders are cancelled when you scroll or zoom, the render pool survives metadata edits, and malformed annotation colours, field flags and font descriptor flags are handled safely.
- **Windows diagnostics:** The log now records the desktop renderer (GPU adapter, `wgpu`/OpenGL) at startup, which helps diagnose display problems (`PDFCRAFT_RENDERER=gl` still forces OpenGL).

---

## Licensing & Notices

Linkco PDF Editor is dual-licensed under the **Apache License, Version 2.0** (`LICENSE-APACHE`) and the **MIT License** (`LICENSE-MIT`). Third-party asset and library notices are provided in `NOTICE` and `ATTRIBUTION.md`.
