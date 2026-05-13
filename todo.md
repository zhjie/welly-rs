# Welly-rs 前后端分离 & 迁移到 gpui — TODO

当前目标：逐步把 welly-rs 的前端与后端分离，最终支持 gpui 前端（并保留 egui 作为 fallback / 兼容选项），同时为将来可能的浏览器/远程前端保留 IPC/序列化契约。

---

## 直接结论（建议）

- 先把后端逻辑抽成独立 crate（welly-core / welly-backend），前端做成独立 crate（welly-egui / welly-gpui）。保持单仓库 workspace。
- 先做“接口+契约”分离（TerminalSnapshot + InputEvent + backend API），并在现有 egui 前端上验证契约后再切换到 gpui。
- 保留 egui 作为 fallback，在用户平台或功能未就绪时可以回退。
- 优先采用“同进程前端替换”，降低复杂度；只有在需要嵌入浏览器或远程访问时再考虑 IPC/HTTP/WS。

---

## 高层迁移路径（里程碑与时间预估）

1) 代码重构（1–3 周，低风险、增量）
   - 建立 Cargo workspace：crates/welly-core（纯逻辑）、crates/welly-backend（连接/事件循环）、crates/welly-egui（thin frontend）。
   - backend 暴露稳定 public API：TerminalBackend、TerminalSnapshot、InputEvent。
   - 把非 UI 的逻辑（attachment 探测、ssh 管理、ANSI 解析、历史/缓存）移入 backend，main.rs 变成 tiny 前端启动器。
   - 验证：功能保持一致、CI 通过。

2) 接口与契约硬化（1–2 周）
   - 定义 snapshot 的稳定序列化（serde），便于未来 IPC/remote frontends。
   - 明确快照频率（差分 vs 全量）、保证前端不阻塞（使用 cheap clone 或 ring buffer）。
   - 验证：高频更新场景无卡顿，快照内存/复制成本可接受。

3) gpui 原型（2–6 周）
   - 用 gpui + Zed 1.1.8 写 minimal frontend（渲染 cells、光标、标题、attachments、简单输入）。
   - 验证点：CJK 渲染、box art（或 xterm.js 方案）、IME、鼠标/剪贴板/拖拽/鼠标报告、bracketed paste。
   - 若 gpui 在某平台（mac）不够成熟：继续保留 egui 为 fallback，或将 gpui 标为 experimental。
   - 验证：target 平台上做 e2e 测试，记录性能指标。

4) 选项/发布策略（1–2 周）
   - 用 feature flags 或 runtime 配置允许用户选择 UI（egui/gpui/xterm.js）。
   - 先做小范围 beta 发布，收集反馈后再默认切换。
   - 更新 README、release notes、迁移指南。

---

## 关键工程/设计细节

### A. 推荐 workspace & crate 结构
- workspace Cargo.toml
  - crates/welly-core: types, terminal model (Cell 等), parser（无 tokio）
  - crates/welly-backend: ssh、连接管理、事件 loop（依赖 tokio），暴露 TerminalBackend
  - crates/welly-egui: 现有 main.rs thin frontend（依赖 welly-backend）
  - crates/welly-gpui: gpui 前端（依赖 welly-backend）
  - examples/：不同前端的 demo

### B. TerminalSnapshot / InputEvent（放在 welly-core）

```rust
// crates/welly-core/src/lib.rs
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub attrs: CellAttrs,
    pub width: u8, // 1 or 2 for CJK
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TerminalSnapshot {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Vec<Cell>>, // rows
    pub cursor: Option<(usize, usize)>,
    pub title: Option<String>,
    pub attachments: Vec<AttachmentLink>,
    pub dirty_regions: Vec<Rect>, // 可选差分
}

#[derive(Clone, Serialize, Deserialize)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize { width: usize, height: usize },
}
```

注意：cells 用 Vec<Vec<Cell>> 简单明了；性能瓶颈可改为 flat Vec 与行索引。实现时尽量保证 Clone cheap 或提供 Cow/Arc 快照。

### C. 前后端通信方式（两种备选）
- 同进程（首选）
  - backend 持有 Arc<RwLock<TerminalModel)>, 提供 snapshot() -> TerminalSnapshot（Clone），前端通过 tokio::sync::watch 或 crossbeam-channel 订阅更新。
  - 优点：低延迟，简单。
- 异进程 / 浏览器前端（备用）
  - backend 提供本地 websocket 或 unix domain socket + JSON / bincode 协议；前端可以是 electron/xterm.js 或远程 web UI。
  - 优点：可远程访问、embed 在浏览器；缺点：运维/安全/版本绑定复杂度。

### D. gpui 特有注意事项
- 字体与 shaping：确认 HarfBuzz/Font fallback 在目标平台行为一致（emoji、CJK、宽字符）。
- Glyph atlas 与 缓存：建议使用 glyph atlas 预渲染 glyphs 到 texture，避免每帧 CPU raster 文本。
- Box art：如果倾向于 xterm.js（复杂 UI），可以先在 web 端实现；若在 gpui 上实现，基于 glyph atlas 的字符绘制 + canvas 线段通常足够。
- IME / composition：确认 macOS 上 IME 行为；若未成熟，继续用 egui/mac 作特殊处理或保留回退。

### E. 测试、CI 与打包
- CI matrix：linux/x86_64, linux/aarch64, windows, macos（若无法在 CI 上跑 mac，可用本地 runner）。
- 自动化 E2E：编写 headless test harness 模拟高频输入、ANSI-heavy workloads、CJK+emoji 测试集。
- 发行包：继续现在的 release 流程，先以“experimental gpui”标签发布二进制并写清楚平台支持与回退方法。

---

## 回退与兼容策略
- ���用 feature flag（cargo 或 runtime）控制是否启用 gpui；发布时把 gpui 标注为 experimental，默认仍用 egui。
- 若采用 IPC（backend server），前端版本不兼容时返回版本 mismatch 并提示用户升级。

---

## 性能与监测
- 在 prototype 阶段量化：帧率（fps）、平均 CPU%、max frame time（ms）、内存、GPU VRAM。
- 记录 baseline（egui）与 gpui 对比。
- 考虑添加 opt-in 的轻量 telemetry 或 debug 界面显示当前 fps/latency/queued events。

---

## 我可以帮你落地的事项（可选）
- 生成 workspace + 初始 crate 结构并提交 PR（把 main.rs 分离出 backend skeleton）。
- 写好 TerminalSnapshot / InputEvent 的具体 types（serde 支持），并把部分文件移动到 crates/welly-core 的草稿 PR。
- 搭建 minimal gpui frontend prototype（渲染 cells & input）并提交 demo 分支。
- 更新 CI matrix，添加 gpui build jobs 与跨平台打包脚本。

---

## 下一步（建议）
如果你想稳妥开始：让我先把仓库拆成 workspace，并把 core types 与 backend skeleton 提为 PR（不改变现有行为，只重构以便后续迁移）。

如果你想快速验证 gpui：我可以先做 gpui minimal prototype 并提交 demo 分支。

请选择我现在先做哪一项，或直接批准我把 todo.md 提交（我已在本仓库创建此文件）。
