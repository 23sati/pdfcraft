---
title: PdfCraft does not start
description: What to do when PdfCraft 0.6.0 closes right after launch on Windows 11, with a graphics error in its log.
order: 10
verified: 0.6.0
---

On Windows 11, first check that you installed the build that matches your PC (`x64` for most). If it still closes at launch, start PdfCraft with OpenGL: set `PDFCRAFT_RENDERER` to `gl`. The steps are below.

## Symptoms

- PdfCraft closes at launch or never shows a window.
- The log file `pdfcraft.log` contains lines like these:

      Shader translation error for stage ShaderStages(VERTEX | FRAGMENT): The parameter is incorrect. (0x80070057)
      In Device::create_render_pipeline, label = 'egui_pipeline'

This was reported for version 0.6.0 on NVIDIA and Intel graphics ([issue #1046](https://github.com/storytold/pdfcraft/issues/1046)).

The log is in `%APPDATA%\PdfCraft\data\logs\pdfcraft.log`. With the portable zip it is in the `PdfCraftData` folder next to `pdfcraft.exe`.

## Fix

### 1. Use the right build

Two users in the issue had installed the 32-bit `x86` build on a 64-bit PC by mistake. Uninstall it and install `pdfcraft-<version>-windows-x64.msi` instead. See [Install PdfCraft](../getting-started/install.md).

### 2. Start with OpenGL

A user in the issue got PdfCraft 0.6.0 running this way.

1. Close PdfCraft.
2. Open PowerShell.
3. Run these two commands. Use the path of your own `pdfcraft.exe`; the reporter's x64 guess was `C:\Program Files\PdfCraft\pdfcraft.exe`, but check yours.

       $env:PDFCRAFT_RENDERER = 'gl'
       & 'C:\Program Files\PdfCraft\pdfcraft.exe'

PdfCraft should open. The setting applies only to that PowerShell window.

To make it permanent for your Windows account, run this in PowerShell, then sign out of Windows and sign back in:

    [Environment]::SetEnvironmentVariable('PDFCRAFT_RENDERER', 'gl', 'User')

To undo it, run the same command with `$null` instead of `'gl'`, and sign out and in again.

## Why this happens

On Windows, PdfCraft draws with Direct3D 12 by default. On some PCs that fails to compile PdfCraft's shaders. In 0.6.0 the error ends the program before PdfCraft can fall back to OpenGL by itself. `PDFCRAFT_RENDERER=gl` starts with OpenGL from the beginning.

A change that lets PdfCraft retry with OpenGL on its own was merged after 0.6.0 ([#1042](https://github.com/storytold/pdfcraft/pull/1042)). It is not in 0.6.0. It does not fix Direct3D 12 itself.

## Still stuck?

Ask in [Discord](https://discord.gg/artcraft) or comment on [issue #1046](https://github.com/storytold/pdfcraft/issues/1046). Say your Windows version, graphics card, which PdfCraft file you installed, and attach `pdfcraft.log`.
