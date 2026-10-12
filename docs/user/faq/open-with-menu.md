---
title: PdfCraft is missing from the Open with menu
description: Why PdfCraft may not show in the Windows right-click menu for PDFs, and how to open PDFs with it anyway.
order: 30
verified: 0.6.0
---

The MSI installer adds an **Edit with PdfCraft** entry to the right-click menu of PDF files. On Windows 11 it is under **Show more options**. The portable zip does not add it.

## If you do not see it

1. Check how you installed PdfCraft. A user in [issue #1080](https://github.com/storytold/pdfcraft/issues/1080) was told the portable zip registers nothing, by design. Install the `.msi` as administrator to get the entry.
2. If you want to keep the portable version, right-click a PDF, choose **Open with**, then **Choose another app**, and point it at your `pdfcraft.exe`.

The entry does not change which app opens PDFs by default.

## Still missing after the MSI

One user installed `pdfcraft-0.6.0-windows-x64.msi` with administrator rights and still did not see PdfCraft in **Open with**. A fix for that, to the MSI installer, was proposed in a pull request and had not been merged when this page was written ([issue #1080](https://github.com/storytold/pdfcraft/issues/1080)). Use the second step above meanwhile.

Ask in [Discord](https://discord.gg/artcraft) if this still does not work.
