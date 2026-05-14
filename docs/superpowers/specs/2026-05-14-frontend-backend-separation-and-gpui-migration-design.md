# 前后端分离 & gpui 迁移设计

**日期**：2026-05-14
**状态**：草稿，待作者审阅
**取代**：根目录 `todo.md` 中的「直接结论」与「高层迁移路径」段落

---

## 1. 背景与动机

### 网站背景

这个 spec 的方案不以用户数量为优化目标。主要给自己用，朋友同学想用就用。相应地，方案的 rigor 上限只对到「作者自己用得顺」为止，不为产品级的健壮性 / CI 矩阵 / telemetry 等付出基础设施成本。

### 当前痛点

- `src/main.rs` 已经 2455 行，混合了 egui 渲染、字体配置、输入翻译、选择/复制、attachment 点击、窗口尺寸等多个职责。继续在上面加功能或迁移 UI 都会很痛。
- **草稿丢失**：BBS 内置编辑器在提交失败时会一并丢弃用户已输入的内容。这是客户端工作流问题，本地化编辑能彻底避免。

### 最终方向

迁移到基于 Zed 的 gpui，**为的是卸掉手搓的渲染、编辑、中英文等宽逻辑**，让 Zed 已经验证过的实现来承担这些。egui 版本会作为过渡期的安全网保留。

**更长远（不在本 spec 范围内）**：把帖子编辑搬到本地完成（gpui 自带编辑器，或外接 Emacs），发表前在本地保留原文，提交失败时不丢草稿。本 spec 不为这些功能做提前设计，但它们是 gpui 路线相对于「继续优化 egui」的额外吸引力。

## 2. 范围与非目标

### 在范围内
- 在**同一 crate 内**拆分前后端职责，把 `main.rs` 瘦下来
- 引入最薄的内部边界（`Backend` 结构 / 接口），让任何前端都能消费同一个后端
- 实现 gpui 前端作为可选 feature，并在 macOS 上验证它能持平或超过当前 egui 体验
- 视情况把 macOS 默认前端切到 gpui

### 明确不做
- **不**拆 workspace 多 crate（4600 行单站点客户端不需要）
- **不**给 TerminalSnapshot 加 serde
- **不**做 IPC / WebSocket / 远程前端协议
- **不**做 Emacs / 浏览器前端（README 的 Future Ideas，目前没有真实需求）
- **不**做 CI matrix 扩容、跨平台自动化回归、telemetry、性能 baseline 对照
- **不**强制 Windows 同步迁移到 gpui——见 §6
- **不**做 xterm.js 路线

## 3. 三阶段计划

### Phase 1 — 后端边界 & main.rs 瘦身（不引入新 UI 框架）

**为什么 P1 不引入 gpui**：故意把「结构重构」与「换 UI 框架」分两步，让两步的爆炸半径独立。P1 出回归 → 一定是重构问题；P2 出问题 → 一定是 gpui 问题。混在一起 bisect 会非常痛。所以 P1 只搬代码、不换技术。

把现有源码按职责重新组织，不改变运行行为：

```
src/
  main.rs              # 启动：解析 config、构造 Backend、运行 egui 前端
  app.rs               # App 状态、事件循环 glue
  backend/
    mod.rs             # pub use; Backend 结构
    ssh.rs             # 从 src/ssh.rs 移入
    terminal.rs        # 从 src/terminal.rs 移入
    ansi_parser.rs     # 从 src/ansi_parser.rs 移入
    cell.rs            # 从 src/cell.rs 移入
    attachment.rs      # 从 src/attachment.rs 移入
    snapshot.rs        # TerminalSnapshot 类型
    input.rs           # InputEvent / KeyEvent / MouseEvent 类型
    keys.rs            # Welly 键盘转义表（Alt-Up → \x1b[5~ 等）
  ui/
    egui/
      mod.rs           # EguiFrontend：拥有 egui app state，run() 进入 eframe 主循环
      render.rs        # cell / box art / cursor / status 渲染
      input.rs         # egui 事件 → InputEvent 翻译
      selection.rs     # 选择 / 复制 / 双击 URL 状态
      fonts.rs         # 字体加载与命名
  config.rs            # 保留
```

**`Backend` 暴露的方法**（API 表面，不是 trait）：
```rust
impl Backend {
    pub fn new(config: ConnectionSettings, notify: Arc<dyn Fn() + Send + Sync>) -> Self;
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&TerminalSnapshot<'_>) -> R) -> R;
    pub fn send_input(&self, event: InputEvent);
    pub fn subscribe_changes(&self) -> tokio::sync::watch::Receiver<()>;
    pub fn reconnect(&self);
    pub fn shutdown(&self);
}
```

**关于 `with_snapshot`**：`TerminalSnapshot<'a>` 借用 `Terminal` 内部的行数据，而 `Terminal` 在 Backend 内部锁后面。安全 Rust 不允许 Backend 「返回一个绑定到自己锁 guard 的借用」（自引用），所以 snapshot 用闭包式 API：调用方在闭包内拿到 `&TerminalSnapshot`，闭包返回时锁释放。这是为了零 clone 渲染又不引入 unsafe / `ouroboros` 的折中。

**`TerminalSnapshot` 是「渲染前」的只读数据结构**——它是 Backend 暴露给 Frontend 的「终端当前状态」视图（行、cell、光标、标题、attachment）。Frontend 在 `with_snapshot` 闭包里读它、画出像素；Backend 不知道它会被怎么画。它本身不是渲染结果，也不脱离当前进程（不加 `Serialize`）。

**`subscribe_changes` 的实现说明**：内部以推送回调（`notify()`）为底层机制——SSH 读循环解析新数据、连接状态变化、重连清屏、shutdown 等会影响前端显示或连接状态的事件都会调用 `notify`。`subscribe_changes()` 返回的 `watch::Receiver` 由 Backend 在 `notify` 内同时 ping 一次，供 pull 风格的消费者（如未来 gpui）使用。egui 路线只用 notify（它就是 `egui::Context::request_repaint`），不消费 watch。它不是完整状态日志，只是“有变化，请重读 snapshot / 状态”的轻量通知。

**没有 `Frontend` trait**——egui 和 gpui 的驱动模型差异太大（immediate vs retained），强造抽象只会变成性能秀。每个前端是自己的模块，消费同一个 `Backend`。

**验收标准**：
- `cargo run` 行为与重构前像素级一致（同样的 BBS 屏幕显示、键盘、选择、复制、attachment 点击）
- `src/main.rs` 不超过 200 行
- 所有现有测试通过

### Phase 2 — gpui 前端原型（feature gated，与 egui 并存）

新增：
```
src/ui/gpui/
  mod.rs    # GpuiFrontend：拥有 gpui app state，run() 进入 gpui 主循环
  render.rs # 复用 box art 几何参数，gpui 直接画
  input.rs  # gpui 事件 → InputEvent
  fonts.rs  # 使用 gpui 自带字体栈
```

Cargo 改动：
```toml
[features]
default = ["egui-frontend"]
egui-frontend = ["dep:eframe", "dep:egui"]
gpui-frontend = ["dep:gpui"]   # Zed 1.1.8 对应版本
```

`main.rs` 根据 feature 选择前端入口。

**验证清单（对照原版 Welly 截图，不对照当前 egui）**：
1. CJK 字符 + 双宽 cell 等宽对齐
2. macOS IME（中文候选框位置、组合状态显示）
3. Welly box art 粗细 / 对齐持平或更好
4. 选择 → 复制 → 双击 URL 跳转
5. Welly 风格的键盘转义（`\x1b[A/B/C/D`、`\x1b[1~/4~/5~/6~`）与鼠标导航

任何一项 gpui 上明显更差且短期无解 → 停在 Phase 2，不继续。

### Phase 3 — 在 macOS 上把默认切到 gpui

- macOS 的 `default` feature 改为 `gpui-frontend`
- egui 路线保留一个 release 作 `--legacy` 兜底，下下个版本可考虑删
- **Windows / Linux 暂时保持 egui**，不强制迁移；gpui 在这些平台的成熟度不可控
- 长期接受「macOS 跑 gpui、其他平台跑 egui」的并存状态

## 4. 数据流

```
SSH bytes ──► ansi_parser ──► terminal (state) ──► snapshot view
                                                        │
                                                        ▼
                                                  Frontend renders
                                                        │
User input ──► Frontend ──► InputEvent ──► Backend.send_input ──► SSH
```

关键不变量：
- Backend 不知道 UI 框架
- Frontend 只读 snapshot、只写 InputEvent
- snapshot 通过借用共享，渲染过程不 clone 整张屏幕

## 5. 暴露的核心类型

```rust
// src/backend/snapshot.rs

pub struct TerminalSnapshot<'a> {
    pub width: usize,        // 80
    pub height: usize,       // 24
    pub rows: &'a [Row],     // 行借用
    pub cursor: Option<(usize, usize)>,
    pub title: Option<&'a str>,
    pub attachments: &'a [AttachmentLink],
}

pub enum InputEvent {
    Key(KeyEvent),           // 键 + 修饰键（Up/Down/Alt+Arrow/Ctrl+...），UI 框架无关
    Mouse(MouseEvent),       // BBS 鼠标导航（滚轮、左缘后退、右半区翻页）
    Paste(String),
    Resize { cols: u16, rows: u16 },
    Reconnect,
    Shutdown,
}
```

**关键归属**：Welly 风格的键盘转义表（Alt-Up → `\x1b[5~` 等）归 Backend 所有，放在 `src/backend/keys.rs`。Frontend 只产 UI 中性的 `KeyEvent`，由 Backend 翻译成发往 SSH 的字节。这样新增 gpui 前端时不必复制转义表。

## 6. 平台决策

| 平台 | Phase 1 后 | Phase 3 目标 |
|------|-----------|-------------|
| macOS | egui | gpui |
| Windows | egui | egui（暂时保持不变） |
| Linux | egui（如适用） | egui（暂时保持不变） |

理由：这是 mac-first 的迁移，Windows 当前 egui 工作良好，没有切换驱动力。

## 7. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|-----|------|
| Phase 1 重构引入回归 | BBS 显示异常 / 输入卡顿 | 重构期间频繁 `cargo run` 对比 Welly 截图；保留每个文件移动作为单独 commit 便于 bisect |
| gpui macOS IME 不达标 | Phase 2 失败 | 验证清单第 2 条卡住即停，不勉强 |
| gpui API 在 Zed 1.1.8 之后变化 | 维护成本 | 锁定一个版本，不追新；这不是商业项目 |
| Phase 1 的 `Backend` API 设计错 | Phase 2 需要回头改 | 接受这种返工——一旦 gpui 前端动起来，再调整 API 是 Phase 2 的一部分 |

## 8. 已知不做的事

- ~~Cargo workspace + 三 crate（welly-core / welly-backend / welly-egui / welly-gpui）~~
- ~~`TerminalSnapshot` 加 serde 支持~~
- ~~tokio::sync::watch 与 crossbeam-channel 的二选一对比~~（默认 watch，不纠结）
- ~~本地 WebSocket / Unix socket / JSON 协议~~
- ~~xterm.js 备选~~
- ~~CI matrix 加 gpui build job + 跨平台打包脚本扩容~~
- ~~性能监测 / telemetry / fps 调试界面~~
- ~~e2e 测试 harness + ANSI-heavy workload 自动化~~
- ~~beta 发布 / 收集反馈 / 默认切换流程~~

这些不是「以后再说」，是这个项目**不会有**。除非将来项目性质变了。

## 9. 开放问题

无。所有取舍已在上文中明示。

## 10. 下一步

- 作者审阅本 spec
- 通过后调用 writing-plans 出 Phase 1 的详细实施计划（不要一次性出三阶段——Phase 2/3 等 Phase 1 落地后再 plan）
