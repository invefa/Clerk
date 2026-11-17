# Clerk使用手册

- 日期：[20261103]
- 适用版本：`0.1.0`，含事务、会话、链接维护与提交说明功能；用`version --json`核对构建
- 读者：工作区使用者、维护者与自动化代理

升级提示：执行器已由`run`改名为`batch`，原任务批次改为`focus`，不保留旧写法。
已有配置须按[迁移说明](../features/batch.md)更新。`batch`草稿是成功后自动清理的本机临时文件。

[快速上手](#quickstart) / [独立练习](#demo) / [对象操作](#objects) /
[协作与Git](#collaboration) / [工作区维护](#maintenance) /
[设计逻辑](#design) / [问题定位](#troubleshooting) / [场景指南](scenarios.md)

## 一、工具定位与阅读路线

Clerk把仓库里的任务、契约、复核等对象组织成可查询、可校验、可追溯的文件。
数据仍保存在CSV、Markdown和TOML里，由Git记录历史；Clerk负责解释声明、执行受控修改、
检查一致性，并把数据渲染成供用户阅读的页面。

已有工作区的用户从第二章开始；想独立体验的用户按第三章建立练习工作区。
第四章覆盖对象操作，第五章覆盖协作与Git，第六章覆盖工作区维护，
第七章解释设计，第八章用于排错。按具体问题学习可直接进入[场景指南](scenarios.md)。

完整分类目录见[文档导航](../README.md)。
命令帮助由当前声明与命令元数据生成，是参数与动词的实际依据。
本手册覆盖各类功能的用途、入口和限制，精确参数以当前帮助为准。
各仓库的种类、字段、编号与动作可能不同。

### 1.1 功能地图

| 用户要做的事 | 功能入口 | 本文位置 |
|---|---|---|
| 找到工作区、核对版本与身份 | `version`、`whoami`、`id`、`people`、`repos` | 第二章、6.3 |
| 发现规则和资料位置 | `help`、`kinds`、`explain`、`scope`、`place` | 2.3、6.3 |
| 建任务、修改字段、推进状态 | 种类的`new/set`、迁移、认领与附件动作 | 4.2、4.4 |
| 筛选、排序、导出和脚本读取 | `list/show`、过滤、输出格式、机器声明 | 4.1、6.5 |
| 管契约、复核和阅读页面 | 文稿、模板、索引与`render` | 4.3、4.8 |
| 记录依赖和设计依据 | `link/unlink`、引用字段 | 4.5 |
| 记录进展并让接手者查到 | 日志追加、显式任务关联、`context` | 4.6、4.7 |
| 管资源、里程碑、验证与限制 | 声明种类、内容哈希与`verify` | 4.8 |
| 清理历史和批量录入 | `archive/restore/delete`、`new --from` | 4.2、4.9 |
| 整批开工、送审或完成 | `workflow`、`batch` | 5.3 |
| 确定关注事项、登记活动范围 | `brief`、`focus`、`session` | 5.4 |
| 审核别人提出的改动 | `--request`、`request` | 5.1 |
| 同步、解释提交并恢复Git失败 | `sync`、说明策略、`operation` | 5.2、5.5 |
| 维护文件链接和已封存内容 | `links`、`check`、`seal` | 6.1、6.4 |
| 建工作区与调整规则 | `init`、声明与默认值 | 第三章、6.2、6.6 |

### 1.2 示例约定

第三章是可从空目录顺序执行的练习，关闭了自动提交和远端同步。
其余示例是命令形状：`APP-001`、`SESSION-ID`、模板名、字段和状态必须替换为
当前工作区已有值，不能把本手册中的编号当成真实任务。

工作区若启用`policy.commit_mode = "required"`，每次会自动提交的写命令都要补充
`--message`或`--message-file`。下文单步示例省略此共用参数；整批`batch`只提供一次。
`--dry-run`也按该策略校验。提交说明文件可放在Git忽略的工作目录，避免触发已有改动保护。
只读命令、请求捕获和活动会话不接受提交说明。完整示例见[场景指南](scenarios.md)。

接手任务可先用`clerk context tasks <编号>`收集资料；常用过程可用`workflow`模板与
`batch --template`执行。关注成员推荐用`focus add/remove`增删，避免覆盖其他人的任务。
参数、声明与使用边界见[任务接手与工作流模板](../features/context-and-templates.md)。

<a id="quickstart"></a>

## 二、已有工作区的快速上手

### 2.1 安装与定位

可从[版本发布](../maintenance/releases.md)下载Windows x64发行包。源码安装需要Rust与Git。源码仓库通过[rust-toolchain.toml](<../../rust-toolchain.toml>)指定工具链。
在Clerk源码目录执行：

```sh
cargo install --path . --locked
clerk --version
clerk version --json
```

安装后确认Cargo的二进制目录已加入`PATH`，通常是用户目录下的`.cargo/bin`。
随后进入要管理的仓库。工作区标记是`.clerk/workspace.toml`，Clerk会从当前目录向上查找。
也可以显式指定根目录：

```sh
clerk --root /path/to/workspace kinds
```

定位优先级为`--root` > 环境变量`CLERK_ROOT` > 向上查找。示例路径应替换成真实路径。

### 2.2 首次检查

在工作区根目录依次执行：

```sh
clerk whoami
clerk kinds
clerk scope
clerk check
```

| 命令 | 需要确认的内容 |
|---|---|
| `whoami` | Git姓名、邮箱、映射后的姓名与本机操作者是否正确 |
| `kinds` | 本仓库实际声明了哪些对象种类 |
| `scope` | 哪些路径受管，治理等级是什么 |
| `check` | 数据、文稿、索引、链接和封存是否存在问题 |

Git身份缺失时，在当前仓库设置自己的真实姓名与邮箱。`clerk id <name>`设置本机操作者。
当前`--as`虽出现在帮助中，命令执行尚未把它传入身份解析，不应依赖它覆盖操作者。
认领、发布者与作者字段取Git身份及人员映射，操作者标签不会替换这些身份。
同一用户的多个Git账号可以由维护者通过`clerk people`归并。身份信息不是权限认证。

`version --json`无需工作区，输出构建提交、构建时的未提交标记和各数据格式版本。
软件版本号相同可能来自不同提交；报告问题时附此输出，再附工作区的Git状态。

### 2.3 发现本仓库的用法

若`kinds`中有`tasks`，继续执行：

```sh
clerk help
clerk tasks
clerk help tasks
clerk explain tasks
clerk tasks list
```

种类页会展示实际动词、字段与取值；`explain`说明声明和存储位置。
`ref`字段会在帮助中列出`registry`或`target`，值必须是来源中的原始键；
负责人、里程碑等引用字段的多值逐项校验，归档对象仍可被引用。
先查看规则，再新建或迁移。`tasks`是声明文件的名字，不是所有工作区都固定存在的命令。

<a id="demo"></a>

## 三、独立练习工作区

本章在一个新的空目录练习，不依赖现有业务仓库。命令块使用PowerShell，TOML与CSV内容
按指定路径保存为UTF-8文本。不要在已有工作区覆盖这些文件。

### 3.1 初始化目录

```powershell
New-Item -ItemType Directory clerk-demo
Set-Location clerk-demo
git init -b main
git config user.name "Demo User"
git config user.email "demo@example.com"
New-Item -ItemType Directory .clerk
Set-Content -Encoding utf8 .clerk/workspace.toml "version = 1"
clerk --root . init
```

当前版本在定位工作区时要求入口文件已存在，因此先写入最小标记，再用`init`补齐
基础目录与现状页。`init`可重复执行；业务种类仍需声明。
将`.clerk/workspace.toml`替换为以下内容：

```toml
version = 1
name = "clerk-demo"

[policy]
auto_commit = false
auto_push = false

[sync]
enabled = false

[ids.tasks]
pattern = "TASK-{serial:3}"
counter = "serial"

[[path]]
path = ".clerk/data"
level = "sealed"

[[path]]
path = ".clerk/archive"
level = "sealed"
```

练习没有远端，因此关闭同步闸门；真实协作仓库应按团队策略配置同步与Git收尾。

保存`.clerk/config/registry/state.csv`：

```csv
key,display
todo,待开始
active,进行中
done,完成
```

保存`.clerk/config/kinds/tasks.toml`：

```toml
title = "任务"
description = "练习任务的创建、迁移与归档"
table = ".clerk/data/tasks.csv"
key = ["id"]
numbering = "tasks"

[state]
field = "state"
registry = "state"
default = "todo"

[[field]]
name = "id"
type = "text"
role = "status"

[[field]]
name = "title"
type = "text"
required = true

[[field]]
name = "state"
type = "enum"
registry = "state"
role = "status"

[[field]]
name = "note"
type = "text"

[[transition]]
name = "start"
from = ["todo"]
to = "active"

[[transition]]
name = "complete"
from = ["active"]
to = "done"
requires = ["note"]
archive = true

[[view]]
name = "list"
columns = ["id", "title", "state", "note"]
```

这里的`role = "status"`表示字段由工具维护，状态通过迁移动词修改。
`complete`要求`note`非空，`archive = true`表示该目标状态允许后续归档。

### 3.2 创建与完成任务

```powershell
clerk check
clerk tasks new --title "编写第一份使用说明"
clerk tasks show TASK-001
clerk tasks start TASK-001
clerk tasks set TASK-001 --note "说明已完成并验证"
clerk tasks complete TASK-001
clerk tasks list --display
clerk check
```

第一条记录编号为`TASK-001`，初始状态为`todo`，随后变为`active`和`done`。
如果省略`note`就执行`complete`，命令会拒绝迁移，并指出缺少的前置字段。

确认结果后建立Git基线，供下一节请求使用：

```powershell
git add .clerk status.md
git commit -m "docs(demo): record the first task"
```

练习中的简短提交消息使用Git默认规则；实际仓库还可能要求提交正文与钩子检查。

### 3.3 请求与归档

```powershell
clerk tasks new --title "补充示例" --request add-example
clerk request list
clerk request show add-example
clerk request apply add-example --dry-run
clerk request apply add-example
clerk tasks list
clerk tasks archive TASK-001
clerk tasks list --data-scope archived
clerk tasks restore TASK-001
clerk check
```

捕获请求时只保存待执行操作，正式应用后才生成第二条任务`TASK-002`。
演练不保留业务修改；归档从在办集合移走记录，恢复保留原编号与状态。
练习关闭了自动提交，后续修改需要自行检查并提交。

<a id="objects"></a>

## 四、日常操作

本章命令形状通用，示例以第三章的任务声明为准。其他工作区需要替换种类、键与字段。

### 4.1 查询与输出

帮助总览按用途导航，具体读取选项按Data scope、Query and order、Content、Output与Reading分组，输入与写入选项另列。`clerk tas lis --help`解析唯一前缀后显示list的适用参数；完整种类帮助继续提供字段与声明。分层规则与不兼容迁移见[读取选项与交互阅读](../features/terminal.md)。选项短名、长名和说明采用全页统一列宽，候选与默认值放在附属行，窄屏自动改为上下布局；可用`clerk tasks list --help --width 48`查看效果，详见[帮助阅读](../features/terminal.md#34-帮助阅读)。

默认查询current，即尚未归档的记录；`--data-scope archived`只查归档，`--data-scope all`合并两者后统一过滤和排序，适用于list与show。归档与混合文本增加`@archive`来源列，JSON通过独立的scope与provenance提供来源。旧`--archived`已移除，原有业务scope字段保持不变。

任务摘要列表默认展示编号、标题、状态、发布者、认领者、类型、仓库与更新时间。
publisher回答“谁创建”，claimant回答“谁当前执行”；空认领者显示`unclaimed`。
owners继续表示长期负责人，保存在记录里，可指定列或通过详情查看。
默认每个任务只占一行，终端宽度不足时用`...`标记长标题，放不下的列在表尾统一提示。
表格多值使用`ability,defect`，逗号后不留空格，列之间仍留一个空格并对齐。

| 模式 | 展示 |
|---|---|
| 默认list | 声明选中的精简列，一任务一行，适应终端宽度 |
| list --long | 声明的短字段扩展列表，一任务一行，适应终端宽度 |
| list --details | 每任务一个竖排详情块，全部字段，按终端宽度换行 |
| show编号 | 单个任务的竖排详情 |

```sh
clerk tasks list --width 100
clerk tasks list --long
clerk tasks list --long --columns id,title,publisher,claimant,owners
clerk tasks list --details --width 100
clerk tasks show APP-001
```

`--long`与`--details`互斥，显式`--columns`在两种模式下都优先。
默认和long都按终端宽度排版，弹性列优先获得空间，次要列按声明的优先级隐藏，表尾统一提示。
long增加负责人、里程碑、创建时间等简短字段，不横向铺开accept、note、邮箱、完整哈希和附件路径。
显式columns优先保留所选列并省略单元格内容；极窄窗口连最小列宽都容纳不下时才隐藏尾列。
完整值始终可通过details、show或机器格式读取。
管道输出默认不探测宽度，`--plain`关闭宽度限制；机器格式按声明或选定列返回原值，
这两个模式不改变JSON、CSV、TSV和Markdown的表示。正文、依赖和日志用context读取。

本轮修正long为声明的短字段扩展列表，不再输出所有字段的无限宽表。
list和long复用view声明，通过columns、flex和priority配置显示顺序、弹性列与隐藏优先级，
字段选择不在代码中硬编码；没有long视图时沿用list。原有details竖排语义保持。
旧视图声明的`secondary`、`presence`、`relations`应移除，历史任务与关系数据不删除。

```sh
clerk tasks list --state active
clerk tasks list --filter "state in (todo, active)"
clerk tasks list --filter "title ~ 示例 and state != done"
clerk tasks list --columns id,title,state --sort id
clerk tasks list --display
clerk tasks list --json
clerk tasks list --csv
```

字段选项在查询时作为等值条件；`--filter`支持组合表达式。脚本优先使用原始键与`--json`，
显示名适合阅读。常规JSON输出使用`count/columns/data`结构，`check`和`doctor`保留诊断结构。
`--headers hide`只影响阅读表格，判断空集宜用JSON的`count`。

| 选项 | 用途与例子 |
|---|---|
| `--filter` | `and/or`与括号组合；支持`=`、`!=`、`~`、`!~`、`in`、`>`、`>=`、`<`、`<=` |
| `--mine`、`--claimed` | 查当前Git身份认领的记录，或所有已有认领者的记录；不按`owners`替代认领 |
| `--sort state,id`、`--desc` | 多字段排序、整体倒序；编号可按声明令牌排序，如`id:prefix` |
| `--long` | 声明的短字段扩展列表，共用终端宽度布局 |
| `--overflow truncate/more/pager/full` | 终端超高时单屏截断、增量分页、交互分页或完整输出；默认full |
| `--details` | 全部字段的竖排详情块，与show一致 |
| `--columns id,title` | 只调整输出列，不删除或修改底层字段 |
| `--display` | 用词表显示名呈现；自动化建议保留原始键 |
| `--json/--csv/--tsv/--md` | 分别用于脚本、表格交换、制表符交换、可粘贴的Markdown表格 |
| `--max-width 100` | 限制终端单元格显示宽度；精确取值请使用机器格式 |
| `--headers hide`、`--quiet` | 隐藏表头，或在支持的命令上抑制成功输出；仍检查退出码 |

文本包含比较忽略大小写；日期和时间比较按存储字符串进行，须使用声明的规范格式。
多值字段的等值查询检查成员，输入仍按字段和词表校验。命令缩写仅在唯一时可解析，
Agent与脚本使用完整命令、动词、长选项和枚举值。短名供用户手工输入；组合遇到带值选项后，余部作为它的值，没有余部则消费下一个参数，见[用户短名](../features/terminal.md#32-用户短名)。
需要关闭默认开关时用`--no-display`等形式，不能写`--display=false`。
字段名与选项名冲突时，可在过滤表达式中使用字段名。

终端文本默认按列对齐，列之间留一个空格，中文按双列计算，多行值的续行与所属列对齐。
加`--compact`可省去列补齐空格与表头分隔线，列表、详情、工作流和上下文包共用此规则。
例如`clerk tasks list --compact`。如果命令默认值设置了`compact = true`，
使用`--no-compact`恢复对齐。JSON、CSV、TSV、Markdown及文档渲染保持原有格式。
同级字段对齐；从属信息逐级缩进两个空格，紧凑模式也保留层级。
例如工作区的分支、提交、状态和远端引用位于`Workspace`下面，各仓库的信息位于仓库名下面。

工具默认full，终端中的对象列表、竖排详情、帮助、工作流list/show和context完整连续输出。
需要单屏预览时显式使用`-o truncate`或`--overflow truncate`。
输出超出窗口时，保留完整表头和尽量完整的条目，底部显示已展示数量、总数量以及
`--overflow full`和`--overflow pager`提示；预留两行给提示符。首个详情块本身超高时，
只显示能容纳的部分并标记`partial`。正文类输出按行计数，列表与详情按条目计数。
这只影响本次显示，不影响查询、数据、提交或导出。

```sh
clerk tasks list                         # 默认完整输出
clerk tasks list -o truncate             # 一屏预览
clerk tasks list --overflow more         # 回车追加一行，空格追加一屏
clerk tasks list --overflow pager        # 在终端中浏览全部结果
clerk tasks list --long --overflow full  # 连续打印全部结果
clerk context tasks APP-021 --overflow pager
clerk help tasks --overflow full
clerk tasks list --json                  # 始终完整，适合Agent
```

more称为增量分页：回车追加一行，空格追加一屏，q、Esc或Ctrl+C停止。
内容保留在普通终端回滚里，支持复制和回看；表头只显示一次，底部显示进度。
调整窗口只重排尚未输出的内容，已经输出的历史保持原样。

pager使用独立屏幕，退出后恢复原终端。上下方向键逐行移动，PageUp/PageDown或空格翻页，
Home/End跳到开头或末尾，`/`输入搜索词并回车，`n`查找下一处。浏览时Esc先清除查询并保持阅读位置，无查询时Esc退出；浏览时q、任意状态Ctrl+C退出。
表头固定在页顶；窗口缩放后按新宽度重排同一份快照，不重新读取或修改工作区。
分页采用前后帧缓冲，只绘制变化的行；输入搜索词时保留正文，短行替换长行时清理旧尾部，
普通按键不清空整屏。支持同步更新的终端会在帧准备好后统一显示。
`--plain`的长行在分页屏幕内仍按屏幕宽度折行，退出分页后不改变原文件。

管道、重定向、JSON/CSV/TSV/Markdown等机器格式始终完整输出，不截断、不进入分页、
不等待键盘输入；标准输入不是终端时，more和pager回退为完整输出。
写入回执、错误与可执行的工作流preview也保持完整。
需要始终完整阅读时，可在工作区命令默认值中设置，例如：

```toml
[commands."tasks.list"]
overflow = "full"
```

### 4.2 搜索、编号选择与终端策略

`-o`是`--overflow`的简写，`-p`是`--paging`的简写。所有枚举型选项的CLI值支持唯一前缀，例如`-o p -p al --color n`；`-p a`有歧义，需使用au或al。`-S`是data-scope简写，`-Sal`表示all。支持`--name=value`、`-Svalue`及短开关组合，完整规则见[参数书写方式](../features/terminal.md#31-参数书写方式)。配置和持久计划使用完整值。pager和more缺省为自动启用：内容能放进一屏时直接输出，超出才进入交互。
需要搜索短列表时加`--paging always`，保证进入pager；more在最后保留提示，回车、空格或q结束。
`--paging always`仅适用于pager和more；`--paging never`或`--no paging`禁止交互分页，truncate仍按显式策略执行。管道、重定向和机器格式从不等待按键。

```sh
clerk tasks list -o pager --paging always
clerk tasks list -o more --paging always
clerk context tasks app-043 -o pager --paging always
```

pager中按`/`编辑当前搜索词，左右与Home/End移动光标，Backspace/Delete删除，输入和粘贴在光标处插入；Ctrl+u清空。输入超长时水平滚动，光标保持可见。回车确认后退出编辑，再按Enter或n跳到下一处，N上一处，首尾循环；无查询时Enter仍向下一行。底部显示搜索词、literal或regex、
icase或case、当前结果与总数，例如`Search "Clerk" | literal icase | match 3/12 @ row 8`。
匹配按出现位置计数，同一行出现两次算两处；所有可见匹配加下划线，当前项反色。
搜索强调与字段语义颜色叠加，不改变匹配范围；移开高亮后恢复原颜色。kind list的pager还可按s编辑排序、Tab补全字段、r反转、R恢复初始排序；它重排完整记录快照，保留搜索词并尽量保留当前记录。详情、long同样支持，context与帮助不显示排序入口。
手动翻页后选中项离开屏幕时显示off-screen；关闭颜色仍保留行位置、计数和状态文字。
搜索只涉及当前展示内容，隐藏列和被省略的文字不产生不可见结果；完整字段使用details或context。

搜索默认为literal且忽略大小写，句点和星号按普通文字处理。Alt+r切换正则，Alt+c切换大小写；
可在命令中设置`--search-mode regex`和`--case sensitive`。
正则支持常用分组、重复、字符类、首尾定位及选择，不支持回溯引用和环视，表达式有长度和编译资源限制。
非法表达式保留输入并显示错误。Esc取消编辑并恢复上次查询，浏览时Esc清除查询及高亮并保留当前位置，再按Esc退出；空查询回车也可清除。编辑时q仍作为字符输入，Tab补全本次阅读的搜索历史。
纯折行按逻辑位置保留选中项和结果数量，不把折行算作新结果；列隐藏或单元格省略变化时，按当前可见内容重新检索。

过滤表达式保留原有含义：`~`和`!~`默认忽略大小写包含，等号和in默认精确比较。
新增matches和not matches正则运算符，默认忽略大小写；多值字段按成员匹配，否定表示没有成员命中。
显式`--case ignore`或`--case sensitive`覆盖文字比较，日期与数值顺序比较不受影响。

```sh
clerk tasks list --filter 'title matches "Clerk|预约"'
clerk tasks list --filter 'id matches "^APP-04[0-9]$"' --case sensitive
clerk tasks list --filter 'publisher = "ALICE"' --case ignore
clerk tasks list --filter 'title not matches "试验|废弃"'
```

show和context默认先选原样精确编号，再忽略大小写精确匹配；可以直接输入小写编号。
`--match prefix/contains/regex`启用前缀、包含或正则编号选择，`--match exact`为默认。
只有唯一匹配才展示，多条返回编号和标题候选，不自动选择第一条。它只查主键，不查标题。
写入、关系和批处理仍使用规范编号，不做近似选择。

```sh
clerk tasks show app-043
clerk tasks show 043 --match contains
clerk context tasks app-04 --match prefix
clerk tasks show '^app-043$' --match regex
```

终端颜色使用`--color auto/always/never`，缺省auto。auto仅对合适的终端启用并遵守NO_COLOR；
always可为人读文本管道保留颜色，never完全关闭。JSON、CSV、TSV、Markdown和生成文档始终纯净。
表头与分组加粗，编号强调，辅助说明弱化；状态颜色由字段的terminal_style声明，不由代码猜测状态名。
宽度和搜索使用原始文本，ANSI只在最后编码；pager差分同时比较文字与样式。

工作区统一默认值与命令覆盖示例：

```toml
[terminal]
overflow = "full"
paging = "auto"
color = "auto"

[commands."tasks.list"]
overflow = "more"

[commands.context]
overflow = "pager"
paging = "always"
```

优先级为命令行、具体命令默认值、命令默认值、terminal、工具缺省。

### 4.3 PowerShell自动补全

在当前PowerShell会话中加载，已在PowerShell7实测：

```powershell
clerk completion powershell | Out-String | Invoke-Expression
```

按Tab可补全命令、种类、动词、选项、模式合法值、词表值、实际编号及工作流模板参数。
编号忽略大小写筛选，插入规范编号，候选说明带标题；文件参数使用PowerShell路径补全。
filter在明确的字段、运算符、词表值位置补全，复杂表达式和正则内部保持原样。

希望持久加载时，将输出保存到自己选择的脚本，并在PowerShell配置文件中点源加载；工具不会自动修改个人配置。
补全脚本只传递光标上下文，候选来自当前工作区声明和只读数据，不同步远端，不恢复写入，也不修改记录。
工作区不可用时仍补全静态命令与选项。其他Shell本轮不提供注册脚本。

机器调用入口：`clerk completion complete --words-json '["tasks","show"]' --current app- --json`。
words-json仅包含当前词元之前的参数，不含clerk命令本身；current为当前词元，可以为空。
候选含insert、label、description和kind；kind为path时由Shell处理路径。

### 4.4 修改、迁移与删除

发布者和发布邮箱是创建时的Git账号快照，不跟随后续认领、转交或本机operator变化。
为这些字段及创建时间声明`immutable = true`后，修改、迁移、批处理和工具动作都不能改写；
更新时间继续反映实际修改时间。已存在的历史值保留，工具不凭当前身份推断原作者。

`new`创建，`set`修改用户字段，迁移动词控制状态，动作完成认领、附件或追加等声明行为。
动作名称与前置条件来自种类声明。多值字段按声明使用`|`分隔，并给整个值加引号。

```sh
clerk tasks set TASK-002 --note "补充说明"
clerk tasks start TASK-002 --dry-run
```

表存储的`new --from <file.csv>`支持按CSV行批量新建，列名对应字段；整批失败时回滚。
`--request`不能与`--from`或`--dry-run`组合。

归档保留对象，删除则需要显式`--force`。普通使用优先选择归档；归档与删除都不会释放
已分配编号。`--force`不是绕过所有前置条件、同步检查或封存检查的通行证。

### 4.5 文稿与生成页面

文稿种类使用Markdown文件保存对象。若本仓库声明了`contracts`，可以先查看帮助，
再用`new --title ... --file ...`从文件提供正文，也可用`--body`提供正文文本。
文稿可能要求特定章节与元信息，起稿时应遵循仓库模板。

文稿正文由用户撰写，受管元信息通过命令修改；修改`title`也会更新文稿标题。
列表页、现状页中的受管区块由视图生成：

```sh
clerk render --dry-run
clerk render
clerk check
```

只编辑生成区块以外的手写正文。记录变化后，页面需要单独`render`才更新。
第三章的最小工作区没有页面目标视图，因此不会生成任务现状区块。

正文入口是`--body`、`--file <path>`或`--file -`。附件追加等需要正文的动作在未显式
输入时可读管道，交互终端可通过`VISUAL`或`EDITOR`打开编辑器。查询命令不会主动读取管道。
直接命令的正文文件相对调用目录解析；工作流中的正文文件相对工作流或模板文件解析。
一个标准输入不能同时承载正文、CSV、工作流和提交说明。

### 4.6 负责人、认领与状态

负责人说明任务分工，认领说明谁正在执行，状态说明当前进度，活动会话说明当前对话
修改哪些路径。四者分别记录，设置负责人不会自动认领、开工或登记会话。

若本仓库声明了相应动作：

```sh
clerk tasks claim APP-001
clerk tasks brief APP-001 --body "范围：调整缓存。验收：回归通过。"
clerk tasks start APP-001
clerk tasks list --mine
clerk tasks hold APP-001 --note "等待接口契约确认"
clerk tasks resume APP-001
clerk tasks finish APP-001 --body "交付、验证和仍需后续处理的事项"
clerk tasks complete APP-001 --evidence tasks/APP-001-done.md --commit abc1234
clerk tasks release APP-001
```

上面展示各入口，不是要求同一任务无条件顺序执行。`brief`保存范围附件，`finish`保存
完成附件，真正推进完成状态的是`complete`。`--commit`是任务证据中的代码提交；
`--message`是本次管理文件自动提交的说明，二者含义不同。
`release`只清除认领，`resume`只做声明的迁移，均不自动执行其他步骤。
状态迁移检查来源、必填证据和声明前置条件。已经开工的任务不能重复跑要求`todo`的动作。

### 4.7 引用字段与对象关系

`owners`等引用字段解决合法取值和归属；对象关系解决依赖、拆分、契约依据等连接。
只有声明了该关系的种类才支持对应的`link/unlink`。

```sh
clerk tasks link APP-001 blocked tasks APP-002
clerk tasks link APP-001 contract contracts 28
clerk context tasks APP-001
clerk tasks unlink APP-001 blocked tasks APP-002
```

关系名、目标种类和键都要匹配声明。关系方向有意义；`blocked`的具体解释按团队约定。
正文写到某个编号，不会自动建立正式关系。上下文可以同时显示直接关系、反向关系和
对象引用字段。当前显式关系命令采用单字段对象主键；复合主键查询在`show/context`中
按声明的`key`顺序分别传入，不能假设把所有键拼成一个字符串即可。

### 4.8 阶段日志与任务关联

日志用于保存已经发生的事实、证据、决定和阻塞。任务状态只说明进度，不能替代过程记录。
如果工作区提供带`entry_target`的`logs add`：

```sh
clerk logs add --title "缓存回归" --tasks APP-001,APP-002 --body "两项任务共用的回归已通过，结果见完成说明。"
clerk logs list
clerk context tasks APP-001
```

`--tasks`支持逗号或竖线分隔多个已存在的主键，包括归档对象；某个编号错误时整条追加
失败。关联保存为每条日志的结构化元数据，后续即使正文未写编号也能进入任务上下文。
旧日志仍可按正文里的完整编号补充匹配，不能只写简称就期待自动关联。
日志通常按日期、操作者聚合成页面，具体目录、标题与条目格式来自声明。
`logs prune --before 20260101`仅在工作区声明了该动作时用于归档旧日志页；先检查帮助
和截止日期，不把它当作清空日志。更多条目边界见[协作登记](../features/collaboration.md)。

### 4.9 任务上下文包

```sh
clerk context tasks APP-001
clerk context tasks APP-001 --json
clerk context tasks APP-001 --full --overflow pager
clerk context tasks APP-001 --full --json
```

接手任务时，它一次输出字段、说明和完成附件、一层双向关系、关联文稿摘要、最近五条
日志、本机仓库与会话观察。没有在办记录时再查归档。每份来源带路径、摘要与哈希；
默认正文过长会截断并保留来源，缺失来源会给诊断。`Truncated: true`表示字符数超限，
不是固定行数限制；提示中的`--full`可读取完整正文，也可按Source路径打开原文。
文稿原文逐行带`|`，与工具的层级标签分开，原文Markdown标题不会被误认为工具标题。

直接输出到终端时，`context`按可见窗口宽度主动换行，并在`|`之前显示真实源文件
行号；折行续行的编号栏留空，原文空行仍有编号。`--width 80`可固定总宽度，
`--plain`关闭主动换行和行号，保留层级与原文标记，两者互斥。
管道与重定向默认使用plain；显式宽度可在管道中启用阅读排版。中文、组合字符和
表情按显示宽度与字素边界处理，阅读时的制表符展开不修改原文件。
JSON保留原文与来源信息，不添加显示折行或行号装饰。
`workflow list/show`共用阅读排版；`show`展示真实模板原文及行号，`preview`继续
输出可执行TOML。`--compact`只省略补齐空格，`--max-width`只限制表格单元格。
阅读选项允许设为命令默认值；显式JSON与模板预览忽略这些默认排版，不能显式
同时传入阅读选项。总宽度允许20到10000列，终端探测失败时用有效`COLUMNS`或80列。

附件、仓库和日志来源由种类的`[context]`声明，不是按文件名猜测。
关联契约须有正式关系或引用字段。只展开一层、最多五十条关系，以免无限展开；
附件最多四千字符，关联文稿两千字符，每条日志一千五百字符。
`--full`取消上述正文字符上限，同时适用于终端和JSON，仍只取一层关系和最近五条日志。
`--overflow full`只控制终端显示范围，`--plain`只控制排版，两者均不取消正文摘要。
可在`[commands.context]`设置`full = true`作为本工作区偏好，临时用`--no-full`恢复摘要。
颜色沿用`--color auto|always|never`：行号与续行栏弱化，来源弱化，截断与诊断使用警示色，
字段值复用种类的`terminal_style`声明；原文内容保持原样，JSON始终不含颜色码。
上下文不抓取远端，远端引用标记为缓存；存在中断事务时拒绝读取，先按提示恢复。
本机仓库和外部编辑器状态属于读取时观察，不能据此断言远端已经同步。

### 4.10 各类受管资料

种类名称和业务字段来自工作区声明，以下是常见配置，不是内建固定业务模型。

| 资料 | 解决的问题 | 常见用法 |
|---|---|---|
| `tasks`任务 | 范围、负责人、验收与进度 | 创建、认领、说明、迁移、关联、完成证据 |
| `contracts`契约 | 多方需要共同遵守的接口和数据约定 | 从正文文件创建文稿，保留编号、章节与索引 |
| `reviews`复核 | 保存检查对象、结论和整改依据 | 创建复核文稿并与任务关联；不会自动执行代码审查 |
| `milestones`里程碑 | 汇总阶段目标和任务归属 | 创建记录，通过任务引用字段分组 |
| `verify`验证 | 记录哪些能力已验证及其证据 | 新建或更新记录，渲染验证页面 |
| `limits`限制 | 记录已知限制、影响和解决情况 | 声明的`resolve/reopen`迁移控制解决与重开 |
| `assets`资源 | 保存来源、许可和文件内容哈希 | 新建登记、复合主键查询、`verify`核对内容、声明动作确认 |
| `logs`日志 | 保存阶段事实，并关联多个任务 | 追加条目、查询页面、按声明归档旧页 |

文稿创建所需的标题、短名通常是创建期输入；编号、路径及状态等工具字段不能任意设置。
`explain`可区分工具维护、创建期可写和持续可写字段，避免用`set`绕过状态迁移。
契约与复核可用`new --title ... --slug ... --file ...`，仅传当前种类声明接受的字段。
文稿索引在相关文稿操作时维护，目标页面的视图区块通过`render`更新。

声明了`[hash]`的种类才有`verify`动词，用它重新计算并比较文件哈希，查询本身不更新
登记值。哈希只能判断内容是否变化，不能判断来源或许可是否正确。资源可能使用
`repo,path`复合主键，例如`assets show design figures/icon.png`；文件解析位置按当前
声明和本机检出核对，跨仓证据不能靠猜测目录名。遇到解析问题先用`repos --check`
确认检出，不能为了通过校验填写错误路径。

### 4.11 批量录入与历史整理

表存储可用CSV批量创建，首行是实际字段名，每行相当于一次`new`，共用事务和提交：

```csv
title,note
检查缓存边界,验收为回归通过
补充契约示例,验收为示例可执行
```

```sh
clerk tasks new --from import.csv --dry-run
clerk tasks new --from import.csv
```

先准备UTF-8文件并演练；自动编号等工具字段不要填写。需要编号前缀时在命令中提供
声明的令牌。某行非法时整批回滚，演练不会占用编号。`new --from`不用于文稿种类。

`archive`将允许归档状态的对象移入归档集合，`list --data-scope archived`查历史，`restore`
恢复原编号及状态。`delete --force`删除对象；已经分配的编号不再使用。
归档不是完成迁移，也不自动解除引用。删前检查关系和引用，删后运行`check`。

<a id="collaboration"></a>

## 五、请求审核与Git协作

需要把新建、认领、状态、附件、日志与渲染合并为一次提交时，使用`clerk batch`。
安全快进同步与失败后仅重试Git收尾的方法见[协作工作流](../features/workflows.md)。

### 5.1 请求流程

`--request <name>`把一次种类写操作保存到待处理目录，供维护者审核后执行。
它保存字段、正文、执行选项和Git基线；文件正文在捕获时读入，应用时不再读取原文件。
请求文件仍需要通过团队既有的Git协作流程交给维护者。

```sh
clerk request list
clerk request show add-example
clerk request apply add-example --dry-run
clerk request apply add-example
clerk request reject another-request --reason "需要补充依据"
```

不指定名称的`request apply`会应用整批待处理请求。整批先校验，执行失败则恢复业务文件。
应用成功后，数据、请求删除与回执属于同一次本地操作，并在启用自动提交时共同提交。
相同请求内容由哈希去重；目标或声明相对基线已变化时，命令会拒绝应用。
此时先核对差异，再基于当前状态重新提出请求。

请求提供审核载体与冲突检查。文件写权限、审核权限和身份信任仍由仓库权限与团队流程负责。

### 5.2 同步与提交

```sh
clerk sync fetch
clerk sync check
git status --short
```

`sync check`只比较本地已有的远端引用，`sync fetch`抓取后再比较。
写前是否抓取由声明决定。发现远端包含本地没有的提交时，按仓库约定更新并处理冲突，
再运行`check`与原操作。

| 策略 | 写入之后的行为 |
|---|---|
| 未启用自动提交 | 本地文件保留，由用户检查、提交和推送 |
| 启用自动提交，写前工作树干净 | 工具尝试提交受影响文件，再按策略推送 |
| 写前已有未提交修改 | 跳过自动提交并给出提示，用户自行整理 |
| Git提交或推送失败 | 返回非零，本地已经接受的数据保留 |

自动提交支持本次调用的`--message`或`--message-file`。工作区可以通过
`policy.commit_mode`选择必填、自动生成或固定说明，完整规则见
[提交说明](../maintenance/commit-messages.md)。必填模式下演练也需提供说明，模板预览无需提供。

遇到Git失败，先查看错误与`git status`、`git log -1`，确认数据和提交是否已存在，
再修复钩子、远端或网络问题，完成剩余Git步骤。直接重跑新建命令可能产生额外对象。

`sync fetch`只抓取，`sync update`会在配置分支上把干净工作树安全快进到远端。
有本地改动、分叉或不能快进时停止，保留现场。写命令可加`--sync ff-only`先同步再
执行，声明也可配置`sync.write_update = "ff-only"`。同步后会重新加载声明与默认值。
演练不能加`--sync`；先单独同步，再演练。`context`和模板预览明确不接受同步选项。

### 5.3 批量事务与工作流模板

独立工作流描述一组相关写操作，解决单步命令反复校验、提交和推送的成本。
模板进一步把重复流程和变化参数分开，适合固定的开工、送审、完成流程。

```sh
clerk workflow list
clerk workflow show start
clerk workflow preview start --task APP-001 --description "范围与验收条件"
clerk batch --template start --task APP-001 --description "范围与验收条件" --dry-run
clerk batch --template start --task APP-001 --description "范围与验收条件"
```

| 阶段 | 能确认什么 | 是否写入 |
|---|---|---|
| `workflow show` | 模板参数、默认值与步骤 | 只读 |
| `workflow preview` | 参数合法性和展开后的可执行TOML | 只读，不分配编号 |
| `batch --dry-run` | 实际状态、前置条件、整批执行和最终检查 | 在事务中执行后回滚 |
| `batch` | 接受已验证的整批结果并按策略提交推送 | 一次事务，逐项审计，一次Git收尾 |

演练和正式`batch`都要求工作树干净。后一步失败时恢复前面步骤的文件和编号分配；
Git阶段失败则保留已接受数据，按5.5恢复。模板不会自动跳过失败步骤，也不会
因为模板名叫`finish`就证明证据合格或外部PR已合并。

不需要长期模板时，把流程保存为工作流文件，通过`batch --file`执行：

```toml
version = 1

[[step]]
kind = "tasks"
verb = "new"
as = "task"
values = { title = "检查缓存边界" }
tokens = { prefix = "ENG" }

[[step]]
kind = "tasks"
verb = "start"
keys = ["@task"]
```

```sh
clerk batch --file flow.toml --dry-run
clerk batch --file flow.toml
```

该例要求任务的编号方案接受`prefix = ENG`。`values`是用户字段，`tokens`是编号令牌
或动作参数，`body/file`提供正文。`@task`引用前面创建步骤返回的完整主键，
`${task.id}`引用字段。保存于配置目录的模板用`${params.task}`接收声明参数，
展开不拼接TOML源码，正文中的引号和换行不会生成新步骤。
可加入`kind = "focus"`成员操作和无`kind`的`verb = "render"`步骤。
完整步骤和转义规则见[工作流](../features/workflows.md)与[模板声明](../features/context-and-templates.md)。

临时收集一组操作时，不必先手写计划文件。各个写命令添加`--batch <名称>`，
只保存在本机临时草稿；正文在收集时固定下来，正式执行时才分配编号与修改业务。

```sh
clerk tasks new --prefix APP --title '预约校验' --batch reservation --step-name task
clerk tasks brief @task --body '范围与验收条件' --batch reservation
clerk focus add @task --batch reservation
clerk render --batch reservation
clerk batch show reservation
clerk batch apply reservation --preview
clerk batch apply reservation --dry-run --message-file .work/reservation-message.txt
clerk batch apply reservation --message-file .work/reservation-message.txt
```

草稿不进入版本控制，成功后自动清理；演练和失败保留。`batch list/drop`用于检查与删除。
`--step-name task`让后续步骤用`@task`引用创建结果，不指定时使用`@step1`等名称。
相关目标变化时拒绝执行；Git失败后用回执编号执行`operation retry`，不要重放业务。
还可以用`batch --file changes.json --input-format json`接收JSON计划；`--json`控制输出。
完整格式、不兼容迁移及恢复边界见[批处理与关注事项](../features/batch.md)。

### 5.4 关注事项与活动会话

`brief`汇总各类对象的状态数量与关注事项，适合每天开始时查看全局情况；
它与`tasks brief`不同，后者是任务说明附件动作。

```sh
clerk brief
clerk focus show
clerk focus add APP-001 APP-002
clerk focus remove APP-001
```

`focus set`替换整个集合，适合明确重排；多人协作优先用`add/remove`保留其他人的
成员与顺序。重复添加和移除不存在成员均为空操作；新增仅允许在办对象，移除可清理
失效编号。关注事项不会自动认领或改变状态。

活动会话补充当前对话、分支、任务和修改范围，适合同一个人运行多个Agent：

```sh
clerk session open --chat cache-fix --tasks APP-001 --paths src/cache,tests/cache.rs --json
clerk session list
clerk session show SESSION-ID
clerk session touch SESSION-ID --ttl 3600
clerk session scope SESSION-ID --add docs/cache --remove tests/cache.rs --strict
clerk session close SESSION-ID
```

编号来自`open`返回值。路径相对当前工作区根目录，不能填绝对路径或`..`。
目录与子路径重叠时提示，`--strict`拒绝有重叠的本次登记或调整。
默认有效30分钟，`--ttl`允许1到86400秒；没有后台续期，继续工作时显式`touch`。
过期记录保留但不再占用，已关闭记录不能续期。
同一Git克隆的worktree共享登记，不跨独立克隆或机器传播；登记不阻止编辑器写入，
不能代替锁。只有原Git身份和工作树可以修改会话。会话不进入Git提交或推送。

### 5.5 提交说明与操作回执

| 策略 | 未提供本次说明时的行为 |
|---|---|
| `required` | 在业务写入前拒绝，要求调用方解释本次改动 |
| `auto` | 从成功动作、对象编号、字段值预览和受影响路径生成说明 |
| `default` | 使用`policy.commit_message`固定说明 |

显式`--message`和`--message-file`优先，二者互斥。文件按UTF-8读取，相对调用目录
解析，`-`可读标准输入；它不能再同时供正文、工作流或CSV使用。说明不能为空或包含
非法控制字符，工具的`Clerk-Operation:`回执标记不能由调用方伪造。
新配置缺省`auto`；旧配置只设置`commit_message`而未指定模式时保持固定说明行为。
自动说明对长字段值截断并标记，对非ASCII字符转义，不复制附件全文。
提交风格和正文要求继续受工作区校验规则约束。完整规则见[提交说明](../maintenance/commit-messages.md)。

```sh
clerk tasks start APP-001 --message-file .work/start-message.txt
clerk operation status
clerk operation status --json
clerk operation retry
```

`.work/start-message.txt`是你准备的说明文件，使用前确认所在目录已被Git忽略。
`batch`、CSV导入和请求整批应用均只需要一份事务说明。关闭自动提交时不强制提供。

执行回执和逐步结果保存在Git元数据的`clerk/operations/`，不进入版本控制；草稿、锁、撤销日志与缓存也统一在`clerk/`下。升级与存储位置见[本机状态](../maintenance/local-state.md)。

回执分别记录数据、提交和推送阶段，`Clerk-Operation`是将Git提交与本机回执对应的
编号，形如`op-...-...`。`status/retry`省略编号时选择最近一条本机回执；多项操作
交错时按错误返回的编号明确指定。`retry`只做Git收尾，复用已保存的原说明，
不接受新说明，也不重新新建、迁移或追加日志。

| 数据与Git状态 | 下一步 |
|---|---|
| `data=not_written`或`rolled_back` | 修正输入或前置条件，再执行原业务命令 |
| `data=saved`，提交失败 | 检查钩子、索引和HEAD，解决后按回执`operation retry` |
| 提交成功、推送失败或未知 | 检查网络和远端，重试会核对远端是否已接收该提交 |
| HEAD或受影响文件已被改动 | 自动重试拒绝，保留现场并人工审查Git历史与差异 |
| 旧回执没有保存说明且尚未提交 | 人工核对并完成Git提交，不自动编造新说明 |

`not_run`表示对应阶段尚未执行，`skipped`不表示提交成功，`unknown`需要核对现场。
失败后先读回执再决定是否重跑业务；不要把网络重试改写成重复执行整套工作流。

### 5.6 请求、事务和会话的选择

| 情况 | 选择 | 原因 |
|---|---|---|
| 本次已经获准修改一条记录 | 普通种类写命令 | 入口直接，仍走校验和封存 |
| 一组修改应共同成功或失败 | `batch` | 共用事务及一次Git提交 |
| 多次执行相同流程 | 声明模板后`batch --template` | 参数替代重复拼接步骤 |
| 改动须先交维护者审核 | `--request` | 保存提案和基线，应用时再验证 |
| 多个对话可能改到同一范围 | `session` | 给出本机活动和范围提示 |
| 接手资料需要集中阅读 | `context` | 只读收集记录、关系、附件与日志 |

这些功能可组合，例如会话登记后运行已批准的模板；会话本身不会审批写入，请求
本身也不会把文件送到远端，仍要通过Git协作传递。

<a id="maintenance"></a>

## 六、工作区维护

### 6.1 检查与封存

`check`汇总字段、词表、编号、关系、文稿、索引、链接与封存问题；`doctor`描述工作区
结构与数据规模，帮助定位加载与布局问题。日常写入会更新受影响路径的封存。

若手工修改了已封存内容，普通写命令会拒绝继续。先审查`git diff`并运行`check`，
确认修改合理且其他问题已经处理，再显式接受这次漂移：

```sh
clerk seal --note "修正已审核的数据录入错误"
clerk check
```

`seal`记录理由并重建封存。它接受内容变化，不证明内容正确；不要用它掩盖未知来源的漂移。
生成内容应通过修正源数据与重新渲染恢复。

### 6.2 默认值

维护者可以在`workspace.toml`中为已声明命令设置默认输出：

```toml
[commands."tasks.list"]
display = true
columns = ["id", "title", "state"]
sort = "id"
```

优先级为命令行 > 动词默认 > 命令默认 > 视图 > 内建缺省。
开关默认值`false`表示不设置，不会关闭其他层已开启的开关；命令行使用`--no-display`
取消显示名。也可用通用`--no display`。`--no filter`、`--no sort`与`--no columns`取消配置的默认选择并阻止回填，恢复工具或视图的基础行为；`--no color`和`--no paging`则显式关闭颜色与交互分页。短名`-N`的目标支持唯一前缀，详见[通用否定参数](../features/terminal.md#35-通用否定参数)。带值选项也可显式覆盖。`force`、`dry-run`、`request`等危险或运行期选项
不能设为默认。未知命令、选项与错误类型在加载时会被拒绝。

### 6.3 身份、仓库与资料目录

用`clerk whoami`核对Git账号、归并人员和本机operator，用`clerk id <名字>`设置operator。
operator按工作树存于Git元数据目录的`clerk/identity`，不修改Git账号、负责人或历史发布者。
旧身份文件的迁移与冲突处理见[本机状态](../maintenance/local-state.md)。

```sh
clerk people list
clerk people add --person alice --email alice@example.com --name Alice
clerk people remove --email old-alice@example.com
clerk repos --check
clerk scope
clerk scope .clerk/data
clerk place contracts
```

人员映射把多个Git邮箱归并到稳定人员键，供作者显示和负责人字段校验。
删除映射之前检查仍使用该人员键的记录。仓库登记通过远端归一化匹配本机检出，
`repos --check`出现`missing`时检查检出位置与登记的`remotes`，不能只按目录名认仓。
`place`查类别到目录的映射，用于判断新资料应放哪里，不创建文稿；目录和类别映射
由维护者配置。`scope`查声明覆盖与治理等级，不是操作系统权限检查。

### 6.4 文件链接扫描与修复

文件移动会导致阅读路径失效；链接维护使用Git重命名证据生成修复方案。

```sh
clerk links check
clerk links scan --linkify --json
clerk links repair docs tasks --linkify --json
clerk links repair docs tasks --linkify --apply
clerk check
```

默认扫描Git跟踪的Markdown，显式路径可限制范围。`check`查现有链接，`scan`和
不带`--apply`的`repair`只输出位置、候选和替换方案。`--linkify`将能确认存在的
行内代码文件地址改为可点击链接，不转换任意代码标识符或命令。
已有链接和新文档请优先使用相对Markdown链接。

只有历史唯一且目标存在时自动修复；目标重命名和文档搬家都可利用历史。
同名复用导致歧义时报告`ambiguous`，缺证据或已删除时报告`missing`，需要人工判断。
浅克隆和Git未识别的移动可能无法恢复。生成区块报告`generated`，须修改源数据或视图
后`render`，不直接覆盖。修复保留章节片段和行号后缀，但不验证它们的语义是否仍正确。
应用走正式事务、封存与Git收尾，只允许写当前工作区，不支持请求模式。
外部URL、代码块及HTML注释不作为本地文件地址修复。

### 6.5 Agent与脚本集成

Agent实际调用与自动化脚本使用完整命令、动词、长选项和枚举值，不使用前缀或组合短选项。输出格式仅使用`--json`、`--tsv`、`--csv`、`--md`；旧`-j`移除，`-t`、`-c`、`-m`已重新分配，见[迁移说明](../features/terminal.md#33-不兼容迁移与agent规范)。

```sh
clerk explain tasks --json
clerk workflow show start --json
clerk context tasks APP-001 --json
clerk operation status --json
clerk completion powershell
```

机器声明导出字段写权限、取值约束、自动值、编号令牌、迁移、动作输入、关系和
当前提交策略；模板导出参数、默认值与步骤。脚本根据声明构造命令，执行时仍接受
当前规则验证。原始键用于存储和调用，显示名用于呈现，不能把表格显示名当作编号。

`completion powershell`输出PowerShell补全脚本，先检查再加载；
不改系统配置。脚本检测`$LASTEXITCODE`或进程退出码，不依赖终端列宽、对齐空格或
截断后的文本。各专用JSON结构不同，`version`报告格式版本，不能假设所有命令
都返回同一种`count/columns/data`结构。

### 6.6 声明维护与提交检查

新工作区先按第三章初始化，再逐步添加业务声明。维护分工如下：

| 需要改变的事实 | 维护位置与方式 |
|---|---|
| 合法人员、状态或分类 | 维护工作区词表，保留稳定键和引用完整性 |
| 对象字段、编号、迁移与动作 | 维护种类及编号声明，再运行`check` |
| 附件格式、必需章节、文稿索引 | 维护文稿模板、章节与索引声明 |
| 页面列、排序与组合区块 | 维护视图，`render --dry-run`核对后正式渲染 |
| 路径治理、同步与提交策略 | 维护工作区声明，核对兼容行为 |
| 常用过程和输出习惯 | 维护工作流模板与命令默认值 |

用户字段通常持续可写，创建期字段仅在`new`输入，工具字段由编号、自动值或动作
维护。字段类型、条件必填、唯一性、引用、状态前置条件和文稿结构约束都应在声明
中表达。新增规则需评估已有数据；不要通过手改受管表绕过工具。
配置格式与完整例子见[声明格式](../architecture/declarations.md)和[声明样例目录](../../examples/library)。

<a id="design"></a>

## 七、设计逻辑

### 7.1 声明、数据与呈现

```text
工作区声明 + 种类声明 + 词表
              |
              v
       命令校验与受控操作
              |
              v
      CSV记录 / Markdown文稿
              |
              v
        视图 -> 页面受管区块
```

| 概念 | 责任 | 例子 |
|---|---|---|
| 工作区 | 路径、治理与协作策略 | `.clerk/workspace.toml` |
| 种类 | 对象字段、身份、状态、动作与视图 | `tasks.toml` |
| 取值域 | 有稳定键与显示名的合法取值 | `state.csv` |
| 映射 | 把别名或账号对应到对象 | 账号归并 |
| 台账 | 记录不需要独立对象身份的事实 | 关注事项 |
| 记录 | 一行一个对象 | 任务CSV |
| 文稿 | 一个文件一个对象 | 契约Markdown |
| 页面 | 从数据投影出的阅读入口 | 现状页受管区块 |

键承担身份，显示名承担呈现，修改显示名不会改变引用关系。
同一个状态词表可同时约束输入并提供页面显示名。新增业务规则优先修改声明，
解析、读写、排序和渲染算法则留在程序里。完整概念见[概念模型](../architecture/model.md)。

### 7.2 写入边界

```text
写锁与中断恢复 -> 同步及旧封存检查 -> 校验与修改 -> 更新封存
-> 接受本地结果 -> Git提交与推送 -> 报告
```

写锁防止同一克隆的Clerk写命令同时分配编号或覆盖文件。文件修改前记录原内容，
接受本地结果前失败则恢复；进程被终止时，下次写命令先恢复未完成操作。
编号登记保留已使用身份，因此删除最高编号也不会让下一次新建复用它。

Git处在本地接受之后：推送失败不会撤销已经保存的数据或已有提交。
这也解释了为什么失败后需要先确认当前状态，再决定后续步骤。

### 7.3 治理与保证范围

`scope`展示实际声明的治理等级。`sealed`比对内容哈希，`validated`检查结构与取值，
`tracked`追踪内容哈希，`generated`表示由工具生成，`referenced`用于引用路径，
`free`不治理。未声明路径按自由处理，声明路径不会自动覆盖整个仓库。

当前写入恢复针对进程中断，不承诺断电或磁盘损坏恢复。只读命令与外部编辑器不参与
通用写事务；`context`有专门的本机快照锁，其余普通查询不能假设是跨文件一致快照。
应避免在批量写入中途编辑文件。不同克隆可能分配相同编号，
协作时仍需要同步、审核与Git冲突处理。更多边界见[架构复核](../architecture/review.md)。

<a id="troubleshooting"></a>

## 八、问题定位与后续阅读

| 现象 | 处理方式 |
|---|---|
| 找不到工作区 | 检查标记文件、当前目录、`CLERK_ROOT`和`--root` |
| 未知种类、动词或选项 | 运行`kinds`与对应`help`，按当前声明使用 |
| 声明加载失败 | 按错误定位文件与键，修正未知键、类型或词表引用 |
| 取值被拒绝 | 查看字段类型、词表键、必填与条件必填规则 |
| 状态迁移冲突 | 查看当前状态、迁移来源与前置字段 |
| 封存漂移 | 审查差异；恢复误改，或经审核后带理由重新封存 |
| 同步闸门阻止写入 | 抓取并核对远端，按仓库流程更新和解决冲突 |
| 请求基线冲突 | 核对目标及声明变化，重新提出适用于当前状态的请求 |
| 写锁竞争 | 等待当前写命令结束；不要删除锁文件或恢复日志来绕过 |
| 页面没有更新 | 检查目标视图声明，执行`render`与`check` |
| 命令失败但数据已变化 | 检查是否为Git阶段失败，确认本地数据与提交后完成收尾 |
| 缺少提交说明 | 检查`commit_mode`，提供本次完整说明，演练也按策略提供 |
| `held`等状态无法恢复 | 查看迁移声明和可达性；维护者修正配置后再执行，不手改状态 |
| `context`缺少阶段日志 | 使用`logs add --tasks`显式关联，旧日志需完整编号提及 |
| 会话报告范围重叠 | 查看对方任务、分支和最后活动，协调范围；过期记录不再占用 |
| 链接无法自动修复 | 查看`missing/ambiguous/generated`及Git历史，按原因人工修正或重渲染 |
| 版本号相同但行为不同 | 对比`version --json`的构建提交与未提交标记 |

退出码适合脚本判断大类：`0`成功，`1`检查问题或读写失败，`2`用法或声明错误，
`3`取值校验失败，`4`状态冲突，`5`同步或请求基线冲突。细节以错误消息为准。
PowerShell中通过`$LASTEXITCODE`读取上一条命令退出码。

| 文档 | 进一步了解 |
|---|---|
| [声明格式](../architecture/declarations.md) | 完整字段、编号、动作、视图与策略配置 |
| [设计要求](../architecture/design.md) | 维护实现时的单一事实来源与分层规则 |
| [架构复核](../architecture/review.md) | 已修复问题、剩余缺陷与验证边界 |
| [协作工作流](../features/workflows.md) | 工作流步骤、同步和Git收尾恢复 |
| [接手与模板](../features/context-and-templates.md) | 上下文来源、模板参数、批次与机器声明 |
| [协作登记与文件引用](../features/collaboration.md) | 日志、会话、链接和版本身份 |
| [提交说明](../maintenance/commit-messages.md) | 三种提交策略、说明来源和重试兼容 |
| [场景指南](scenarios.md) | 从具体问题出发的完整使用过程 |

当前尚无统一受管资料全文搜索、Issue/PR远端状态追踪或内建Markdown视觉预览入口。
`list --filter`只查该种类字段，`context`只汇集指定对象资料，`render`只生成受管页面；
它们都不能代替上述功能。需要时通过Git检索、外部平台或Markdown阅读器完成。
