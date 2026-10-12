---
title: Chinese text in the interface shows as boxes
description: Why the Simplified Chinese interface of PdfCraft 0.6.0 shows empty boxes for some characters, and what is changing.
order: 30
verified: 0.6.0
---

In PdfCraft 0.6.0, some Chinese characters in the menus and dialogs, mostly Simplified-only ones, appear as empty boxes. The release has no Simplified Chinese interface font. A change merged after 0.6.0 draws them with a Chinese font already installed on your PC. Until you have it, switch the interface language to English or another language.

## Symptoms

- With the interface in Simplified Chinese, characters such as 欢, 签 and 导 show as boxes. Only characters shared with the Japanese fonts draw correctly ([issue #984](https://github.com/storytold/pdfcraft/issues/984)).
- It affects the installer and the portable zip alike (issue #984, Windows with a Simplified Chinese locale).

## Fix

Change the interface language:

1. Click the **Menu** button, then **Edit**, then **Preferences…** (`Ctrl+,`).
2. Set **Interface language** to a language you can read. **Auto** follows your system language.

A user in [issue #778](https://github.com/storytold/pdfcraft/issues/778) reported that choosing Traditional Chinese removed the boxes. Another user replied that Simplified Chinese text still turned into boxes when editing. The project notes say Traditional Chinese is drawn with fonts that miss some characters, which then show as boxes.

## Why this happens

PdfCraft's release builds embed Japanese fonts from craft-fonts but no Simplified Chinese interface font. Changing that is on the project's list ([ROADMAP](https://github.com/storytold/pdfcraft/blob/main/ROADMAP.md), issue #826).

The change after 0.6.0 ([#925](https://github.com/storytold/pdfcraft/pull/925)) makes PdfCraft read one Chinese font that is already installed on your PC when the build has none. It does not copy or ship that font. It is in the main branch, not in 0.6.0: the 0.6.0 source of `system_fonts.rs` has no Chinese probe.

## Still stuck?

Comment on [issue #984](https://github.com/storytold/pdfcraft/issues/984) or ask in [Discord](https://discord.gg/artcraft). Say your Windows or macOS version and the PdfCraft version from **Help**, **About PdfCraft**.
