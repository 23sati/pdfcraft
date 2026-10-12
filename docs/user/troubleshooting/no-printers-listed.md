---
title: No printers are listed in the Print dialog
description: Why PdfCraft 0.6.0 shows no printers on Windows 11 and Linux, and how to print meanwhile.
order: 20
verified: 0.6.0
---

In PdfCraft 0.6.0 on Windows 11, the Print dialog lists only **Save as PDF**. Choose **Save as PDF**, then print the saved file from another PDF app or your browser. A fix is merged but not in 0.6.0.

Shortcuts on this page use Ctrl. On macOS, use Cmd instead of Ctrl.

## Symptoms

- **File**, **Print…** (`Ctrl+P`) opens the **Print** dialog, but the **Printer:** list has no printers.
- Only **Save as PDF** is available ([issue #1031](https://github.com/storytold/pdfcraft/issues/1031), [issue #1084](https://github.com/storytold/pdfcraft/issues/1084)).

## Fix

### Windows 11

1. In the **Print** dialog, choose **Save as PDF** and save the file.
2. Open the saved PDF in another app and print it from there. A reporter printed with Google Chrome.

Printing for Windows ([pull request #847](https://github.com/storytold/pdfcraft/pull/847)) was merged on 11 October 2026, a few hours after 0.6.0 was published. Once you install a release that includes it, the Print dialog should list your printers.

On a PC with App Control for Business script enforcement, printing can still fail. This is open as [issue #1107](https://github.com/storytold/pdfcraft/issues/1107).

### Linux

Printers come from CUPS. A user in issue #1084 reported that printing works on Arch Linux when CUPS is set up and working. Check that your printer shows in your system's printer settings first. PdfCraft 0.6.0 also lists CUPS printers that have no permanent queue.

## Why this happens

Before pull request #847, PdfCraft had no way to list printers on Windows.

## Still stuck?

Ask in [Discord](https://discord.gg/artcraft) or comment on [issue #1084](https://github.com/storytold/pdfcraft/issues/1084). Say your system and the printer model.
