# Tree Mode 迁移 — 方案 A + Branch Summary

## Objective

实现方案 A（轻量级消息级分支）+ 在分支时生成从回退点到当前位置的摘要，告知模型"从那个 point 到现在干了什么"。

## 核心流程

用户执行 `:tree` → fzf 展示消息列表 → 选择某条消息 → 系统执行：
1. **Clone** 当前 conversation（新 ID）
2. **截断** clone 到选中的消息位置
3. **生成 Branch Summary**：对被截断的消息范围生成 `ContextSummary`，作为系统消息注入到 clone 的 context 中
4. **切换**到 clone 的 conversation

这样模型在新分支上会收到一个 summary frame，知道"从你选中的那个 point 到原始对话的末尾，之前尝试过什么"。

## Implementation Plan

### Phase 1: 数据模型 — EntryId

- [ ] **新增 `EntryId` 类型** 在 `crates/forge_domain/src/context.rs`
  - 8 字符 hex ID，类似 pi 的设计
  - `EntryId::generate()` 生成随机 ID
  - `Serialize`/`Deserialize`/`Clone`/`Debug`/`PartialEq`

- [ ] **给 `MessageEntry` 添加 `id` 字段**
  - `id: Option<EntryId>` — 向后兼容，旧数据为 None
  - 新消息创建时自动生成 ID
  - 更新所有 `MessageEntry` 构造点

- [ ] **更新测试和快照**
  - `cargo insta test --accept` 接受快照变更

### Phase 2: Branch Summary 生成

- [ ] **新增 `branch_summary` 方法** 在 `crates/forge_app/src/compact.rs` 或新文件 `crates/forge_app/src/branch.rs`
  - 输入：`&Context` + `truncate_at: usize`（截断位置索引）
  - 提取 `context.messages[truncate_at+1..]` 作为被截断的范围
  - 对该范围生成 `ContextSummary::from()` 并通过 `SummaryTransformer` 转换
  - 用 `TemplateEngine::render("forge-partial-summary-frame.md", ...)` 渲染为文本
  - 返回 `String`（摘要文本）

- [ ] **新增 `forge-partial-branch-summary-frame.md` 模板** 在 `templates/`
  - 类似 `forge-partial-summary-frame.md` 但头部说明这是分支摘要
  - 包含：`This branch was forked from message N. The following summarizes what was attempted in the abandoned branch:`

### Phase 3: Branch 操作

- [ ] **新增 `forge conversation branch <id> --at <entry_index>` CLI 命令** 在 `crates/forge_main/src/cli.rs`
  - `--at` 参数为消息索引（从 tree 输出中获取）
  - 或者用 `--at-entry-id <EntryId>` 使用 entry ID

- [ ] **实现 branch 逻辑** 在 `crates/forge_main/src/ui.rs`
  - Clone conversation（复用现有 `on_clone_conversation` 逻辑）
  - Truncate clone 的 `context.messages` 到指定位置
  - 调用 `branch_summary()` 生成摘要
  - 将摘要作为一条 `ContextMessage::user()` 消息追加到截断后的 context 末尾
  - Upsert clone 并切换到它

### Phase 4: Tree 可视化

- [ ] **新增 `forge conversation tree <id>` CLI 命令**
  - 输出 ASCII 格式的消息列表，每条显示：索引、角色、内容摘要（前 80 字符）、时间戳
  - 标记当前活跃位置（最后一条消息）
  - 格式示例：
    ```
    [0] user: "Please help me implement auth..."
    [1] assistant: "I'll read the auth module..."
    [2] tool: read src/auth.rs
    [3] assistant: "The auth module needs..."
    [4] user: "Try approach B instead..."
    [5] assistant: "For approach B..."
    ← current (5 messages)
    ```

- [ ] **输出格式支持 porcelain 模式**
  - `--porcelain` 输出 TSV 格式供 fzf 消费
  - 列：index、role、summary、timestamp

### Phase 5: Shell Plugin `:tree` 命令

- [ ] **新增 `:tree` action** 在 `shell-plugin/lib/actions/tree.zsh`
  - 调用 `forge conversation tree <current_id> --porcelain` 获取消息列表
  - 用 fzf 展示，预览窗口显示完整消息内容
  - 选择后调用 `forge conversation branch <id> --at <index>`
  - 自动切换到新 conversation

### Phase 6: EntryId 自动生成

- [ ] **在消息创建时自动填充 EntryId**
  - 修改 `Context::add_message()` 等方法
  - 或者在 conversation 保存时批量补充缺失的 ID
  - 确保 `ContextMessage::user()` / `assistant()` 等构造器生成的消息在加入 context 时获得 ID

## Verification Criteria

- [ ] `MessageEntry` 新增 `id: Option<EntryId>` 后所有现有测试通过
- [ ] `forge conversation tree <id>` 输出可读的消息列表
- [ ] `forge conversation branch <id> --at <index>` 创建新 conversation，context 截断到指定位置
- [ ] 新 conversation 的 context 末尾包含 branch summary
- [ ] `:tree` shell 命令能交互式选择消息并分支
- [ ] Branch summary 准确反映被截断消息范围的操作（文件读写、shell 命令等）

## Potential Risks and Mitigations

1. **EntryId 向后兼容**：旧 conversation 没有 entry ID。Mitigation：`Option<EntryId>`，tree 命令用数组索引作为 fallback
2. **Branch summary 模板质量**：复用 compaction 的 summary 模板可能不够精确。Mitigation：新增专门的 branch summary 模板，开头说明这是"被放弃的分支"
3. **fzf 消息选择体验**：消息可能很长，fzf 行显示不完整。Mitigation：tree 命令输出截断摘要，preview 窗口显示完整内容
4. **截断位置语义**：截断到某条 user 消息 vs assistant 消息的语义不同。Mitigation：只允许选择 user 消息作为分支点（与 pi 行为一致）

## Alternative Approaches

1. **用 LLM 生成摘要而非模板**：调用 LLM 对截断范围生成自然语言摘要，质量更高但有延迟和成本
2. **不生成摘要，直接截断**：最简实现，但模型不知道之前尝试过什么，可能重复错误
3. **HTML 导出中添加树导航**：不实现终端内 tree，在 `dump html` 中添加交互式树
