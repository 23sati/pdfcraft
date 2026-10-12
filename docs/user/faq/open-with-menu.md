---
title: PdfCraft is missing from Open with
description: Why PdfCraft may be missing from Open with or Default apps on Windows 11, and how to open PDFs with it anyway.
order: 30
verified: 0.6.0
---

The MSI installer registers PdfCraft for PDF files, so it appears under **Open with** and in Windows' **Default apps** settings. It does not change which app opens PDFs by default. The portable zip registers nothing. An **Edit with PdfCraft** right-click entry was added after 0.6.0 and is not in a release yet.

## If you do not see it

1. Check how you installed PdfCraft. A user in [issue #1080](https://github.com/storytold/pdfcraft/issues/1080) was told the portable zip registers nothing, by design. Install the `.msi` as administrator to get PdfCraft under **Open with**.
2. If you want to keep the portable version, right-click a PDF, choose **Open with**, then **Choose another app**, and point it at your `pdfcraft.exe`.

## Installed with the MSI and still missing

One user installed `pdfcraft-0.6.0-windows-x64.msi` with administrator rights and did not see PdfCraft in **Open with** ([issue #1080](https://github.com/storytold/pdfcraft/issues/1080), still open). Use the second step above, and tell us on the issue if you see the same.

Ask in [Discord](https://discord.gg/artcraft) if this still does not work.
