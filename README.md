<p align="center">
  <img alt="XueLuoPDF" src="assets/app-icon/hicolor/256x256/apps/ai.storyteller.xueluopdf.png" width="128">
</p>

<h1 align="center">雪落 PDF · XueLuoPDF</h1>

<p align="center">
  <b>用纯 Rust 从零写起的开源 PDF 工作台。</b><br>
  阅读、批注、填表、签名、整理、合并、拆分、保护，一个原生应用全搞定。<br>
  macOS · Windows · Linux · FreeBSD · 浏览器
</p>

<p align="center">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-12a58a">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-0a7563">
  <img alt="Platforms: macOS, Windows, Linux, FreeBSD, web" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20FreeBSD%20%C2%B7%20web-12a58a">
  <img alt="No account, no telemetry" src="https://img.shields.io/badge/no%20account-no%20telemetry-0a7563">
</p>

<p align="center">
  <a href="https://github.com/PLSshenyun/xueluopdf"><b>github.com/PLSshenyun/xueluopdf</b></a> ·
  <a href="https://github.com/PLSshenyun/xueluopdf/issues">问题反馈</a> ·
  <a href="#下载">下载</a>
</p>

---

## 这是什么

XueLuoPDF 是一个本地优先的 PDF 应用：文档不离开你的电脑，没有账号、没有遥测、没有云端处理。它把 PDF 的对象结构当作数据模型，保存时默认**增量写入**——原始字节原样保留，新的改动追加在后面。

工程上它是一个 Cargo 工作区（约 30 个库 crate + 3 个可执行程序），引擎与界面分层：解析、渲染、编辑都在可复用的库 crate 里，界面只是最上面可替换的一层。同一个代码库同时产出桌面版、命令行版和 WebAssembly 浏览器版。

> 本项目基于 **PdfCraft**（[storytold/pdfcraft](https://github.com/storytold/pdfcraft)）改名而来，原作者与项目介绍见文末[「上游」](#上游)。

- 渲染：世界文字系统、日文竖排、彩色 emoji、渐变、软遮罩与透明
- 数据安全：增量保存、原子写入、深撤销、崩溃后自动恢复
- 可自动化：每个功能都有无头接口（CLI 与 MCP），界面本身也能被脚本驱动
- 全平台：macOS / Windows / Linux / FreeBSD 原生，另有浏览器版

## 快速开始

需要 Rust 1.95 或更高版本。

```sh
git clone https://github.com/PLSshenyun/xueluopdf
cd xueluopdf
cargo run --release -p pdfcraft -- some.pdf     # 打开桌面版
cargo run -p pdfcraft-cli -- --help             # 命令行工具
```

构建产物叫 `xueluopdf`（桌面版）和 `xueluopdf-cli`（命令行版）——crate 名仍是 `pdfcraft*`，所以上面的 `-p pdfcraft` 选择器写法不变。

几个常用任务：

```sh
cargo xtask models          # 下载「扫描与 OCR」需要的识别模型（正式发布包已内置）
cargo xtask assets          # 校验所有素材的授权与哈希
cargo xtask parity          # 查看与目标功能的对齐进度
cargo test --workspace      # 跑全部测试
```

界面语言在 **菜单 → 编辑 → 首选项…** 里切换（macOS 是 ⌘,，其他平台是 Ctrl+,），没有打开文档时也能改；Auto 跟随系统语言。简体中文、繁體中文、日本語等 20 种语言在列，其中中日文覆盖面最广（命令、对话框、面板、快捷键）。命令面板同时接受译文、英文原文和稳定的命令 id；文件名、PDF 内容、作者名以及来自引擎或操作系统的错误细节保持原样。

日文字体来自 [craft-fonts](https://github.com/storytold/craft-fonts)，它是可选的构建输入（正式发布都会带上）。要用它构建（日文界面文字、以及写入 PDF 的日文）：

```sh
git clone https://github.com/storytold/craft-fonts ../craft-fonts
CRAFT_FONTS_DIR=../craft-fonts cargo run --release -p pdfcraft -- some.pdf
```

## 阅读

- **缩放不糊。** 大页面按块（tile）渲染，任意倍率下文字都清晰。
- **坏文件也不崩。** 每页独立渲染，受损文档会尽量修复；在 pdf.js 的 983 个测试文件上 0 崩溃。
- **版式齐全。** 连续、单页、双页对开、旋转视图、全屏，以及无干扰的「阅读模式」。
- **明暗两套主题**，长时间阅读更护眼。

搜索边打边出结果，⌘G 在命中之间跳转；复制出来的文字按正确的阅读顺序排列——分栏、从右到左的文字、中日韩文字都适用。

书签、页面缩略图和文档自带的页码标签（i、ii、1、2…）帮你在大文档里定位。书签面板可以按标题过滤，父节点折叠时也能找到嵌套条目，命中结果保留祖先作为上下文；「清除」恢复完整目录且不改变展开状态。

## 整理页面

打开**整理页面**可以一次看到所有页面：

- **选择**：单击、⌘ 单击、⇧ 单击。
- **操作**：旋转、删除、插入空白页、从其他文件插入页面、前移后移。
- **撤销**：⌘Z，然后保存。

每次改动都是历史记录中的一步，可以来回走；「编辑」菜单会写明这一步是什么（例如「撤销 旋转页面」），保存之后依然可以撤销。

保存策略：

- **增量**：原始字节逐字节保留。
- **原子**：先写临时副本，再整体替换。
- **可验证**：可用 qpdf 等独立工具复核。

标签页超过窗口宽度时，用滚轮或触控板在标签栏上横向滚动，也可以拖它的横向滚动条；打开或切换文档时对应标签会滚入视野。未保存的文档标签上有一个圆点，关闭或退出前会询问。改动每分钟自动保存一次；如果应用异常退出，下次启动会提示恢复。加密文档在磁盘上保持加密。

## 合并与拆分

**合并**把任意多个 PDF 合成一个，每个来源生成一个书签，来源自身的书签嵌在下面。**提取**把选中的页面复制成新文档。**拆分**按每 *n* 页，或在你指定的页面之前切开。

过程中不会悄悄丢东西：链接与命名目标会重新指向复制后的页面，表单域保持可交互，图层保留开关默认值，附件一并带走。合并后每一页的渲染结果与来源逐像素一致。

```sh
xueluopdf-cli combine report.pdf appendix.pdf --out combined.pdf
xueluopdf-cli extract report.pdf --pages 1,3,5 --out highlights.pdf
xueluopdf-cli split   report.pdf --every 10 --out-dir parts/
```

## 加密文档

完整实现 PDF 标准安全处理器：从 40 位 RC4 到 AES-256 的所有修订版本、用户密码与所有者密码（含按 SASLprep 规范化的 Unicode 密码）、加密过滤器与仅附件加密。

作者设了限制的文档会明确提示，并尊重其权限设置；输入所有者密码即可解除限制。对加密文档的编辑会以同样的密钥加密保存。

**文档属性**里可以看到：可编辑的标题/作者/主题/关键词，用到的字体以及是否内嵌，PDF 版本、页面尺寸、标签、表单域、图层与附件，以及完整的加密信息（加密方式、用哪个密码打开的、每一项权限）。

## 批注、表单、图层与附件

- **表单**：列出每个域及其当前值，域高亮，复选框、单选按钮、列表和签名按其作者设计的样式绘制。
- **图层**：开关可选内容，页面立即重绘。
- **批注**：以对话串（thread）的形式呈现，可回复、改状态、改样式、删除。
- **附件**：可以打开或另存。

## 工具与命令

按 ⌘K（其他平台 Ctrl+K）搜索所有工具与命令，或浏览**全部工具**目录。尚未完成的工具会标注计划发布的里程碑。

## 为自动化而生

每个引擎功能都能脱离界面使用，共用一张带 JSON Schema 描述的工具表：打开、检查、把页面渲染成 PNG、提取与查找文本、旋转/删除/移动/插入页面、编辑书签与页码标签、添加/回复/改样式/删除批注（给出一个短语就能高亮它）、列出并填写表单域、设置密码保护、写元数据、撤销重做、保存、合并、提取、拆分。三个入口共用这张表：

**1. `xueluopdf-cli run`** —— 单次调用或 JSON 脚本：

```sh
xueluopdf-cli tools                                     # 列出所有工具及其 JSON Schema
xueluopdf-cli run text_find doc=1 query=invoice         # key=value，值按 JSON 解析
xueluopdf-cli run --script review.json                  # 例如 comment_add {"type": "highlight", "find": "total due"}
xueluopdf-cli run --script steps.json --root ./work     # 一次会话跑多步
```

带 `--root` 时，脚本每一步读写文件都限制在该目录内，包括步骤用 `"out"` 保存的 PNG（脚本本身可以从任意路径读取）。

**2. MCP 服务器** —— 给 Claude 之类的 AI 代理用。**它是选择性开启的**：应用自己绝不会启动它，也不监听任何网络端口。只有代理主动运行 `xueluopdf-cli mcp` 时才工作，通过 stdin/stdout 通信，代理断开即退出。加到代理的 MCP 配置里即可：

```json
{ "mcpServers": { "xueluopdf": { "command": "xueluopdf-cli", "args": ["mcp", "--root", "/path/to/your/pdfs"] } } }
```

`--compact` 会缩小代理需要读的工具清单：`tools/list` 只返回约十个核心工具加上 `tool_search` 和 `tool_call`，由它们查找并运行其余工具，token 开销小得多，但每个工具依然可用。`--root` 把代理能读写的文件限制在一个目录里。完全不想包含这个服务器的构建可以加 `cargo build -p pdfcraft-cli --no-default-features`。

**3. Rust API**（`pdfcraft_automation::Automation::call`）—— 用于把引擎嵌进别的程序。

编辑都留在内存里、可撤销，直到调用 `doc_save`。保存到同一个文件会追加一个增量更新，原始字节因此得以保留，写入是原子的。未保存的改动绝不会被静默丢弃。

### 驱动界面本身

用 `xueluopdf --control ~/.xueluopdf-control.json` 启动桌面版，代理就能看到并操作真实界面：控件树（含标签与位置，取自无障碍树）、点击、输入、按键、执行命令、改视图选项、截图。这个通道同样默认关闭，只监听回环地址，每个连接都必须带上写入该文件的随机令牌，而那个文件只有你能读。

控制文件请放在只有你能写的目录里，不要放在 `/tmp` 这类共享目录：否则别人可以先创建同名文件，从而接收你的命令。`xueluopdf-cli ui` 会拒绝符号链接形式、以及在 macOS / Linux / FreeBSD 上属于他人或他人可读写的控制文件。应用写不出这个文件时不会启动。

```sh
xueluopdf-cli ui --control ~/.xueluopdf-control.json inspect query=rotate      # 查找控件
xueluopdf-cli ui --control ~/.xueluopdf-control.json click label="整理页面"
xueluopdf-cli ui --control ~/.xueluopdf-control.json key key=K modifiers='["command"]'
xueluopdf-cli ui --control ~/.xueluopdf-control.json command id=comment.square   # 选一个工具再画：
xueluopdf-cli ui --control ~/.xueluopdf-control.json drag from='[400,300]' to='[600,420]'
xueluopdf-cli ui --control ~/.xueluopdf-control.json screenshot --out window.png
xueluopdf-cli ui --control ~/.xueluopdf-control.json resize width=1200 height=800
xueluopdf-cli ui --control ~/.xueluopdf-control.json focus label="阅读"
xueluopdf-cli ui --control ~/.xueluopdf-control.json inspect focused=true
xueluopdf-cli ui --control ~/.xueluopdf-control.json quit
```

`ui.dock` 用来排布 `tools` 与 `inspector` 面板；`canvas` 是受保护的文档视图。每个面板请求需要一个 `panel` ID 和一个 `operation`：`open`、`close`、`activate`、带 `rect: [x, y, width, height]` 的 `float`，或带 `target` 与 `zone: center|left|right|top|bottom` 的 `move`。例如 `{"operation":"float","panel":"tools","rect":[100,100,320,440]}` 会把「工具」面板浮动出来。`ui.dock.reset` 恢复默认排布。控制通道直接接受这些方法；`ui.command` 也接受 `{"id":"ui.dock","params":{...}}`。保存的偏好会记住分组、活动标签、可见性与浮动位置。非法请求会返回错误，不会改动文档或布局。

`resize` 以逻辑点请求原生窗口内尺寸（每个维度必须在 320 到 8192 之间），窗口管理器仍可能做限制；`state` 返回观察到的 `window.width`、`window.height` 与 `window.pixels_per_point`。`focus` 接受一个控件 `id`，或一个唯一的、可获得焦点且启用的 `label`；之后的 `key` 请求会遍历或激活真实控件。`inspect` 会给出是否可获得焦点，并接受 `focused` 过滤器。`quit` 不带参数：没有未保存改动就直接退出，否则弹出既有的「保存/不保存/取消」提示并返回 `quitting: false, needs_confirmation: true`——它绝不会静默丢弃你的工作。窗口缩放与退出应用只在原生版本可用。

## 技术结构

XueLuoPDF 是一个按职责切分的 Cargo 工作区，分层设计，核心不依赖界面：

| crate | 负责什么 |
|---|---|
| `pdfcraft-filters` | 全部 PDF 流过滤器（Flate、LZW、ASCII85、RunLength、predictors），编码与解码，带属性测试 |
| `pdfcraft-crypt` | 标准安全处理器：RC4、AES-128/256、修订版 2–6、权限位 |
| `pdfcraft-cos` | PDF 对象层：容错解析、修复、写时复制编辑、增量与完整写入 |
| `pdfcraft-organize` | 页面操作、合并/提取/拆分、书签、页码标签、文档信息 |
| `pdfcraft-fonts` | 生成外观流所需的字体度量与编码 |
| `pdfcraft-annot` | 批注：便签、文本标注、图形、墨迹、文本框的构造与外观流；回复、状态、编辑 |
| `pdfcraft-forms` | 交互表单：域模型、重绘外观的填写、清除表单 |
| `pdfcraft-render` | 渲染、检查与按阅读顺序的文本提取 |
| `pdfcraft-engine` | 所有前端共用的门面：会话、编辑、撤销、保存、工具目录 |
| `pdfcraft-automation` | 代理控制：无头工具表、`xueluopdf-cli run`、选择性开启的 MCP 服务器 |
| `pdfcraft-ui-egui` | 桌面与网页界面 |

**质量门禁。** 每次改动都要过同一套自动检查：格式与 Clippy（警告即错误）、200+ 单元/属性/界面测试、crate 分层规则、WebAssembly 构建检查，以及素材授权审计。此外还有两轮真实文件语料扫描：983 个 pdf.js 测试文件中 963 个可正常打开并渲染、0 崩溃；打开—编辑—保存往返 958 个成功。输出用 hayro、qpdf 与 poppler 等独立工具复核。

行为来自 ISO 32000 规范与黑盒观察，不来自任何他人的代码。每一个图标、字体和图片都有开放授权，逐项列在 [ATTRIBUTION.md](ATTRIBUTION.md)。

## 下载

构建产物发布在本仓库的 [releases](https://github.com/PLSshenyun/xueluopdf/releases) 页面。文件名里的 `<ver>` 是版本号，`SHA256SUMS.txt` 列出每个文件的校验和。

### Windows

| 版本 | 安装包 | 免安装 |
|---|---|---|
| x64（64 位 Intel/AMD） | `xueluopdf-<ver>-windows-x64.msi` | `xueluopdf-<ver>-windows-x64-portable.zip` |
| arm64（骁龙等 ARM 电脑） | `xueluopdf-<ver>-windows-arm64.msi` | `xueluopdf-<ver>-windows-arm64-portable.zip` |
| x86（32 位） | `xueluopdf-<ver>-windows-x86.msi` | `xueluopdf-<ver>-windows-x86-portable.zip` |

安装包与可执行文件都经过代码签名。免安装版可以从任何目录（包括 U 盘）运行：它旁边的 `portable.txt` 让设置、日志与崩溃恢复到 `xueluopdf.exe` 同级的 `XueLuoPDFData` 目录，不写 `%APPDATA%`；删掉这个文件就改用常规的每用户目录。

MSI 为所有用户安装，需要管理员权限。它会在文件资源管理器的 PDF 右键菜单里加入「使用 XueLuoPDF 编辑」（Windows 11 需在「显示更多选项」里找），但不会改变默认打开 PDF 的程序。无人值守安装（不要桌面快捷方式）：

```powershell
msiexec /i "xueluopdf-<ver>-windows-x64.msi" /qn /norestart INSTALLDESKTOPSHORTCUT=0
```

再加 `INSTALLPDFCONTEXTMENU=0` 可以不要右键菜单项。升级时也要带上这些参数，因为每次安装都按自己的选择执行。请选与自己架构对应的 MSI；不支持改为每用户安装。

### macOS

| 构建 | 文件 | 说明 |
|---|---|---|
| 应用（通用，Apple 芯片 + Intel） | `xueluopdf-<ver>-macos-universal.dmg` | 已签名并公证 |
| 命令行工具（通用） | `xueluopdf-cli-<ver>-macos-universal.zip` | 已签名并公证 |

### Linux

| 格式 | x86_64 | aarch64 (ARM64) | 说明 |
|---|---|---|---|
| AppImage | `xueluopdf-<ver>-linux-x86_64.AppImage` | `xueluopdf-<ver>-linux-aarch64.AppImage` | 随处可运行；用 [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate) 自更新（`.zsync`） |
| Flatpak | `xueluopdf-<ver>-linux-x86_64.flatpak` | `xueluopdf-<ver>-linux-aarch64.flatpak` | 沙箱运行；`flatpak install --user <文件>` |
| Debian/Ubuntu | `xueluopdf-<ver>-linux-x86_64.deb` | `xueluopdf-<ver>-linux-aarch64.deb` | |
| Fedora/RHEL/openSUSE | `xueluopdf-<ver>-linux-x86_64.rpm` | `xueluopdf-<ver>-linux-aarch64.rpm` | |
| tar 包 | `xueluopdf-<ver>-linux-x86_64.tar.gz` | `xueluopdf-<ver>-linux-aarch64.tar.gz` | 解压即用 |
| 命令行工具 | `xueluopdf-cli-<ver>-linux-x86_64.tar.gz` | `xueluopdf-cli-<ver>-linux-aarch64.tar.gz` | 只含 `xueluopdf-cli`（及可选的 MCP 服务器），适合服务器、CI 与代理 |

RISC-V（riscv64）只有 tar 包：`xueluopdf-<ver>-linux-riscv64.tar.gz` 与 `xueluopdf-cli-<ver>-linux-riscv64.tar.gz`，交叉编译，需要 glibc 2.39 或更高（Ubuntu 24.04+、Debian 13+）。其他 Linux 构建需要 glibc 2.35 或更高（Ubuntu 22.04+、Debian 12+、Fedora 36+、RHEL 10）。

### FreeBSD

| 构建 | 文件 |
|---|---|
| x86_64 | `xueluopdf-<ver>-freebsd-x86_64.tar.gz` |

### Web（WebAssembly）

| 构建 | 文件 | 说明 |
|---|---|---|
| 静态站点 | `xueluopdf-web-<ver>.zip` | 在现代浏览器中运行；可托管在任意静态服务器上 |

## 现状与计划

目标是一个能看、能整理、能批注、能填表签名、能编辑 PDF 的工作台，对齐 Acrobat Pro 的离线能力。

**坦率地说（2026 年 10 月）：** 目前是 **alpha**。Acrobat Pro 的离线功能大多已具雏形（占全部跟踪功能约 57%，必做项约 94%）。日常使用（查看、批注、填表签名、整理页面）大约达到可替代 Acrobat 的 55%；对比全部 Acrobat Pro 约 49%；如果只需要基本功能，约 66%。日常流程都能走通，但转换与共享偏弱：没有 Office 转 PDF，Word 导出会丢版式。

- **目前可用**：查看与搜索；整理、合并、拆分；大多数批注；表单填写与制作（带沙箱 JavaScript）；密码、涂黑与清理；基础数字签名；打印；无障碍检查器；通过 CLI 与 MCP 的代理控制。
- **仍在借用**：页面绘制暂时依赖 `hayro` crate，自研渲染器仍在建设中。
- **薄弱或缺失**：对既有文字的可靠编辑（尤其中日韩与从右到左文字）、拉丁字母以外的 OCR、Office 导入导出、签名令牌与在线长期验证、Preflight 与 PDF/X/UA、网页版打印、实体打印机保真、完整的 XFA 表单。
- **加固中**：模糊测试仍会在恶意文件上发现崩溃与卡死，每一个都补上回归测试后才修复；质量尚未与 Acrobat 做过逐项对照。

**接下来的顺序**：Office 转 PDF 与保留版式的 Word 导出、已报告的问题与中文字体、与 Acrobat 对照的保真度基准与渲染器、编辑既有内容，然后是专业工作流（OCR、Office、Preflight、签名、无障碍标注）与 1.0 打磨。

完整评估见 [ROADMAP.md](ROADMAP.md)、[docs/target-app-parity.md](docs/target-app-parity.md) 与 [docs/gaps.md](docs/gaps.md)，里程碑表在 [docs/roadmap.md](docs/roadmap.md)。开发相关的日志、环境变量等说明在 [docs/development.md](docs/development.md)。

## 授权

XueLuoPDF 采用 [MIT](LICENSE-MIT) 或 [Apache-2.0](LICENSE-APACHE) 双授权，任选其一。版权与上游声明见 [NOTICE](NOTICE)。

随附的字体、图标、图片等素材各自保留其开放授权，逐项的作者、来源与许可列在 [ATTRIBUTION.md](ATTRIBUTION.md)。正式发布版还会内嵌 [craft-fonts](https://github.com/storytold/craft-fonts/blob/main/ATTRIBUTION.md) 的日文字体（SIL Open Font License 1.1）。

## 上游

XueLuoPDF 基于 **PdfCraft**（[storytold/pdfcraft](https://github.com/storytold/pdfcraft)）改名而来，原作者与项目介绍属于他们；本仓库只是换了名称与图标，引擎、界面与文档主体都是上游的工作成果。两者的授权相同（MIT OR Apache-2.0），上游的 ArtCraft 商标素材按授权要求已从本仓库移除。

Adobe、Photoshop、Illustrator、Premiere Pro、Lightroom、Acrobat、After Effects 与 InDesign 是 Adobe Inc. 在美国和其他国家的商标或注册商标。XueLuoPDF 是独立的开源项目，与 Adobe Inc. 无隶属关系，也未获得其赞助或认可；上述名称仅用于说明它所兼容的工作流。
