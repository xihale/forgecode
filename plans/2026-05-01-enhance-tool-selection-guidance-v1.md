# 增强模型工具选择：防止模型通过 shell 调用 find/grep

## Objective

模型经常通过 `shell` 工具调用 `find`、`grep` 等传统 Unix 命令，而不是使用内置的 `fs_search` 工具。需要分析原因并制定改进方案，确保模型优先使用专用工具。

## 现状分析

### 已有的引导机制

项目中**已经存在**多层引导机制，但效果不够理想：

1. **`fs_search` 工具描述** (`crates/forge_domain/src/tools/descriptions/fs_search.md:4`)
   > `ALWAYS use fs_search for search tasks. NEVER invoke grep or rg as a Bash command.`

2. **`shell` 工具描述** (`crates/forge_domain/src/tools/descriptions/shell.md:6`)
   > `Do NOT use find/grep/cat/head/tail/sed/awk/echo — use dedicated tools instead.`

3. **forge agent 系统提示** (`crates/forge_repo/src/agents/forge.md:48`)
   > `Use specialized tools instead of shell commands for file operations.`

4. **已有评估基准** (`benchmarks/evals/search_over_find/task.yml`, `benchmarks/evals/read_over_cat/task.yml`)
   专门测试模型是否选择正确工具

### 问题根因分析

**引导力度不足**：当前 shell.md 仅 8 行，"Do NOT use" 只是一行列表中的条目，容易被模型忽略。相比之下 pi 的 shell 工具描述有 39 行详细指导，明确列出了每个命令的替代方案。

**关键对比**：

| 维度 | ForgeCode 当前 shell.md | pi shell 描述（参考） |
|------|------------------------|----------------------|
| 长度 | 8 行 | ~39 行 |
| 替代表格 | 无 | 每个禁用命令都有明确替代 |
| 理由说明 | 无 | 解释了为什么（权限优化、.gitignore） |
| 语气 | 简单列表 | CRITICAL/IMPORTANT 强调 |

## Implementation Plan

### 阶段一：增强 shell 工具描述（高优先级）

- [ ] **1.1 重写 `shell.md` 工具描述**，将禁用命令列表扩展为带替代方案的详细指导

  当前文件：`crates/forge_domain/src/tools/descriptions/shell.md`（8 行）
  
  需要增加：
  - 将禁用命令列表从一行拆分为独立的详细条目
  - 每个禁用命令明确指向替代工具（`find` → `fs_search`, `grep` → `fs_search`, `cat` → `read` 等）
  - 添加简短理由（如 "has been optimized for correct permissions and access"）
  - 保留现有的 `cwd`、`&&`、输出截断等指导

  参考 pi 的 shell 描述格式，但适配 ForgeCode 的工具名（使用 `{{tool_names.fs_search}}` 模板变量）。

### 阶段二：增强 forge agent 系统提示（中优先级）

- [ ] **2.1 增强 `forge.md` 的 Tool Selection 部分**

  当前文件：`crates/forge_repo/src/agents/forge.md:41-48`
  
  当前的 Tool Selection 部分只有 6 行，且只有一行提到"Use specialized tools"。需要：
  - 添加明确的"DO NOT"列表，与 shell.md 一致
  - 增加文件发现场景的指导（"需要查找文件时用 `fs_search` 的 `glob` 参数"）
  - 增加内容搜索场景的指导

### 阶段三：运行评估验证（必须）

- [ ] **3.1 运行 `search_over_find` 评估基准**

  已有基准：`benchmarks/evals/search_over_find/task.yml`
  验证模型是否在文件发现任务中使用 `fs_search` 而非 `find`。

- [ ] **3.2 运行 `read_over_cat` 评估基准**

  已有基准：`benchmarks/evals/read_over_cat/task.yml`
  验证模型是否在文件读取任务中使用 `read` 而非 `cat`。

## Verification Criteria

- `search_over_find` 评估通过：模型不再通过 shell 调用 `find`
- `read_over_cat` 评估通过：模型不再通过 shell 调用 `cat`
- 所有现有测试继续通过（`cargo insta test --accept`）
- shell.md 和 forge.md 的修改不引入未渲染的模板变量

## Potential Risks and Mitigations

1. **工具描述过长导致 token 浪费**
   Mitigation：shell.md 增加的内容控制在 20-25 行以内，仅保留关键指导

2. **过度限制导致模型在合理场景下无法使用 shell**
   Mitigation：保留 "unless explicitly instructed or when truly necessary" 的例外条款

3. **不同模型对指令的遵从度不同**
   Mitigation：通过评估基准在多个模型上验证（评估框架已支持多模型矩阵）

## Alternative Approaches

1. **运行时拦截**：在 `ToolExecutor` 中检测 shell 命令是否包含 `find`/`grep`，自动重定向到 `fs_search`
   - 优点：100% 可靠，不依赖模型遵从度
   - 缺点：增加实现复杂度，可能误拦截合理用例（如 `find . -name "*.lock"` 管道操作）

2. **工具描述 + agent 提示双重强化**（推荐方案）
   - 优点：不修改运行时逻辑，仅优化提示工程
   - 缺点：依赖模型遵从度，可能需要迭代

3. **从 shell 工具中移除文件操作能力**
   - 优点：从根本上消除问题
   - 缺点：过于激进，某些场景确实需要 shell（如 `git grep`、复杂管道）
