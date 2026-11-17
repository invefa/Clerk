# Clerk文档导航

文档按用途分类，文件名不带数字前缀。目录决定归属，本文提供阅读顺序。

## 一、阅读路线

- 初次使用：先读[使用手册](guide/user-manual.md)的快速上手，再按[场景指南](guide/scenarios.md)完成一轮任务。
- 日常协作：从[批处理](features/batch.md)、[上下文与模板](features/context-and-templates.md)和[协作登记](features/collaboration.md)查对应功能。
- 维护工作区：以[声明格式](architecture/declarations.md)为配置依据，遇到Git收尾问题查[本机状态](maintenance/local-state.md)。
- 修改实现：先读[概念模型](architecture/model.md)与[设计要求](architecture/design.md)，再定位源码职责。

## 二、使用指南

| 文档 | 用途 |
|---|---|
| [使用手册](guide/user-manual.md) | 完整功能地图、快速上手、操作、终端阅读、补全与排错 |
| [场景指南](guide/scenarios.md) | 接手、任务推进、并行协作、提案、批量操作与恢复 |

使用手册保留完整阅读路径，专题页负责深入解释，声明格式是参数与配置规则的依据。

## 三、功能说明

| 文档 | 用途 |
|---|---|
| [批处理与关注事项](features/batch.md) | 临时草稿、计划、预览、演练和一次提交 |
| [上下文与模板](features/context-and-templates.md) | 接手资料、工作流参数、关注事项增删与机器声明 |
| [协作登记与文件引用](features/collaboration.md) | 活动会话、结构化日志与文件链接维护 |
| [协作工作流](features/workflows.md) | 文稿权限、安全同步、事务结果与Git重试 |
| [读取选项与终端](features/terminal.md) | 数据范围、选项分组、枚举简写、搜索编辑与交互排序 |

## 四、维护与迁移

| 文档 | 用途 |
|---|---|
| [版本发布](maintenance/releases.md) | 下载、安装、干净构建、校验与发布流程 |
| [提交说明](maintenance/commit-messages.md) | 自动、必填与固定文案策略 |
| [本机状态](maintenance/local-state.md) | Git元数据目录、身份、回执、中断恢复与迁移 |

当前操作以使用手册、专题说明和实际声明为准。
命令不兼容变更在[批处理说明](features/batch.md)中集中提示。

## 五、架构与声明

| 文档 | 用途 |
|---|---|
| [概念模型](architecture/model.md) | 形态、身份、写入边界、不变量与源码职责 |
| [声明格式](architecture/declarations.md) | 工作区、种类、字段、动作、视图与命令参数 |
| [设计要求](architecture/design.md) | 声明边界、共享实现、错误处理与验收标准 |
| [架构复核](architecture/review.md) | 架构问题、已修复项、验证方法与未验证边界 |

## 六、源码定位

源码使用实际Rust子模块组织，不保留旧根模块别名。二进制入口为[main.rs](../src/main.rs)，库入口为[lib.rs](../src/lib.rs)。内部Rust路径随目录调整，命令行、工作区声明与数据格式保持不变。

| 目录 | 职责 |
|---|---|
| [foundation](../src/foundation/mod.rs) | CSV、错误、时间与版本 |
| [config](../src/config/mod.rs) | 声明、种类、词表、工作区与身份 |
| [storage](../src/storage/mod.rs) | 数据存储、文稿、编号与封存 |
| [execution](../src/execution/mod.rs) | 上下文、操作执行、事务、校验与渲染 |
| [collaboration](../src/collaboration/mod.rs) | 接手资料、日志、会话、请求、文件引用与模板 |
| [git](../src/git/mod.rs) | 提交、同步与操作恢复 |
| [query](../src/query/mod.rs) | 共享匹配、过滤、排序与只读选择 |
| [terminal](../src/terminal/mod.rs) | 值显示、文本、列表、分页、搜索与样式 |
| [interface](../src/interface/mod.rs) | 命令元数据、解析、分发、帮助与补全 |

端到端测试仍位于[tests](../tests)，隔离的单元测试跟随所属模块。文档旧地址已移除，不提供旧文件名入口；仓库外的书签需要更新。
