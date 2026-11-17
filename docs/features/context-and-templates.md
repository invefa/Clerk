# Clerk任务接手与工作流模板

- 日期：[20261101]
- 范围：上下文包、工作流模板、关注事项增删与机器声明

## 一、快速使用

在工作区根目录执行。编号、模板名与参数以当前声明为准，下面以已配置的设计工作区为例。

```sh
clerk context tasks APP-021
clerk workflow list
clerk workflow list --compact
clerk workflow show start
clerk workflow preview start --task APP-021 --description "任务范围与验收条件"
clerk batch --template start --task APP-021 --description "任务范围与验收条件" --dry-run
clerk batch --template start --task APP-021 --description "任务范围与验收条件"
clerk focus add APP-021
clerk focus remove APP-021
```

已经认领或开工的任务不能重复执行`start`模板；前置条件不满足时整批回滚。
先查看上下文与模板内容，按实际状态选择动作。模板不会自动跳过失败步骤。

终端列表、参数与上下文元数据默认对齐；`--compact`省略列补齐空格与表头分隔线。
附件摘录和模板定义用`|`标识每行原文，工具标题不使用Markdown标题语法。
`workflow preview`输出的可执行TOML、JSON等机器格式保持原有结构。
从属内容相对父级缩进两个空格，紧凑模式保留层级；工作区、仓库、关系、来源和模板参数共用此规则。

直接输出到终端的`context`及`workflow list/show`会探测stdout可见窗口宽度，
主动换行并保持续行缩进。摘录同时显示源文件行号，显示折行的编号栏留空；
模板定义展示文件原文，注释、顺序和行号都对应源文件。管道与重定向默认关闭这些排版，
仍保留原文标记。可显式选择：

```sh
clerk context tasks APP-021 --width 80
clerk context tasks APP-021 --plain
clerk workflow show start --width 80
```

`--width`指定总列宽，允许20到10000，显式指定时即使输出到管道也启用阅读排版；
`--plain`关闭主动换行和行号，两者互斥。`--compact`仍只控制空格补齐，
`--max-width`仍限制表格单元格。JSON与`workflow preview`不接受显式阅读选项；
命令默认值中的阅读选项在显式选择JSON或执行模板预览时跳过。
终端宽度无法探测时依次使用有效的`COLUMNS`与80列；不会因stderr连接终端而排版stdout管道。

例如，摘录中的Markdown标题带原文标记，工具标签使用冒号：

```text
Excerpts:
  brief:
    Source: tasks/APP-021.md:1
    Content:
      1 | # 任务说明
      2 |
      3 | ## 验收条件
      4 | 原文内容很长时，按可用宽度主动换行，
        | 续行不重复显示源行号。
```

## 二、任务上下文包

`context <kind> <key...>`同时支持单字段与复合主键，在办记录优先，找不到时查询归档。
它输出完整字段、声明的任务附件、双向一层关系、关联文稿摘要、最近五条日志与本机仓库观察。
关系包括声明关系与记录里的对象引用字段；关联契约须显式建立关系，正文中的提及只算文字。

```sh
clerk tasks link APP-021 contract contracts 28
clerk context tasks APP-021 --json
```

上述契约编号只是形状示例，须替换为`contracts list`中的现有主键。
上下文的关系输出标记`incoming`、`outgoing`或`reference`，同时标明目标的在办、归档状态。
只展开一层，最多五十条关系，避免循环依赖无限展开。附件最多四千字符，关联文稿两千字符，
每条日志一千五百字符；截断会保留原路径并标记`truncated`。

这些是字符数上限，不是固定行数。终端显示`Truncated: true`时，会同时提示使用`--full`读取完整正文：

```sh
clerk context tasks APP-021 --full
clerk context tasks APP-021 --full --overflow pager
clerk context tasks APP-021 --full --plain --color never
clerk context tasks APP-021 --full --json
```

`--full`取消附件、关联文稿和已选日志条目的正文长度限制，仍只展开一层、最多五十条关系和最近五条日志。`--overflow full`只控制终端窗口溢出，`--plain`只关闭阅读换行和行号；两者都不取消正文截断。

启用颜色时，行号与首行、续行分隔栏统一弱化；来源与观察信息弱化，截断和诊断使用警示色。记录字段沿用种类的`terminal_style`声明，不根据状态名称猜测颜色。原始Markdown中的冒号和标题保持原文。`--color never`禁用颜色，JSON始终不含ANSI控制序列。

日志按完整编号匹配，`APP-02`不会匹配`APP-020`。按日志文件日期与小节时间倒序取最近五条，
标记为`log text mention`，保留文件和小节起始行，不把文字提及升级成正式关系。
缺失附件、无效关系、缺失仓库和截断都保留诊断，已有资料仍会输出。

上下文不抓取远端、不接受业务写入、不创建执行回执。它取得本机Clerk锁并重新加载声明；
发现中断写入的撤销日志时拒绝读取，提示先由写命令恢复。锁文件属于本机Git元数据，
不进入提交。外部编辑器和其他仓库不遵守此锁，其内容是`observed_at`时的观察。
远端引用明确标记`cached = true`，不能据此判断服务器上是否又有提交。

JSON格式版本为`1`，包含`record`、`parts`、`relations`、`repositories`、`sources`和
`diagnostics`。来源列表给出文件路径与原内容SHA-256，工作区观察包含当前HEAD、分支与
未提交状态，便于接手者核对资料是否已过时。机器输出保留原字段名与原始键。

种类声明的配置如下，字段名可以替换，不要求种类名是`tasks`：

```toml
[context]
attachments = ["brief", "completion"]
repos = "repos"
logs = "logs"
```

`attachments`必须指向路径字段，`repos`必须指向带词表的字段，词表登记使用`remotes`列；
本机检出通过远端归一化匹配。`logs`必须是文稿种类。未声明`[context]`的种类仍可查询记录
与关系，并提示尚未配置附件、仓库和日志来源。

## 三、声明工作流模板

### 3.1 模板与参数

模板放在与种类目录并列的`workflows/`，通常是`.clerk/config/workflows/`。
每个TOML文件一个模板，文件名就是命令中的名字。样例见
[examples/library/workflows](../../examples/library/workflows)，部署时按工作区实际动作调整。

```toml
version = 1
title = "开工"

[[param]]
name = "task"
type = "ref"
target = "tasks"
required = true

[[param]]
name = "description"
type = "text"
required = true

[[step]]
kind = "tasks"
verb = "start"
keys = ["${params.task}"]

[[step]]
kind = "tasks"
verb = "brief"
keys = ["${params.task}"]
body = "${params.description}"

[[step]]
kind = "focus"
verb = "add"
keys = ["${params.task}"]

[[step]]
verb = "render"
```

参数复用字段解析器与取值检查，支持`type`、`required`、`default`、`multi`、`registry`、
`target`与`display`。引用可以指向对象或身份词表，提交使用`commit`类型，证据使用`path`。
参数名不能占用已有命令选项；用户字段权限不适用于参数本身，最终写入仍遵守目标字段权限。

`${params.name}`在已解析的字符串值里替换，不拼接TOML源码。引号与换行不会产生新步骤。
参数中的`${alias.field}`保持字面内容；预览中的`$${`是供执行器保留字面`${`的转义。
普通工作流也支持该转义；键开头的`@@`表示字面`@`，单个`@alias`仍引用此前新建步骤的主键。
编号别名与参数分别使用`${alias.field}`和`${params.name}`，`params`不能作为步骤别名。
正文文件相对模板文件解析，预览转换为绝对路径；`file = "-"`仍不支持嵌套标准输入。

### 3.2 预览、演练与执行

| 入口 | 行为 |
|---|---|
| `workflow list` | 列出模板与标题 |
| `workflow show <name>` | 显示参数、默认值与步骤 |
| `workflow preview <name> --参数 值` | 校验参数并输出可执行TOML，不分配编号、不改数据 |
| `batch --template <name> --参数 值 --dry-run` | 在真实事务中演练，执行检查后回滚 |
| `batch --template <name> --参数 值` | 复用`batch`，一次事务、逐步审计、一次Git收尾 |

这些入口均支持`--json`。预览不检查会随执行变化的状态前置条件，必须用演练检查整批行为。
演练和实际执行都要求干净工作树；模板文件应先提交。预览无此要求，也不允许`--sync`。
演练可能留下本机执行回执，不保留业务文件和编号分配。具体事务与Git失败处理见
[协作工作流](workflows.md)。

设计工作区的三个模板：

| 模板 | 参数 | 步骤 |
|---|---|---|
| `start` | `task`、`description` | 认领、开工、任务说明、加入批次、日志、渲染 |
| `review` | `task`、`report` | 保存完成材料、送审、日志、渲染 |
| `finish` | `task`、`report`、`evidence`、`commit` | 完成说明、完成迁移、移出批次、日志、渲染 |

完成模板仍要求用户提供真实证据与提交，不推断外部PR是否合并。送审只推进任务状态并保存
材料；工作区若要求独立复核对象，可以在模板中增加`reviews new`步骤。

## 四、关注事项增删

`focus set`替换整个集合，`focus add/remove`只改指定成员。增删在写锁内重读集合，保留
其他成员及其顺序；新增成员排到末尾。重复新增与移除不存在成员都是成功的空操作，
不为相同集合创建空提交。输入中的重复编号只处理一次。

`add`与`set`要求编号存在于某个单字段主键种类的在办记录中；错误输入整批拒绝。
`remove`允许移除已经失效的编号，便于清理旧批次。增删不自动认领或推进任务状态。

```sh
clerk focus add APP-021 APP-022 --dry-run --json
clerk focus remove APP-021 --json
```

JSON回执区分`added`、`removed`和`unchanged`，并返回最终`keys`。工作流里的
`kind = "focus"`步骤使用同一执行入口，支持`set/add/remove`，只接受`kind`、`verb`与
`keys`。本机锁不能避免不同克隆的Git冲突，冲突仍需同步与合并处理。

## 五、机器声明

```sh
clerk explain tasks --json
clerk workflow show start --json
```

种类导出包含格式版本、字段权限、条件必填、自动值、引用目标、编号令牌、动词输入、
动作、迁移、关系与完整声明。模板导出包含参数约束、默认值、来源和未展开步骤。
Agent可以据此构造结构化输入，最终执行仍由工作区当前声明校验。

活动会话、日志显式关联、版本身份与文件引用维护见[协作登记与文件引用](collaboration.md)。
全文搜索、外部Issue与PR状态追踪和Markdown预览入口尚未提供。缓存远端引用不代表实时远端状态。
