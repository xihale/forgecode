# Tree Mode 迁移可行性分析

## Objective

评估将 pi-mono 的 Tree Mode（会话树导航/分支功能）迁移到 ForgeCode 的可行性、所需改动范围和推荐实施路径。

## 核心对比

| 维度 | pi-mono | ForgeCode |
|------|---------|-----------|
| **消息存储** | JSONL 文件，每行一个 `SessionEntry`，通过 `id`/`parentId` 构建树 | SQLite 单表，整个 `Context`（含所有 messages）序列化为一个 JSON blob |
| **消息模型** | `Vec<SessionEntry>` 扁平列表 + `parentId` 链接 | `Vec<MessageEntry>` 纯线性列表，无任何 parent/child 字段 |
| **分支机制** | `branch(targetId)` 移动叶子指针，保留完整历史 | `clone` 复制整个 conversation，`retry` 重发最后一条 |
| **压缩** | `CompactionEntry` 作为树节点保留摘要 | 破坏性 splice，用摘要替换原始消息范围 |
| **UI 层** | 自定义 TUI 组件 `TreeList`（1000+ 行） | fzf 外部工具做 conversation 选择，无消息级导航 |
| **交互方式** | ZSH plugin + 自定义 keybindings | ZSH/Fish plugin + fzf + CLI commands |

## 关键发现

### 1. 数据模型差异是核心障碍

pi 的树结构建立在 `id`/`parentId` 字段上（`SessionEntryBase`），每条消息都有唯一 ID 和父节点引用。ForgeCode 的 `MessageEntry` 完全没有这些字段：

- `crates/forge_domain/src/context.rs:368-375` — `MessageEntry` 仅有 `message` + `usage`
- `crates/forge_domain/src/context.rs:294-321` — `TextMessage` 无 `id`、无 `parent_id`
- `crates/forge_repo/src/database/schema.rs:1-13` — 数据库无消息级存储

要支持树结构，必须给 `MessageEntry` 添加 `id` 和 `parent_id` 字段，或者采用完全不同的方案。

### 2. 存储格式差异

pi 使用 JSONL（每行一个 entry），天然支持 append-only 和树构建。ForgeCode 使用 SQLite + JSON blob，整个 context 作为一个字段存储。这意味着：

- **优点**：SQLite 事务保证一致性，查询 conversation 列表快
- **缺点**：无法做消息级查询/更新，每次修改都要重写整个 context blob
- **迁移影响**：需要设计消息 ID 的持久化和索引方案

### 3. 现有功能已覆盖部分场景

ForgeCode 已有的 `clone` + `retry` 组合可以实现类似效果：

- `:clone` → 复制当前 conversation，在新 ID 上继续（等价于"从当前点分支"）
- `:retry` → 重发最后一条消息（等价于"重试最后一个 assistant 回复"）
- `:conversation` → fzf 浏览切换 conversation（等价于"在不同分支间切换"）

但这缺少：
- **消息级导航**：无法查看/选择对话中的某条消息
- **原地分支**：clone 创建全新 conversation，不是同一 conversation 内的分支
- **树可视化**：无法看到分支拓扑结构
- **分支摘要**：切换分支时不生成被放弃路径的摘要

### 4. Compaction 与分支的冲突

ForgeCode 的 compaction 是**破坏性**的（`splice` 替换原始消息，`crates/forge_app/src/compact.rs:147-151`）。一旦压缩，原始消息永久丢失。而 pi 的 compaction 是**非破坏性**的——`CompactionEntry` 作为树节点保留，原始消息仍可通过树导航访问。

要让树模式与 compaction 共存，需要改为非破坏性压缩（保留原始消息，仅标记为已压缩）。

## 迁移方案评估

### 方案 A：轻量级 — "消息级 retry + 可视化"

**思路**：不修改数据模型，在现有 `clone`/`retry` 基础上增加消息级导航。

- [ ] 给 `MessageEntry` 添加 `id: Option<EntryId>` 字段（向后兼容，旧数据为 None）
- [ ] 新增 CLI 命令 `forge conversation tree <id>` 输出 ASCII 树
- [ ] 新增 `:tree` shell plugin 命令，用 fzf 做消息级选择
- [ ] 选择某条消息后，`clone` conversation 并 truncate 到该消息，实现"从该点分支"
- [ ] 不改 compaction 逻辑

**优点**：改动最小，不破坏现有数据模型，向后兼容
**缺点**：不是真正的树（每个分支是独立 conversation），无法在同一 conversation 内看多分支

### 方案 B：中等 — "Conversation 内分支"

**思路**：在 conversation 内部实现真正的分支。

- [ ] 给 `MessageEntry` 添加 `id: EntryId` + `parent_id: Option<EntryId>` 字段
- [ ] 修改 `Context.messages` 为 `Context.entries: Vec<MessageEntry>` 并保持线性存储
- [ ] 新增 `branch(target_id: EntryId)` 方法：truncate messages 到 target，保留被截断部分作为 "inactive branches"
- [ ] 新增 `branches: Vec<Branch>` 字段存储非活跃分支
- [ ] 改 compaction 为非破坏性（标记而非删除）
- [ ] 新增 `forge conversation tree` 命令渲染 ASCII 树
- [ ] 新增 `:tree` shell plugin 交互

**优点**：真正的分支体验，一个 conversation 内多分支
**缺点**：数据模型改动大，compaction 重写，context blob 膨胀（存多个分支）

### 方案 C：完整迁移 — "复制 pi 的 JSONL + 树模型"

**思路**：完全复制 pi 的存储和树模型。

- [ ] 新增 `session_entries` SQLite 表（每条消息一行，含 id/parent_id）
- [ ] 重写 conversation 加载/保存逻辑
- [ ] 实现完整的 `SessionManager.getTree()` 等价逻辑
- [ ] 实现自定义 TUI 组件（Rust 替代 TypeScript TreeList）
- [ ] 实现键盘导航、过滤、折叠、搜索、标签
- [ ] 实现非破坏性 compaction
- [ ] 实现分支摘要生成

**优点**：功能最完整，与 pi 体验一致
**缺点**：工作量巨大（估计 3000+ 行 Rust），需要重写存储层、UI 层、compaction

## 推荐方案

**推荐方案 A（轻量级）**，理由：

1. **投入产出比最高**：80% 的用户场景是"我想回到之前某条消息重新来过"，方案 A 用 `clone + truncate` 就能实现
2. **不破坏现有架构**：向后兼容，不需要重写 compaction 或存储层
3. **渐进式迭代**：先实现方案 A 验证用户需求，如果确实需要方案 B/C 再升级
4. **pi 的 TreeList 组件（1000+ 行 TypeScript）需要完全用 Rust 重写**，方案 A 可以用 fzf 做消息选择，避免重写 TUI

### 方案 A 实施步骤

- [ ] **Phase 1: 数据模型** — 给 `MessageEntry` 添加 `id: Option<EntryId>` 字段，自动为新消息生成 ID，旧消息保持 None
- [ ] **Phase 2: 消息级导航** — 新增 `forge conversation tree <id>` 命令，输出 ASCII 格式的消息列表（含消息摘要、角色、时间戳）
- [ ] **Phase 3: 分支操作** — 新增 `forge conversation branch <id> --at <entry_id>` 命令：clone conversation，truncate messages 到指定 entry
- [ ] **Phase 4: Shell Plugin** — 新增 `:tree` 命令，用 fzf 选择消息，选择后自动 branch
- [ ] **Phase 5: 可视化增强** — 在 tree 输出中标记当前活跃路径 vs 已被 truncate 的历史

## Verification Criteria

- [ ] `MessageEntry` 新增 `id` 字段后，现有测试全部通过（向后兼容）
- [ ] `forge conversation tree <id>` 能输出可读的消息树
- [ ] `forge conversation branch <id> --at <entry_id>` 能创建分支 conversation
- [ ] `:tree` shell 命令能交互式选择消息并分支
- [ ] compaction 后 tree 命令仍能正常工作

## Potential Risks and Mitigations

1. **Context blob 膨胀**：添加 `id` 字段会增加每条消息的 JSON 大小。Mitigation：使用短 ID（8字符 hex），影响 < 5%
2. **向后兼容**：旧 conversation 没有 entry ID。Mitigation：`id` 为 `Option<EntryId>`，tree 命令对无 ID 消息使用序号
3. **fzf 消息选择体验**：fzf 是行选择的，消息可能很长。Mitigation：tree 命令输出摘要（角色 + 前 80 字符），选择后再显示完整内容
4. **与 compaction 交互**：compaction 后原始消息丢失，tree 只能看到压缩后的。Mitigation：Phase 1 不处理，后续可改为非破坏性

## Alternative Approaches

1. **不迁移，增强现有 clone/retry 文档**：最小改动，通过文档引导用户用 `:clone` + `:retry` 组合实现类似效果
2. **Web UI 替代**：不实现终端内 tree，而是在 HTML export（`dump html`）中添加交互式树导航
3. **外部工具集成**：类似 pi 的 shell plugin 模式，用独立脚本实现 tree 导航，通过 forge CLI 交互
