# Zed crates 迁移与 YssBI 组织分析

本文保留 YssBI 对 Zed crates 的迁移、替换与职责划分评估，并记录按该文档完成的桌面职责重构。候选上游能力与已实施的本地重组分开说明，不把候选表视为已经接入的功能清单。

- 分析快照日期：2026-10-10；整理入库日期：2026-10-11。
- 输入材料：`zed-crates-guide-2026-10-10.md`（用户提供的 Zed crates 职责清单）。
- Zed 源码基线：[`96984225abc235519e66b5c6c9ed23b87621f868`](https://github.com/zed-industries/zed/tree/96984225abc235519e66b5c6c9ed23b87621f868)。该清单统计 `crates/` 下 249 个工作区成员，以及 `extensions/`、`tooling/` 下另 7 个成员。
- 评估不涉及版权；重点是职责适配、依赖、状态所有权、运行时与维护成本。
- 原分析数字来自重构前工作区，未绑定不可变提交；保留为基线，不是当前清单。下节分轮记录实施结果。YssBI 源码链接指向当前 `main` 路径，上游链接固定到上述提交。

本文位于 mdBook 正文之外，保留分析快照、候选顺序与实施记录。当前架构以[系统架构](../src/development/architecture.md)和模块契约为准；未实施的产品能力仍由仓库 [TODO](../../TODO.md) 管理。

## 第一轮：职责与生命周期拆分

已完成 P0/P1 所要求的职责与生命周期拆分，没有整包引入 Zed 的领域或 UI 系统。

- 薄入口只保留参数、进程 Tokio runtime、GPUI 初始化和最终窗口组装；项目、Dock、窗口协调与跨功能导航归 `yss-workbench`。
- 提取 Graph、Assistant、Data、Results、Settings、Resource、Plugin 七个功能 UI crate。建立共享呈现 `yss-ui` 与无 GPUI 的后台 Application 调用、事件层 `yss-desktop-services`，后者不是完整服务定位器。共享 UI 的业务依赖泄漏在第二轮继续收紧；当前 owner 见[架构表](../src/development/architecture.md#dependency-direction)。
- AssistantController 接管目录查询、会话强缓存、请求代次及会话操作；GraphDetails 接管图属性表单。设置和会话实体跨窗口隐藏保留，导入通过类型化事件提交；功能 crate 不依赖 Workbench。
- `yss-project-operation` 合入 Project 私有 `project_operation` 模块，保留准入、重放和 reservation 释放；`yss-display-naming` 合入 Application 私有 `display_naming` 模块，保留原名称分配语义。旧独立包和调用路径已移除。
- `yss-application::runtime` 仍是平台中立组合根。没有新增第二宿主或替换存储需求，因此不建立 `yss-bootstrap`；SCI Runtime 的真实报告适配、共享身份/布局/Schema 契约和 Store 查询边界保持原 owner。
- 新 crate 使用命名库入口。未对不涉及职责迁移的既有 `lib.rs` / `mod.rs` 做全仓机械重命名，也未修改 `.rules`。

| 检查项                       | 重构前 | 重构后 |
| ---------------------------- | -----: | -----: |
| Workspace packages           |     61 |     69 |
| 桌面入口直接内部依赖         |     29 |      2 |
| Application 直接内部依赖     |     45 |     44 |
| 桌面入口 `src` Rust 文件     |    346 |      1 |
| 桌面入口 `src` Rust 物理行数 | 65,122 |     86 |

增加 10 个职责明确的 crate、合并 2 个单消费者包；入口缩小来自功能迁移，不是删除原功能。第一轮结束时正常/构建内部依赖图有 324 条边、无环；入口只直接依赖 `yss-ui` 与 `yss-workbench` 两个内部包。全部 GUI crate 继承同一 `gpui-kit` 版本及 features；Project、Application、桌面任务与事件层的依赖闭包没有 GPUI。

第一轮验证：

- 原生入口 all-targets 编译检查、开发可执行文件构建通过；13 个受影响包的 all-targets Clippy（`--no-deps -- -D warnings`）和格式检查通过。
- 既有行为测试共 26 项通过：共享 UI 9、Data UI 1、Resource UI 7、Project 操作账本 5、Application 名称分配 4。
- 隔离目录中的 Linux/X11 原生窗口完成图执行、结果独立窗口租约交接、示例导入/末页分页、会话草稿隐藏重开、失败后重试另存为与项目切换、文档/Mind/图表保存、偏好及布局重启恢复。具体场景和未覆盖范围见[桌面集成验收](../../crates/yss-desktop-gpui/README.md)。

P2 的编码识别、JSONC 编辑、HTML 导入及统一取消桥接仍以真实需求为前提；现有 Tokio handle 被复用，但未引入一套新的自动 abort 语义。P3 的沙箱、MCP/ACP、IDE 编辑、分发等不是此次职责重构的交付项，也没有用占位实现宣称完成。未验证真实模型服务、签名插件执行、Windows/macOS 或完整物理输入行为。

## 第二轮：消除共享 UI 的业务依赖泄漏

第一轮拆包后，`yss-ui` 仍通过图诊断、节点目录行、资源目录和 `ActivityText` 翻译辅助函数依赖 Application、Project 及执行链。复用通用控件因此仍会带入完整业务栈；这不是继续增加 crate 能解决的问题。

- 将节点目录行移入 `yss-graph-ui::catalog_rows`，图诊断与运行失败文案移入 `yss-graph-ui::feedback`；画布、属性、问题面板、侧栏和 Workbench 输出迁移到同一实现。
- 将 GPUI 无关的只读 `ResourceCatalog` 移入既有 `yss-desktop-services::resources`。保留同一 `Arc` 分发、项目身份、资源/函数签名版本、排序和搜索，不新增查询或资源状态。
- `ActivityText::resolve_with` 由调用方提供翻译器，字面文本不翻译、不插值；`yss-ui::text` 不再导入 Application 类型。
- 删除旧模块、旧公共辅助函数和不再需要的依赖，不保留兼容转发，也不新增 crate。

| 正常/构建内部依赖指标 | 第二轮前 | 第二轮后 |
| --- | ---: | ---: |
| Workspace packages | 69 | 69 |
| `yss-ui` 直接依赖 | 14 | 5 |
| `yss-ui` 传递依赖（不含自身） | 57 | 5 |
| Workspace 内部依赖边 | 324 | 316 |

数据来自 `cargo metadata --no-deps --format-version 1 --offline`。依赖图仍无环，功能 UI 不反向依赖 Workbench；`yss-ui` 的内部闭包仅含 canonical-hash、data-contract、filesystem、i18n、settings。另用 `cargo tree -p yss-ui --edges normal,build --offline` 核查实际解析树，没有 Application、Project、Database Engine、DataFusion、Arrow、Rig 或 Plugin Runtime。这里证明的是编译依赖隔离，不是未经测量的构建耗时提升。

第二轮验证：

- `yss-ui` 独立 all-targets 检查通过；共享 UI 既有 9 项测试和 Application 字面文本绕过翻译/插值的回归测试通过。
- UI、Desktop Services、Graph UI、Assistant UI、Workbench、Application、桌面入口这 7 个受影响包的 all-targets Clippy（`--no-deps -- -D warnings`）通过，桌面可执行文件重新构建成功。
- 使用隔离应用数据和项目，重建后的 Linux/X11 原生窗口验证了中文节点分类/名称、`sqrt` 搜索与创建、缺失输入及未解析类型诊断，以及助手资源选择器中的当前图和 Markdown 文档名称、路径。未将禁用的运行按钮当成执行失败分支验证，也未调用真实模型服务。

Store 与 DataFusion 的耦合还涉及 `edits/rows`、`edits/columns` 的编辑准备和压实，不只是 `DatasetSnapshot::query`。本轮没有只移动查询方法便宣称存储与引擎已解耦，也未改变该边界。

## 结论

**YssBI 最值得向 Zed 学习的是薄应用入口、独立功能 crate 和明确的状态所有权，而不是整体搬入 `project`、`workspace`、`editor`、`agent`。**

1. 优先重组桌面功能边界：将已经成形的 Canvas、Assistant、数据库浏览、结果展示等功能逐步从 `yss-desktop-gpui` 提取出来。
2. 保留现有 BI 领域内核：Graph、Database、Execution、SCI、Node Kernel 没有可以从 Zed 等价替换的实现。
3. 少量基础组件可以移植：`file_content`、`settings_json`、`html_to_markdown` 等值得评估，但多数是补充能力，不是当前必须替换。
4. UI 继续统一到 GPUI Kit：不要同时引入 Zed 的 `ui/theme/workspace`，形成两套组件、主题和生命周期体系。
5. 不以 crate 数量或名称相似作为迁移依据：当前既有值得拆开的大功能包，也有不能随便合并的小契约包。

在已核查的候选中，没有发现值得无条件整包替换现有核心系统的 Zed crate。最有价值的替换面集中在编码、配置文本编辑和异步桥接等窄边界。

## 重构前组织与规则的差距

### 拆分粒度不均衡，而不是没有分层

分析时执行了以下命令，并据其输出分析 workspace 的内部依赖关系：

```sh
cargo metadata --no-deps --format-version 1 --offline
```

| 检查项                               | 分析快照结果 |
| ------------------------------------ | -----------: |
| Workspace packages                   |           61 |
| `yss-desktop-gpui` 直接内部依赖      |           29 |
| `yss-application` 直接内部依赖       |           45 |
| `yss-desktop-gpui/src` Rust 文件     |          346 |
| `yss-desktop-gpui/src` Rust 物理行数 |       65,122 |
| `yss-application/src` Rust 物理行数  |       42,314 |
| 使用 `lib.rs` 的库目标               |           58 |
| 使用命名入口的库目标                 |            2 |
| `crates/` 中的 `mod.rs`              |           75 |

直接内部依赖统计正常依赖和构建依赖，不含开发依赖，并按包名去重。行数包含测试、注释和空行，只用于判断组织规模，不代表业务复杂度或性能。

正常依赖和构建依赖组成的内部图没有环。分析快照中只有桌面 crate 直接依赖 GPUI Kit，平台中立业务层这一点已经落实，不应为了模仿 Zed 而破坏。无环也不等于所有语义分层都已经理想。

### 桌面层承担了最多跨功能协调

重构前的 [`Workbench`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-workbench/src/workbench.rs) 同时持有：

- 项目与最近项目；
- 插件管理及插件视图；
- 设置窗口；
- Assistant 会话和刷新状态；
- Dock 布局；
- Graph、Document、Mind、Database、Chart 的视图集合；
- Results、Problems、Logs、Output；
- UI intent 队列、布局恢复、项目关闭等状态。

作为窗口组合点，持有各功能句柄很正常；但分析时还直接承担不少功能自己的状态和流程。

建议把功能内状态与流程下沉到功能 owner，Workbench 保留窗口级协调。这不是单纯把大文件切成更多文件：当前已经有大量模块，缺少的是 Cargo 层面的功能边界。

### 区分组织目标与历史命名债务

- `mod.rs`、默认 `lib.rs` 与当前约定不一致，属于可整理的命名债务。
- 全部重命名不会自动改善状态所有权和依赖方向。
- `crates/.rules` 强调单一事实源、控制抽象，没有要求仿照 Zed 拆成几百个 crate。
- 根规则要求架构地图放文档，而不是不断塞进 `.rules`。

应先调整真实职责边界，再处理命名，而不是反过来。本文不修改 `.rules`，也不将候选结构转化为新的强制规则。

## GPUI 兼容性是迁移前提

分析快照中，项目使用：

```toml
gpui-kit = "=0.7.1"
```

安装的 GPUI Kit 实际将 `gpui-pre = 0.3.8` 作为底层 GPUI；Zed 基线使用其 workspace 中的 `gpui`。

**同属 GPUI，不等于可以直接混用。** Zed 的 `Entity<T>`、`App`、`Task<T>` 等类型，不能因为名称相同，就默认与当前 GPUI Kit 的对应类型兼容。引入依赖 Zed GPUI 的 crate，需要处理依赖身份、API 差异和平台后端一致性，而不只是修改 `use`。

应避免：

- 两套 GPUI 核心；
- 两套主题与控件；
- 两套窗口或面板状态；
- 两套 Tokio runtime；
- 将 Zed 的 GPUI 类型引入当前领域契约。

这也是许多候选被归为借鉴或局部移植，而不是直接依赖的原因。桌面依赖入口见 [`yss-desktop-gpui/Cargo.toml`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-desktop-gpui/Cargo.toml)。

## 值得移植的候选

本文的“可移植”指依赖和职责相对独立，值得做适配验证，不表示已经证明可以原样编译接入。低耦合也不等于可以原样复制 manifest：上游的 workspace 继承、内部依赖、功能开关和版本仍需处理。

### 相对独立、优先评估的组件

| Zed crate              | 对应 YssBI 位置                    | 建议                                                 | 关键条件                                             |
| ---------------------- | ---------------------------------- | ---------------------------------------------------- | ---------------------------------------------------- |
| `file_content`         | 外部文本导入、CSV 导入前处理       | 优先评估编码识别、二进制识别、流式转码               | 放在导入边界，不改变内部项目文件的既定 UTF-8 契约    |
| `settings_json`        | `yss-settings`、快捷键配置文件编辑 | 需要 JSONC、保留注释和格式时，局部替换 JSON 编辑逻辑 | 保留现有设置校验、原子提交和快照 owner               |
| `html_to_markdown`     | 未来网页资料或 HTML 知识源导入     | 可作为独立能力引入                                   | 不是 Markdown 渲染器的替代品                         |
| `path`                 | 相对路径类型、跨平台路径转换       | 按类型或算法摘取                                     | 不能替代当前根目录身份验证、安全访问和事务租约       |
| `gpui_tokio`           | 桌面异步任务桥接                   | 移植桥接设计或小段实现                               | 复用已有 Tokio handle，适配 GPUI Kit，不另建 runtime |
| `proxy_handshake`      | 未来自定义代理传输                 | 有明确需求再引入                                     | 若现有 HTTP 库已经满足代理需求，不再叠一层           |
| `shell_command_parser` | 未来 Agent shell 或终端工具        | 有明确需求再引入                                     | 当前原生插件启动不是 shell 工具；解析器也不等于沙箱  |

已经核查的依赖证据：

- [`file_content` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/file_content/Cargo.toml)：生产依赖只有 `anyhow`、`chardetng`、`encoding_rs`、`futures-lite`。
- [`settings_json` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/settings_json/Cargo.toml)：JSON 和 Tree-sitter 相关依赖，没有直接引入 Zed UI。
- [`html_to_markdown` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/html_to_markdown/Cargo.toml)：基于 HTML 解析库，职责相对独立。
- [`path` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/path/Cargo.toml)、[`proxy_handshake` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/proxy_handshake/Cargo.toml)、[`shell_command_parser` manifest](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/shell_command_parser/Cargo.toml)：用于判断这些组件的直接依赖边界，不代表已完成完整传递依赖验证。

### 文件解码应放在导入边界

当前 [`DocDocument`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-project-model/src/doc.rs) 按 UTF-8 解码；[CSV 读取](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-database-io/src/csv.rs)直接接 Arrow decoder，并有批次和采样字节预算。

可以在外部数据导入之前接入 `file_content` 的编码识别和流式转码，不需要替换整个 `yss-database-io`。

但[上游部分解码路径](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/file_content/src/file_content.rs)没有向调用方报告编码错误标志。对于统计数据，静默替换字符可能改变列名或分类值。移植时必须决定：

- 自动检测与用户显式选择的优先级；
- 非法字节是拒绝、警告还是替换；
- 转码后的字节预算；
- 是否保留原始编码与 BOM 信息。

文本编辑器能接受的容错，不一定适合数据分析导入。

### Tokio 桥接应复用运行时并区分取消语义

[`gpui_tokio` 的实现](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/gpui_tokio/src/gpui_tokio.rs)将 Tokio 任务包装成 GPUI Task，后者被丢弃时调用 abort；同时提供 `init_from_handle`。

重构前已有 `NativeServices.executor` 和 `spawn_blocking`。当前保留[工作台私有组合层](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-workbench/src/services.rs)，共用后台入口位于 [`ApplicationTasks`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-desktop-services/src/desktop_services.rs)；继续复用原运行时，而不是再建一个。

必须区分：

- 可取消的读取、搜索和预览；
- 已进入提交阶段的写操作；
- 已经开始执行的 `spawn_blocking` 工作。

不能把视图关闭统一解释为业务写入已经取消。当前 [Gateway](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-application/src/harness/gateway.rs) 要求写操作等待真实回执，应保留这个约束。

### 能移植，但尚无足够理由替换的组件

| 候选                    | 可以解决的问题               | 当前决策                                                           |
| ----------------------- | ---------------------------- | ------------------------------------------------------------------ |
| `rope`、`sum_tree`      | 大文本增量编辑、增量摘要索引 | 等明确性能证据；只换容器而保留全文传输契约，收益有限               |
| `streaming_diff`        | 流式文本修改的差异展示       | 有流式编辑审阅需求再引入，不替代 Graph history                     |
| `watch`                 | 保留最新快照的通知通道       | 不是文件监控；不能替换需要完整顺序的 Harness 事件流                |
| `scheduler`             | 优先级执行器、可控调度       | 不替代业务 DAG 调度，没有已证明的运行时替换需求                    |
| `sqlez`                 | SQLite 访问、串行写队列      | 可替换某个 SQLite adapter 的内部访问层，但目前 SQLx 已提供所需底座 |
| `fuzzy`、`fuzzy_nucleo` | 模糊排序和匹配高亮           | 两者有 GPUI 耦合；优先考虑中立 matcher，而不是整包迁入             |
| `sandbox`               | 受限进程执行                 | 安全能力新增候选，需要单独设计和平台验证                           |

依赖与实现入口：[`rope`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/rope/Cargo.toml)、[`sum_tree`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/sum_tree/Cargo.toml)、[`streaming_diff`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/streaming_diff/Cargo.toml)、[`watch`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/watch/Cargo.toml)、[`scheduler`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/scheduler/Cargo.toml)、[`sqlez`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/sqlez/Cargo.toml)、[`fuzzy`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/fuzzy/Cargo.toml)、[`fuzzy_nucleo`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/fuzzy_nucleo/Cargo.toml)。

搜索不能丢掉现有中文语义。`yss-node-catalog/src/search.rs` 已有 Unicode 归一化、大小写折叠、别名、技术词、拼音和首字母扩展。模糊匹配只能接在这些候选文本之后，不应把它们替换掉。

沙箱不能只看 crate 名称。当前插件 runtime 明确只接收 `TrustedNative`，拒绝 `SandboxRequired`，不存在一个已经实现、等待替换的完整沙箱。Zed 的实现涉及 Linux `bwrap`、macOS `sandbox-exec`、Windows WSL 等平台条件；其部分限制策略允许广泛读取、仅限制写入，不能直接解释为 BI 数据隐私隔离。详见 [sandbox 实现与策略](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/sandbox/src/sandbox.rs)。

## 名称相似但不适合整体替换的组件

| Zed 候选                               | YssBI 对应内容                  | 判断                                                                 |
| -------------------------------------- | ------------------------------- | -------------------------------------------------------------------- |
| `ui`、`theme`、`ui_input`、`ui_prompt` | GPUI Kit 组件、主题、输入和弹层 | 不引入第二套体系                                                     |
| `workspace`                            | Workbench、DockArea、布局持久化 | 借鉴功能组织，不替换现有 Dock owner                                  |
| `project`、`worktree`                  | `yss-project`、资源索引         | IDE 文件或语言项目模型，不是 BI 项目 authority                       |
| `fs`                                   | `yss-filesystem`                | 上游依赖 GPUI、Git、text、proto 等；不等价于当前事务与身份验证契约   |
| `settings` 整套                        | `yss-settings`                  | 上游绑定 GPUI、fs、产品设置及迁移；当前中立设置 owner 更符合项目要求 |
| `editor`、`language`、`multi_buffer`   | Markdown、SQL 或代码输入        | 先复用 GPUI Kit Editor；除非产品明确要做 IDE 级编辑能力              |
| `markdown`、`markdown_preview`         | 共享 Markdown 与文档预览        | 已有 TextView 加数学、链接等扩展，不宜换成 Zed 编辑器依赖链          |
| `tabular_data_preview`                 | Database 或 Results 表格        | 编辑器文本驱动的表格预览，不是 Arrow/DataFusion 数据浏览与编辑引擎   |
| `task`                                 | Graph Execution、Workflow       | Shell 或终端任务定义，不是数据流求值器                               |
| `db`                                   | Database、Harness、项目注册存储 | Zed 应用数据库框架，不是通用 BI 数据库领域                           |
| `zlog`、`telemetry`                    | `yss-logging`                   | 不能等价替换脱敏、SQLite 历史、有序交付和保留策略                    |
| `extension*`                           | 原生插件协议、SDK、runtime      | Zed WIT/WASM 扩展模型与当前签名原生进程协议不同                      |

几个已核查的依赖事实：

- [`workspace`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/workspace/Cargo.toml) 直接依赖 `project`、`language`、`remote`、`client`、`settings`、`ui` 等。
- [`fs`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/fs/Cargo.toml) 直接依赖 `gpui`、`git`、`rope`、`text`、`proto`、`telemetry`。
- [`settings`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/settings/Cargo.toml) 包含 GPUI、产品设置和迁移依赖，不应整体接入当前平台中立设置层。
- [`markdown`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/markdown/Cargo.toml) 直接依赖 `language`、`settings`、`theme`、`ui`、`mermaid_render`。
- [`tabular_data_preview`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/tabular_data_preview/Cargo.toml) 依赖 `editor`、`workspace`、`text`、`ui`，并非独立数据表格引擎。

名字通用不代表工程上独立。SQLx、Arrow、DataFusion、Parquet 的数据接入和计算能力，不能因为 Zed 存在 `db` 或表格预览就被替换。Zed 的 `util`、`collections` 也不应仅为统一命名整包引入；已有成熟依赖和明确领域模块优先。

## AI 与插件的迁移边界

### 不用 Zed Agent 替换整个 Harness

当前分层包括：

- `yss-harness-contract`：平台中立契约；
- `yss-harness-core`：会话、编排、授权与执行事实；
- `yss-harness-rig`：真实模型和工具循环适配；
- `yss-harness-sqlite`：持久化；
- `yss-harness-tantivy`：知识检索投影。

现有 Rig adapter 已接入 OpenAI Responses/Chat、Anthropic、Gemini，不是等待填充的空框架。Zed [`agent`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/agent/Cargo.toml) 围绕代码项目、文本缓冲区、文件改动、终端和上下文服务器组织。

整体替换会触及 YssBI 的领域不变量：

- 模型不能自行生成统计事实；
- Project、Database、Execution 继续拥有业务权威；
- 工具基于真实读取版本提交；
- 已提交写入不能因超时被判为未执行；
- 会话回放不能重新执行已完成操作；
- 结果引用和报告必须关联真实成果。

Zed Agent 的成熟度不等于其领域模型适合统计工作流。现有契约见 [Harness Core](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-harness-core/README.md) 和 [Rig adapter](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-harness-rig/README.md)。

### 借鉴 AI 子能力，不建立第二套 owner

- `agent_skills`：借鉴目录发现、元数据解析、来源管理；保留 YssBI 精确版本和内容 hash 规则，不照搬同名覆盖优先级。
- 模型 provider crates：有具体 Rig 协议缺口时，摘取请求、事件或错误处理逻辑；不同时建立第二套模型注册中心。
- `language_model_core`、`language_model`、`language_models`：可以研究类型与 provider 分层；不能仅凭 core 命名判断为中立叶子包。后两者包含 GPUI 执行器或产品注册依赖，不整体替换现有 Contract/Rig 边界。
- `action_log`：借鉴变更审阅交互，不替换现有工具账本和 Project history。
- `prompt_store`：只有需要用户可编辑提示词库时再研究；原始 Markdown 加载不需要整套存储框架。
- `context_server`、`agent_servers`、`acp_thread`：属于未来 MCP/ACP 接入，不是现有进程内 Gateway 的替代品，不通过 loopback MCP 重建内部调用链。
- `credentials_provider`：当前已有中立凭据接口和 keyring；上游接口直接暴露 GPUI 上下文，没有替换收益。
- `agent_ui`：可参考交互，不将持久会话 owner 移入 UI，也不整包引入其 editor/workspace 依赖。

关键上游证据：[`language_model`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/language_model/Cargo.toml)、[`language_models`](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/language_models/Cargo.toml)、[`agent_skills` 来源优先级](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/agent_skills/agent_skills.rs)、[`credentials_provider` 接口](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/credentials_provider/src/credentials_provider.rs)。

### 不用 Zed 扩展系统直接替换原生计算插件

当前插件体系处理签名包、独立进程、IPC、任务授权、快照租约、结果回执与 provenance。

Zed 的 `extension_api/extension_host` 是另一套宿主能力与运行模型。采用它意味着选择新的插件执行模式，不是替换 SDK 就能完成。

可以学习清单验证、能力授权和安装生命周期，但应保留当前业务协议与成果提交边界。参考 [YssBI Plugin Runtime](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-plugin-runtime/README.md) 与 [Zed extension_host 依赖](https://github.com/zed-industries/zed/blob/96984225abc235519e66b5c6c9ed23b87621f868/crates/extension_host/Cargo.toml)。

## YssBI crates 的重组建议

### 优先提取 UI 功能，而不是重建领域层

下表保留原建议的职责边界和候选名称；本次实际落地以文首实施记录及当前架构文档为准。

| 候选目标                          | 提取来源                             | 应拥有的内容                                   |
| --------------------------------- | ------------------------------------ | ---------------------------------------------- |
| `yss-desktop-gpui`                | 保留入口                             | 启动、平台集成、最终组装                       |
| `yss-workbench`                   | `workbench*`、窗口布局相关模块       | 窗口协调、Dock、面板注册、布局恢复、跨功能导航 |
| `yss-graph-ui`                    | `canvas*` 及图专属 Details、创建交互 | 画布交互、图视图状态、节点编辑呈现             |
| `yss-assistant-ui`                | `assistant*` 及会话相关工作台逻辑    | 消息、输入、工具状态、引用与会话展示           |
| `yss-data-ui`                     | `databases*`、相关导入界面           | 数据源与数据集浏览、编辑交互                   |
| `yss-results-ui`                  | `results*`、相关图表呈现             | 结果阅读、分页、报告与可视化                   |
| 后续按需要提取设置、文档、插件 UI | 已有对应功能模块                     | 各自输入、订阅和生命周期                       |

提取时必须保持：

1. 功能 crate 不依赖 Workbench 的具体类型。
2. 不把 `NativeServices` 原样变成所有功能共用的万能接口。
3. 跨功能通过明确事件、命令和稳定数据通信，不访问兄弟视图内部字段。
4. 领域状态仍留在原 owner，UI 拆包不意味着复制业务模型。

GPUI Kit 已提供的 Button、Input、Dock、Tree、Table、通知等，不再包装成一套仿 Zed 基础 UI。只有真正多处复用的产品组件才考虑共享 crate。

### Application 组合根按需要提取

[`yss-application/src/runtime.rs`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-application/src/runtime.rs) 已明确负责具体 adapters 的组装。用例与组装在同一个 crate，是设计选择，不等于已经发生错误反向依赖。

如果后续确实需要更轻的应用用例测试、CLI 或后台等第二宿主、可替换的持久化或模型组合，或者减少消费者强制加载的实现依赖，再考虑把 runtime composition 提取到 `yss-bootstrap` 一类独立 crate。

不要一边拆 UI，一边同时重写全部应用契约和组装方式。

### 小 crate 按依赖用途判断，不按行数合并

应保留的典型边界：

- `yss-project-identity`、`yss-project-layout`：多个低层消费者共享。
- `yss-database-schema`：轻量展示投影，不迫使消费者依赖 Arrow 实现。
- `yss-graph-document-edit`：Project 可以复用文档操作而不依赖完整语义运行时。
- `yss-node-catalog` 与 `yss-node-kernel`：可发现性和可执行实现不是同一职责。
- `yss-sci-contract`、`yss-sci`、`yss-sci-linalg`：科学契约、算法、线代后端分离有价值。

值得讨论的合并候选：

- `yss-project-operation`：分析快照中的正常依赖消费者只有 Project；如果没有独立隔离价值，可回归 Project 内部模块。会话绑定、防重和 reservation 释放语义必须保留。
- `yss-display-naming`：分析快照中的正常依赖消费者只有 Application，可评估是否值得独立包。
- `yss-sci-runtime`：存在薄转发，也存在真实报告适配；只能逐项评估，不能直接判定整个 crate 冗余。

### 审视 Store 到具体查询引擎的耦合

分析快照中存在以下依赖：

```text
yss-project -> yss-database-store -> yss-database-engine -> DataFusion
```

[`DatasetSnapshot::query`](https://github.com/zhouyi207/YssBI/blob/main/crates/yss-database-store/src/lib.rs) 显式接收 `DataFusionRuntime`。如果目标是让项目管理不承载查询引擎依赖，可以研究把查询适配移到 Database Runtime。

这只是有证据的优化候选，不是已证明的架构故障。还要检查 Store 的其他 engine 使用，不能声称移动一个方法就能切断全部依赖。

## 推荐实施顺序

以下保留原候选工作的依赖顺序。P0/P1 和已确认的单消费者小包合并已经实施；有条件的 P2/P3 不因重构请求自动转为新增产品能力。

### P0：确定目标架构，不引入 Zed 大包

- 保留平台中立领域层和 GPUI Kit 单一 UI 技术栈。
- 明确各桌面功能的状态 owner。
- 以依赖关系和实际生命周期为拆分依据。
- `.rules` 保留约束，架构地图放现有开发文档。

### P1：完成一个 UI 功能提取

建议先选 Assistant UI 或设置 UI，而不是最复杂的 Graph Canvas。

验收重点不是编译成功，而是关闭和重开、项目切换、未提交输入保留、订阅与任务释放、晚到结果拒绝，以及 Workbench 不再持有被提取功能的内部流程状态。

形成清晰边界后，再提取 Graph、Data、Results。

### P2：按真实需求接入小型上游能力

1. 外部文本或 CSV 编码需求：评估 `file_content`。
2. 可手工维护配置、保留注释：评估 `settings_json`。
3. 网页资料导入：评估 `html_to_markdown`。
4. 桌面后台任务生命周期需要统一：借鉴 `gpui_tokio`。

每次只替换一个明确接缝，完成后删除旧路径，不长期并存。没有真实需求的候选不为补齐目录而引入。

### P3：单独评估高成本能力

- 原生插件沙箱；
- MCP/ACP；
- IDE 级编辑；
- 大文本增量模型；
- SQLite 底座替换；
- 自动更新与平台分发。

Git、终端、LSP、调试器、远程协作、音视频、Zed 云服务等，没有足够的产品需求支撑整包引入。更新、诊断和性能工具也应按 YssBI 的平台与分发方式选择，而不是跟随 Zed 目录补齐。

## 证据范围与后续验证

原分析只读检查了当前工作区、关键模块契约、上游 manifest 和部分实现，并执行 workspace metadata 检查与内部依赖图分析，没有修改业务代码或规则。

原分析阶段没有编译移植样例，也没有运行桌面、真实模型或沙箱。本次本地职责重构的编译、行为测试与桌面 smoke 单独记录在文首，不能用于证明任意上游候选的兼容性或其平台支持。

选择实际迁移项时，需要验证其具体接缝：依赖身份和功能开关、输入输出与错误语义、取消和提交边界、内存与结果预算、平台支持，以及原 owner 的不变量。桌面功能提取需要操作实际界面；编码、配置编辑等能力需要覆盖真实输入和失败路径。

先整理 YssBI 自己的桌面功能边界，再有选择地移植 Zed 的独立能力，是本次评估的推荐方向。
