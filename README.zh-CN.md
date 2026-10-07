# OrangeData

轻量可视化数据库客户端 —— Tauri 2 + Rust + 原生 JS，**没有前端构建步骤**。
支持 **SQLite / MySQL / PostgreSQL**，可连接本地文件、Docker 容器与远程实例。

[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Release](https://img.shields.io/github/v/release/xiaoins/OrangeData)](https://github.com/xiaoins/OrangeData/releases)
[![CI](https://github.com/xiaoins/OrangeData/actions/workflows/ci.yml/badge.svg)](https://github.com/xiaoins/OrangeData/actions/workflows/ci.yml)

English docs: [README.md](README.md)

## 下载

到 [Releases 页面](https://github.com/xiaoins/OrangeData/releases) 直接下载安装包：

| 平台 | 产物 |
| --- | --- |
| Windows 10/11 (x64) | `OrangeData_x.y.z_x64-setup.exe`（NSIS，当前用户安装） |
| macOS（Apple Silicon / Intel） | `OrangeData_x.y.z_aarch64.dmg` / `..._x64.dmg` |
| Linux | `OrangeData_x.y.z_amd64.deb` 与 `.AppImage` |

安装包**尚未做代码签名与公证**，首次启动会遇到 Windows SmartScreen（"更多信息 → 仍要运行"）和
macOS Gatekeeper（`xattr -dr com.apple.quarantine /Applications/OrangeData.app`）。

## 功能

- **连接管理**：连接配置持久化到 `connections.json`，本地 / Docker / 远程三种填法，一键测试连接
- **Docker 发现**：扫描运行中的容器，读取 `MYSQL_*` / `POSTGRES_*` 环境变量自动回填主机、端口、账号、库名
- **对象树**：数据库 → 模式 → 表/视图，带行数与注释徽标
- **数据网格**：分页、点列排序、逐列筛选（`=` `<>` `>` `>=` `<` `<=` `contains` `starts` `ends` `null` `notnull`）、双击改单元格、`∅` 置 NULL、标记删除、新增行；按主键在一个事务里回写
- **SQL 工作表**：多语句脚本（遇错即停）、每条语句独立结果页签、执行历史（最多 200 条）、Ctrl+Enter 运行
- **结构面板**：字段 / 主键 / 外键 / 索引 / DDL，可复制 DDL、生成 SELECT
- **数据总览**：表数、视图数、字段数、总行数、占用，行数与体积排行条形图
- **ER 关系图**：SVG 绘制表卡与外键连线；拖拽卡片改布局、拖空白处平移、滚轮或 `−`/`+`/`适应窗口` 缩放，点卡片直接打开数据
- **CSV 导出**：由 Rust 侧写文件，只申请对话框权限，不开放文件系统权限

## 技术选型

| 层 | 选择 | 原因 |
| --- | --- | --- |
| 壳 | Tauri 2（Rust） | 安装包 ~4 MB，不捆绑 Chromium |
| 驱动 | sqlx 0.8（`sqlite` / `mysql` / `postgres`） | 一套异步驱动，不走 ODBC |
| 界面 | 纯 HTML/CSS/JS | 无打包器、无框架，运行时不需要 `node_modules` |
| 标题栏 | Windows 上的 `DwmSetWindowAttribute` | 原生标题栏跟随应用内主题 |

`web/` 直接作为 `frontendDist` 提供，改 `.js`/`.css` 只需重新打包资源，没有任何转译步骤。

## 目录结构

```
src-tauri/
  src/
    main.rs            Tauri 入口，注册 26 个命令
    commands.rs        IPC 层：解析会话 → 切库 → 交给 api
    model.rs           请求/响应结构（serde camelCase）
    store.rs           连接配置与 SQL 历史（原子写入 tmp+rename）
    chrome.rs          用实时 CSS 变量驱动 DWM 标题栏着色
    docker.rs          docker ps / inspect 解析与凭据回填
    db/
      dialect.rs       方言契约：全部引擎差异都是纯字符串构造
      dialect_sqlite.rs / mysql_dialect.rs / pg_dialect.rs
      engine.rs        Db trait（query/execute/tx）+ 三驱动 + 会话注册表
      api.rs           引擎无关的浏览 / 编辑 / 元数据 / 导出 / 图
      values.rs        行 → JSON 的逐类型探测
  capabilities/        极窄权限（仅 core + 保存对话框，无文件系统 scope）
web/                   index.html + style.css + app/grid/info/sql/conn.js
assets/app-icon.png    `npx tauri icon` 的源图
```

## 开发

前置依赖：**Node 18+**、**Rust stable**，以及 WebView —— Windows 上是 WebView2（Win11 自带），
Linux 需要 `libwebkit2gtk-4.1-dev`，macOS 用系统自带 WebKit.framework。Windows 还需要 MSVC 工具链
（Tauri 的 build script 会调用链接器）。

```bash
npm install
npm run dev          # tauri dev，调试窗口
cargo check          # 在 src-tauri 下做快速类型检查
```

### 打包

```bash
npm run build                              # 使用 tauri.conf.json 里的 bundle.targets（nsis）
npx tauri build --bundles nsis             # Windows
npx tauri build --bundles app,dmg          # macOS
npx tauri build --bundles deb,appimage     # Linux
```

产物在 `src-tauri/target/release/bundle/<类型>/`。

### 发布版本

版本号有三处：`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`。
三处一起改，然后：

```bash
git tag v0.2.0
git push origin v0.2.0
```

`.github/workflows/release.yml` 会构建全部四个目标，创建以 tag 命名的草稿 Release，并把每个安装包
作为附件传上去。确认无误后在 Releases 页面点 "Publish release"。`v1.2.3-beta.1` 这类带连字符的 tag
会自动标记为预发布。

## 安全约束

表名 / 列名 / 库名无法作为参数绑定，因此先经 `check_ident` 白名单校验（拒绝 `\0`、`--`、`;`、`/*`、
超长），再按方言加引号（`` ` `` 或 `"`）。所有数据值一律参数绑定；PostgreSQL 额外包一层
`CAST(? AS 安全类型)`。分页的 `LIMIT/OFFSET` 由 Rust 侧以整数内联，不走文本参数。

## 本机环境备注

Rust 工具链按目录约定装在 `D:\soft\tool\context\rust`：

```bat
setx RUSTUP_HOME D:\soft\tool\context\rust\rustup
setx CARGO_HOME  D:\soft\tool\context\rust\cargo
setx PATH        "%PATH%;D:\soft\tool\context\rust\cargo\bin"
```

crates.io 直连在本机网络下只有 ~2.5 KB/s，因此用 `.cargo/config.toml` 换成 rsproxy 源。该路径已在
`.gitignore` 中忽略，只影响本机、不进仓库，也不动全局 cargo 配置：

```toml
[source.crates-io]
replace-with = "rsproxy"

[source.rsproxy]
registry = "sparse+https://rsproxy.cn/index/"
```

在 Git Bash 里临时启用工具链时，PATH 必须写成 POSIX 形式，`D:/...` 不会被解析，否则报
`rustc: command not found`：

```bash
export RUSTUP_HOME="D:/soft/tool/context/rust/rustup"
export CARGO_HOME="D:/soft/tool/context/rust/cargo"
export PATH="/d/soft/tool/context/rust/cargo/bin:$PATH"   # 不是 D:/soft/...
```

CMD / PowerShell 用上面 `setx` 的反斜杠路径即可，不受此限制。

## 常见问题

- **`missing manifest in toolchain`**：安装被中断。`rustup toolchain uninstall stable && rustup toolchain install stable --profile minimal`
- **npm EPERM 写不进缓存**：`npm install --cache D:\test\npm-cache` 临时换可写目录，不要改全局配置
- **SQLite 路径不存在**：后缀为 `.db/.sqlite/.sqlite3/.db3` 且父目录存在时会自动新建空库
- **表格只读**：该表没有主键，改用 SQL 工作表写入

## 贡献

欢迎提 Issue 和 PR。开 PR 前请本地跑 `cargo check --all-targets` 与 `node --check web/*.js`，
这正是 CI 做的两件事。

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
