---
title: Edit with PdfCraft is missing from the right-click menu
description: Why the Edit with PdfCraft entry or PdfCraft in Open with may be missing on Windows 11, and how to open PDFs with it anyway.
order: 30
verified: 0.6.0
---

The MSI installer adds an **Edit with PdfCraft** entry to the right-click menu of PDF files. On Windows 11 it is under **Show more options**. The portable zip does not add it.

## If you do not see it

1. Check how you installed PdfCraft. A user in [issue #1080](https://github.com/storytold/pdfcraft/issues/1080) was told the portable zip registers nothing, by design. Install the `.msi` as administrator to get the entry.
2. If you want to keep the portable version, right-click a PDF, choose **Open with**, then **Choose another app**, and point it at your `pdfcraft.exe`.

The entry does not change which app opens PDFs by default.

## Installed with the MSI and still missing

One user installed `pdfcraft-0.6.0-windows-x64.msi` with administrator rights and did not see PdfCraft in **Open with** ([issue #1080](https://github.com/storytold/pdfcraft/issues/1080), still open). Use the second step above, and tell us on the issue if you see the same.

Ask in [Discord](https://discord.gg/artcraft) if this still does not work.
