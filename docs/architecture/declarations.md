# 声明格式

- 状态: 草案
- 日期: [20261031]
- 范围: 工作区入口、词表、种类声明、编号方案与命令面
- 读者: 使用工具的用户与后续实现者

## 一、工作区入口

工作区以`.clerk/workspace.toml`为标记。路径解析顺序为`--root`参数、环境变量`CLERK_ROOT`、自当前目录向上查找。

```toml
version = 1
name = "design"

[time]
offset_hours = 8

[layout]
registry = ".clerk/config/registry"
kinds = ".clerk/config/kinds"
data = ".clerk/data"
status = "status.md"
seal = ".clerk/seal/seal.csv"
seal_log = "log/seal-log.md"
identity = ".work/identity" # 仅用于迁移旧operator

[policy]
auto_commit = false
auto_push = false
commit_mode = "auto"
commit_message = "chore(clerk): update managed state"

[sync]
enabled = true
remote = "origin"
branch = "main"
fail_closed = false
write_fetch = false
read_fetch = false
cache_seconds = 0
```

| 键 | 说明 |
|---|---|
| `version` | 只接受`1`，其他值直接报错 |
| `name` | 工作区名字，缺省取根目录名 |
| `time.offset_hours` | 记录时间戳使用的固定偏移，缺省`0` |
| `layout.*` | 各类路径，相对工作区根目录，未写时用缺省值 |
| `policy.auto_commit` | 写命令之后是否自动提交，缺省关闭 |
| `policy.auto_push` | 提交之后是否推送 |
| `policy.commit_mode` | `required`要求调用方提供说明；`auto`按成功操作和受影响文件生成，缺省值；`default`使用固定说明 |
| `policy.commit_message` | `default`模式的固定说明，可含多段正文；仅设置旧键而未设置模式时保持旧行为 |
| `policy.push_remote`、`policy.push_branch` | 推送目标，缺省`origin`与`main` |
| `sync.enabled` | 写命令前是否核对远端，缺省开启 |
| `sync.remote`、`sync.branch` | 核对对象，缺省`origin`与`main` |
| `sync.fail_closed` | 远端分支本地未知、或写前抓取失败时是否拒绝写入，缺省关闭 |
| `sync.write_fetch` | 写命令核对前是否先抓取远端，缺省关闭 |
| `sync.read_fetch` | 读命令是否也抓取，缺省关闭 |
| `sync.cache_seconds` | 抓取结果复用秒数，缺省不复用 |

未知键一律报错，拼错的键不会被静默忽略。

### 1.1 日志

工作记录按维护方与日期分文件，命令直接追加。目录、归档目录与页头模板都由`logs`种类声明：

```toml
# .clerk/config/kinds/logs.toml
dir = "log"
archive = "log/archive"
header = [
  "# [{date}] {maintainer} 工作记录",
  "维护方：{maintainer}",
  "操作者：{operator}",
]
```

`header`是新页的头部模板，写成`.clerk/templates/log-page.md`这样的文件路径即可，占位符为`{date}`、`{maintainer}`、`{operator}`，缺省用英文写法。`[layout]`不再有日志相关键。模板文件放在`.clerk/templates/`，一件模板一个文件，改样式只改文件。

```sh
clerk logs add --title 标题 [--side 维护方] [--body 正文 | --file 路径或-]
```

`add`写入`log/<维护方>/<日期>.md`，首次创建时按`header`写头部，随后每条记录是一节`## [T时分秒] 标题`。`list`列出文件与条目数，`archive`把早于给定日期的文件移入归档目录。两条写命令都会重算受影响路径的封存并按策略提交。

### 1.2 同步闸门

`clerk scope`只列出声明过的路径，未声明即按自由处理，新增受管目录时要同时补`[[path]]`。`validated`路径下不属于任何文稿种类的 Markdown 会检查链接与表格列数；`generated`路径由工具生成，不参与封存比对。

写命令开始前比较本地分支与远端分支：远端有而本地没有的提交数大于零时拒绝执行，退出码 5，提示先拉取。远端分支在本地不存在时按`sync.fail_closed`决定拒绝还是放行，放行时`clerk sync check`会报未知。

```sh
clerk sync check
clerk sync fetch
```

`sync check`只比对本地已有的远端引用，不发起网络请求；`sync fetch`取回引用之后比较才反映远端现状。

### 1.3 受管路径

受管路径用`[[path]]`表声明，决定哪些路径纳入封存与提交。

```toml
[[path]]
path = ".clerk/data"
level = "sealed"

[[path]]
path = "status.md"
level = "sealed"
regions = ["tasks", "assets"]
```

| 键 | 说明 |
|---|---|
| `path` | 相对工作区根目录的文件或目录 |
| `level` | `generated`、`sealed`、`validated`、`tracked`、`referenced`、`free`，缺省`free` |
| `regions` | 只封存文件内指定标记区间，用于用户手写正文与生成区块共存的页面 |

写命令结束时，命中`sealed`规则的受影响路径会重算哈希并写入封存表；`regions`非空时按区块逐个记录，键写作`文件#区块`。封存表自身的路径由`layout.seal`给出。

## 二、词表

词表放在`layout.registry`目录下，一个文件承担一种角色：

| 角色 | 首列 | 显示名 | 用途 |
|---|---|---|---|
| 取值域 | `key` | 必须有`display` | 字段与状态的取值来源 |
| 映射 | 左值，例如`alias` | 不要 | 别名到键、类别到目录 |
| 台账 | 不声明 | 不要 | 账号归并、受限词与分类 |

映射与台账只要求整行不重复；取值域要求键唯一。视图引用的表可以是任何一种，字段与状态引用的表必须是取值域。

```csv
key,display
todo,待开始
active,进行中
done,完成
```

| 规则 | 内容 |
|---|---|
| 身份列 | 恒为第一列；取值域里叫`key` |
| 显示名列 | 恒为`display`；只有取值域需要，`check`会报告缺列的被引用域 |
| 一域一文件 | 不把多个域压缩进一张表的取值单元格，不设`axis:row`取法 |
| 不设旁路文件 | 显示名写在域文件里，不另建映射表 |
| 多值归字段 | 多值由字段声明的`multi`决定，域不声明 |
| 键英文，显示名中文 | 键用于数据与命令行，显示名只用于渲染与`--display` |

别名是独立的列表，列`alias,key`，把旧编号映射到键；`check`会报告指向不存在键的别名。

显示名只在呈现场合生效：`clerk ... --display`，以及没有声明`display = false`的视图。缺显示名时退回键本身。


## 三、种类声明

种类放在`layout.kinds`目录下，文件名即命令名，一类一个文件。

```toml
title = "任务"
table = ".clerk/data/tasks.csv"
key = ["id"]
numbering = "tasks"
relations_table = ".clerk/data/relations.csv"

[state]
field = "state"
registry = "states"
default = "todo"

[[field]]
name = "title"
type = "text"
required = true

[[field]]
name = "state"
type = "enum"
registry = "states"
role = "status"

[[field]]
name = "updated"
type = "stamp"
role = "status"
auto = "now"

[[transition]]
name = "complete"
to = "done"
from = ["active", "review"]
requires = ["evidence", "commit"]
```

顶层键：

| 键 | 说明 |
|---|---|
| `title` | 显示名，缺省取文件名 |
| `description` | 一句话说明，显示在帮助总览的`about`列与种类页开头 |
| `storage` | `table`或`document`，缺省`table` |
| `table` | 数据表路径，`table`存储必填 |
| `dir` | 文档目录，`document`存储必填 |
| `filename` | 文档文件名模板，缺省`{id}.md`；同时用于识别，模板里的占位符会成为字段 |
| `key` | 主键字段，缺省`["id"]` |
| `numbering` | 编号方案名，对应入口文件的`[ids.<name>]` |
| `relations_table` | 关系表路径，声明关系时必填 |
| `archive_dir` | 归档目录，缺省`.clerk/archive` |
| `meta` | 元信息样式：`block`缺省，或`bullets` |

### 3.1 内容哈希

资源类对象登记的是外部文件，工具按内容哈希核对，避免同名不同内容或同内容重复登记。

```toml
[hash]
file = "path"
digest = "sha256"
```

| 键 | 说明 |
|---|---|
| `file` | 存放工作区相对路径的字段 |
| `digest` | 存放十六进制摘要的字段，工具维护 |
| `algorithm` | 只接受`sha256` |

新建实例时工具读取`file`指向的文件并写入`digest`；文件缺失直接拒绝。修改`file`字段会重算摘要。`verify`动词逐条重算并比对，不符或文件缺失都计入问题，命令以退出码 1 结束。`digest`字段应声明为`role = "status"`，不允许从命令行写入。

公开仓审计会读取所有声明了`[hash]`的种类，把登记的路径当作允许的二进制清单；暂存区里未登记的二进制会被拦下。

### 3.2 模板与结构校验

文档种类可以从模板文件起稿，并声明必须出现的章节与元信息键。

```toml
template = "templates/review.md"

[sections]
required = ["结论", "依据与方法", "发现"]

[metadata]
required = ["date", "scope"]
strict = true
```

| 键 | 说明 |
|---|---|
| `template` | 正文骨架路径，新建时读取并用`{字段}`占位符替换；`--body`优先 |
| `header` | 新建页面时的头部模板，内联文本或以`.md`结尾的模板路径；占位符`{date}`、`{maintainer}`、`{operator}` |
| `sections.required` | 必须出现的章节，按标题文本包含匹配，任意级别标题都算 |
| `metadata.required` | 必须非空的元信息键 |
| `metadata.strict` | 为真时，元信息里出现未声明的键也算问题 |

`check`对每个文档执行这四项与字段校验：章节缺失、元信息缺失、未声明键、正文链接不可解析。链接以文档所在目录为基准解析，跳过`#`锚点、`http(s)://`与`mailto:`，跨仓相对路径（`../../library-app/...`）按同一规则解析。
### 3.3 索引页

文档种类可以声明一个索引页，新建时追加一行，归档时删除该行。

```toml
[index]
file = "contracts/README.md"
columns = ["file", "title"]
```

`columns`里`file`表示文档自身，写成相对链接；其余列取字段值。`check`比对索引与目录：行指向的文件必须是本文档，文档也必须有对应行；指向其他目录的链接不参与比对。索引页本身不算实例，即使它的文件名符合模板。
`table`存储一行一实例；`document`存储一文件一实例，元信息写在文件内的区块里：

```markdown
# 标题

<!-- clerk:meta
id = CON-01
state = draft
-->

正文由用户手写，命令只改区块内的键值。
```

区块外的正文在改写元信息时逐字保留，标题行由`title`字段重新生成。

`meta = "bullets"`时元信息写成标题之后的一组`- 标签: 值`列表，空行结束该组，与现有文档一致：

```markdown
# 预约规则复核

- 日期: 20261027
- 范围: 服务端与公开客户端

## 一、结论
```

写入用字段的`display`，读取时按`display`回映射到字段名，因此中文显示名与英文键可以并存。值外层的中括号会被去掉，形如`[T185600]`的时间去掉`T`前缀后存储。

文件名模板同时是识别规则：模板声明的占位符从文件名解析成字段，不符合模板的文件不算实例，因此同目录的索引页不会被当成对象。主键有多个字段时，地址写成各字段值用空格连接，例如`clerk reviews show 20261027 185600`。

字段键：

| 键 | 说明 |
|---|---|
| `name` | 列名 |
| `type` | `text`、`int`、`bool`、`enum`、`date`、`stamp`、`path`、`commit`、`email`、`ref`；`path`允许`../<仓库>/...`跨仓引用，拒绝绝对路径与带盘符的路径 |
| `role` | `spec`表示用户填写，`status`表示工具维护；缺省`spec` |
| `write` | 用户字段允许写入的阶段：`create`只允许新建，`always`缺省允许后续修改；工具字段不得配置 |
| `multi` | 多值，存储时用竖线分隔 |
| `registry` | `enum`的取值域，或`ref`引用的词表首列；其他类型不允许 |
| `target` | `ref`引用的种类名，目标必须使用单字段主键 |
| `required` | 是否必填 |
| `required_when` | 条件必填，写法为`state == done` |
| `default` | 缺省值 |
| `auto` | 自动取值：`git_name`、`git_email`、`person`、`operator`、`today`、`clock`、`now`；带`auto`的字段必须声明`role = "status"` |
| `display` | 显示名，缺省取字段名 |

引用字段必须在`registry`与`target`中选择一个来源，不能省略或同时指定。
批量`new --from`给错误补充CSV行号，同时保留原错误类别；字段取值错误退出码为`3`。
例如负责人使用`type = "ref"`与`registry = "person"`，身份账号台账可以有重复的身份键，
不需要增加显示名或维护第二份身份名单；任务里程碑使用`type = "ref"`与`target = "milestones"`。
引用只接受原始键，多值逐项检查；对象目标包括在办与归档实例，归档不破坏引用。
自由文本不承担引用约束。删除目标或身份登记后，`check`会报告悬空引用；当前不会级联删除
或自动改写引用方。复合主键引用尚不支持，声明在加载时拒绝。

状态与迁移：

| 键 | 说明 |
|---|---|
| `state.field` | 承载状态的字段 |
| `state.registry` | 状态词表 |
| `state.default` | 新建时的初始状态 |
| `transition.name` | 命令名，缺省取目标状态 |
| `transition.from` | 允许的来源状态，至少一个 |
| `transition.requires` | 迁移前必须非空的字段 |
| `transition.archive` | 为 `true` 时，进入该状态后可用`archive`动词归档 |

动作：

状态迁移只改状态，其他需要前置条件的写操作用动作声明。

```toml
[[action]]
name = "claim"
handler = "identity"
fields = ["claimant", "claimant_email", "claimed_at"]
states = ["todo", "active"]
require_empty = ["claimant"]
require_clean = false
```

| 键 | 说明 |
|---|---|
| `handler` | `append`按`heading`模板往文档页追加小节；`prune`按`--before`把旧页移入归档目录，标题取`--title`，正文取`--body`或管道；`identity`按名字填`fields`（`*_email`取git邮箱，`*_at`取当前时间戳，其余取映射后的姓名）；`clear`清空`fields`；`attach`把`--body`或`--file`的内容写进`filename`展开的路径并把该路径记到`field`；`set`按`values`写字段 |
| `fields` | `identity`与`clear`作用的字段，必须已声明 |
| `field`、`filename` | `attach`用的目标字段与文件名模板，例如`tasks/{id}.md` |
| `states` | 允许执行的状态，空表示不限 |
| `require_empty` | 执行前必须为空的字段 |
| `requires` | 执行前必须非空的字段 |
| `require_clean` | 为真时要求工作区没有未提交改动 |
| `values` | `set`处理器的字段值，值为模板；`{identity}`、`{git_name}`、`{git_email}`、`{operator}`、`{today}`、`{now}`与记录字段可替换 |

`set`示例：

```toml
[[action]]
name = "sign"
handler = "set"
requires = ["claimant"]
values = { evidence = "{identity}", signed_at = "{now}" }
```

模板只要以`.md`结尾就按工作区相对路径读取文件，例如`heading = ".clerk/templates/log-entry.md"`；否则按内联文本处理。页面头部`header`、追加标题`heading`、行模板`template`与`values`里的值都遵循这一条。

动作与迁移共用命令面：声明`name = "claim"`就得到`clerk tasks claim <键>`。文档存储支持`append`与`prune`，其余处理器按记录处理。
关系：

```toml
[[relation]]
name = "blocked"
target = "tasks"
```

关系表三列`from,kind,to`，`kind`是关系名，两端存实例键。关系表路径由种类的`relations_table`给出。

视图：

```toml
[[view]]
name = "list"
columns = ["id", "title", "state", "updated"]
```

`list`视图决定列表命令的列与表格输出，缺省输出全部字段。

带`target`与`region`的视图会被`clerk render`写入页面区块：

```toml
[[view]]
name = "status"
renderer = "table"
target = "status.md"
region = "tasks"
columns = ["id", "title", "state"]
```

渲染只替换`<!-- clerk:begin tasks -->`与`<!-- clerk:end tasks -->`之间的内容，区块外的用户正文不动。区块缺失时命令报错并给出应有的标记，`check`也把缺失区块记为问题。

多个视图声明同一个`target`与`region`时，它们的区块按声明顺序拼接进同一区块，用来拼出复合区块。因此一页的多个区块只需各自声明，`render`一次写入全部。

渲染器：

| 渲染器 | 输出 |
|---|---|
| `table`（缺省） | 本文档记录的Markdown表格，表头取`headers`或字段`display` |
| `sort` | 显示顺序的排序键，依次比较；列名之外可用`字段:部件`取编号的某一段，如`id:prefix`、`id:serial`；枚举列按其词表的`order`列排序，没有该列时按键排序。命令行`--sort`接受同样的键，写了就覆盖视图声明，两者走同一实现。并列时按编号的`前缀`与`序号`兜底；没有声明排序的编号种类也按这两段排 |
| `lines` | 每条记录按`template`展开成一行，占位符用存储的原值；声明`detail_field`时在该行下缩进一级再写一行`字段显示名: 值` |
| `composite` | 按`fragments`顺序拼接其他视图，每个片段可带`title`写成三级标题 |
| `registry` | 把一张词表渲染成表格，`source`给词表名，`columns`取`key`、`display`或列名 |
| `focus` | 读`source`指向的编号清单，逐条查出本文档记录并成表 |
| `filetable` | 把`source`指向的任意CSV渲染成表格，用于关系表这类旁路数据 |
| `details` | 每条记录一个三级标题块，路径字段渲染成链接；`logs = true`时再列出提到该编号的日志文件，前缀由`logs_prefix`给出 |
| `vocabularies` | 把多套词表合并成一张表，`entries`每行声明`display`与`registry`或`kind`，`multi`决定多填列；取值分隔符由`separator`给出，多填列的两种写法由`multi_display`给出 |

视图选项按渲染器校验，写在别的渲染器上的键会报错。视图只影响渲染出来的文档；命令行输出默认用存储的键，需要显示名时加`--display`，表头也随之改成字段显示名。

复合视图用`fragments`按序拼接，片段可带`title`写成三级标题；复合视图是现状页的拼装方式：`fragments = ["frag-prefixes", "frag-list", ...]`，片段视图不声明`target`，只声明`title`、`renderer`与`columns`。

`display = false`时表格的枚举列显示存储键而不是显示名，适合现状页这类以键为准的页面；表头仍然取`display`。



目标文件对应的受管路径为`generated`时不需要区块：`render`整份重写该文件，`check`比对内容与渲染结果，不一致即报错。

## 四、编号方案

编号方案写在工作区入口文件里，供种类按名字引用。

```toml
[ids.tasks]
pattern = "{prefix}-{serial:3}"
counter = "serial"
start = 1
sort = ["serial"]

[ids.tasks.tokens]
prefix = "prefixes"
```

| 键 | 说明 |
|---|---|
| `pattern` | 标识形状。数字段写宽度，例如`{serial:3}`；非数字段必须在`[tokens]`里登记来源词表 |
| `counter` | 自增段的名字，缺省`serial` |
| `start` | 空表时分配的第一个值，缺省`1` |
| `sort` | 排序用的段，缺省取`counter` |
| `tokens` | 段名到词表名的映射，取值必须在词表中的值 |

分配规则：扫描表中已有的键，取同一取值组合下的最大计数加一；表为空时用`start`。编号一经分配不复用，重名直接报错。

## 五、命令面

命令面分三层：种类动词由声明生成，内建命令由固定表驱动的帮助页描述，全局选项由解析层统一处理。

内建命令与子命令、通用选项、短写、默认值资格及种类动词统一由`command`层描述。
解析、帮助、补全与默认值校验都读取同一份描述；上下文加载不再依赖CLI模块。
默认值由[Context::command_defaults](../../src/execution/context.rs)统一展开，命令与动词的唯一前缀先解析再查默认值。
执行层保留完整记录，列选择只在输出层执行，避免`--columns`和排序丢失列表视图之外的字段。
未知的带值选项同样拒绝，只有已声明字段、编号令牌与所用动作的输入可以进入执行层。
请求重放也校验动词与令牌，只能执行声明支持的写动词；不接受未声明的旧别名。
开关不接受`--quiet=false`这样的取值形式，关闭开关使用`--no-quiet`。

默认值的类型在加载时校验：开关必须是布尔值，列与排序可以是非空字符串数组，宽度是
非负整数，表头为`show`或`hide`；普通字段按声明类型与词表校验。布尔字段的默认值
作为字段值传递，不转成开关；空字符串也保留为字段值。

帮助的出口只有一页，四种写法到达同一处：

| 写法 | 输出 |
|---|---|
| `clerk`、`clerk help`、`clerk --help`、`clerk -h` | 总览：用法、内建命令、种类、全局选项 |
| `clerk help <名字>` | 该名字的页 |
| `clerk <名字> --help`、`clerk <名字> -h` | 同一个页 |
| `clerk <种类> <动词> --help` | 仍取种类页；动词没有独立页 |
| `clerk <种类>` | 简短使用说明：用法、动词分组、示例，退出码 0 |

种类页的动词、字段、迁移、动作、关系与视图全部取自声明，末尾附全局选项与动作专用选项（如`--body`、`--file`、`--title`、`--side`），不会与实现漂移。不在开关表里的选项按用法错误报出，拼错不会静默失效。

```sh
clerk <kind> list [--<field> <value> ...] [--json|--tsv|--csv] [--columns a,b]
clerk <kind> show <key>...
clerk <kind> new --<field> <value> ... [--prefix APP] [--dry-run]
clerk <kind> set <key>... --<field> <value> ...
clerk <kind> <transition> <key>... [--<field> <value> ...]
clerk <kind> link <key> <relation> <kind> <key>
clerk <kind> unlink <key> <relation> <kind> <key>
clerk <kind> archive <key>...
clerk <kind> restore <key>...
clerk <kind> verify
clerk <kind> delete <key>... --force
```

`delete`做硬删除：表存储删行，文档存储删文件并从索引页移除行；必须带`--force`，否则拒绝。需要留痕的删除用归档。

归档去向：表存储写入`<archive_dir>/<kind>.csv`，文档存储移入`<archive_dir>/<kind>/`。只有声明了`archive = true`的迁移所到达的状态允许归档，其他状态拒绝并返回退出码 4。`list --data-scope archived`读取归档而不是在办数据。

工作区命令：

| 命令 | 说明 |
|---|---|
| `whoami` | 显示git身份、映射后的姓名与操作者 |
| `id <name>` | 写入本机操作者标签 |
| `kinds` | 列出全部种类 |
| `explain <kind>` | 显示一个种类的字段、迁移、关系与视图 |
| `check` | 按声明校验各数据表、封存与页面区块 |
| `doctor` | 检查工作区、词表、身份与表文件 |
| `scope` | 列出受管路径与治理强度 |
| `seal` | 手工改动受管文件之后重算封存并留痕，必须给`--note` |
| `init` | 补齐缺失的目录、标记文件与现状页，可重复执行 |
| `place <类别>` | 按`paths`词表给出该类内容应放的目录 |
| `people list\|add\|remove` | 维护身份映射表 |
| `repos [--check]` | 列出仓库登记，`--check`核对本机检出 |
| `brief` | 按种类汇总状态计数与当前批次（`config/registry/focus.csv`，单列`key`） |
| `focus set\|show` | 维护当前批次（`config/registry/focus.csv`，单列`key`）清单 |
| `completions bash\|zsh\|powershell` | 输出补全脚本 |
| `help [种类]` | 总览，或某个种类的动词与字段 |
| `links check [路径...]` | 检查 Markdown 链接能否解析，缺省扫全工作区的`.md` |
| `render` | 把声明了目标与区块的视图写进页面 |

通用选项：`--root`、`--as`、`--columns`、`--max-width`、`--compact`、`--json`、`--tsv`、`--csv`、`--dry-run`、`--request`、`--data-scope archived`。

### 5.1 请求

没有写权限的用户不直接改共享表，而是把一次写操作捕获成请求文件：

```sh
clerk tasks new --title 标题 --request add-task
clerk tasks complete APP-001 --evidence a.md --commit abc1234 --request finish-a
```

请求文件落在`<layout.requests>/pending/`。新版格式为版本2，在种类、动词、键、字段值、编号令牌、请求者、基线提交与时间之外，保存正文`body`和删除确认`force`；旧版请求仍可读取。`--file`或标准输入的正文在捕获时读入请求，应用时不再读取原路径。`--request`不能与`--from`或`--dry-run`组合。有写权限的用户在合并之后执行：

```sh
clerk request list
clerk request show add-task
clerk request apply
clerk request apply add-task
clerk request reject add-task --reason 理由
```

`apply`要求基线是当前提交的祖先，并比较声明与目标记录。目标变化时以退出码5停止；记录按完整记录比较，文档操作保守比较受影响目录。同批请求先统一检查，再在同一事务中执行；任一失败整批回滚。`--dry-run`在锁内演练整批并恢复原文件。

成功时，数据、封存、请求删除和`applied.log`回执纳入同一次本地操作与自动提交。回执保存请求原始内容的SHA-256，重新提交相同内容会被拒绝。同批相同内容也会被拒绝；修改过内容的请求视为新的请求。`reject`的删除与`rejected.log`同样走写管线。

### 5.3 命令默认值

`[commands]`给命令配省缺行为，键是命令名，或`命令.动词`；显式参数永远优先。

```toml
[commands."tasks.list"]
state = "active"
sort = "state,id:prefix,id:serial"

[commands."tasks.new"]
prefix = "APP"
type = "ability"

[commands."check"]
quiet = true
```

| 项 | 规则 |
|---|---|
| 查找顺序 | 先`<命令>`，再`<命令>.<动词>`，同名键后者覆盖 |
| 布尔`true` | 当作开关打开 |
| 布尔`false` | 不产生默认项，不能用来关掉命令行给的开关 |
| 字符串、整数 | 当作取值 |
| 数组 | 用逗号连接，适合`columns`、`sort` |
| 优先级 | 命令行 > 动词默认 > 命令默认 > 视图声明 > 内建缺省 |
| 不允许 | `force`、`dry-run`、`request`、`help`、`version`、`root` |
| 校验 | 命令名、动词、字段名与选项名在加载时校验，未知键直接报错 |

默认值在帮助页的`Defaults`段列出，来源与执行共用同一张表。

取消办法：开关用`--no-<开关>`覆盖，例如默认开了`quiet`，写`--no-quiet`即可；取值项直接给新值，例如默认`headers = "hide"`，写`--headers show`即可。配置里的`false`表示不设置，不能用来关掉命令行给的开关。

表格输出的表头由`--headers show|hide`控制，缺省显示；`--md`与`--json`等机器可读格式不受影响。

终端表格与键值块默认按显示宽度对齐；`--compact`省略列补齐空格与表头分隔线，
`--no-compact`恢复默认对齐。`compact = true`可写入命令默认值；不改变机器格式或受管文档。
### 5.0 短旗标

短名供用户手工输入，长名与短名由共享命令元数据一一映射，不随命令变化。Agent与脚本使用完整命令、动词、选项及枚举值。输出格式只保留`--json`、`--tsv`、`--csv`、`--md`，不提供短名。

| 短 | 长 | 短 | 长 |
|---|---|---|---|
| `-h` | `--help` | `-l` | `--long` |
| `-v` | `--version` | `-s` | `--sort` |
| `-q` | `--quiet` | `-r` | `--desc` |
| `-b` | `--batch` | `-f` | `--filter` |
| `-t` | `--template` | `-d` | `--display` |
| `-c` | `--columns` | `-p` | `--paging` |
| `-S` | `--data-scope` | `-o` | `--overflow` |
| `-m` | `--message` | `-M` | `--message-file` |
| `-i` | `--file` | `-C` | `--root` |
| `-w` | `--width` | `-D` | `--details` |
| `-F` | `--full` | `-n` | `--dry-run` |
| `-P` | `--preview` | | |

解析规则：

| 写法 | 结果 |
|---|---|
| `-rq` | 合并为`--desc`与`--quiet` |
| `-s state`、`-f "state = done"` | 有值短选项消费下一个参数 |
| `-f=state = done` | 有值短选项使用等号内联 |
| `-rs title` | 先设置desc，再由sort消费下一个参数title |
| `-rSall` | 先设置desc，再由data-scope消费余部all |
| `--` | 结束选项解析，余下均为位置参数 |
| `-j`、`-z` | 未知短名报错 |

本次重分配不兼容旧输出格式短名，`-t`、`-c`、`-m`已有新含义。迁移表与完整规则见[用户短名与Agent规范](../features/terminal.md#32-用户短名)。`--force`、`--sync`、`--request`与`--max-width`等继续保留长名。

### 5.1 列表过滤

字段旗标是等值过滤的简写，复杂条件走`--filter`，一个参数一条表达式：

```sh
clerk tasks list --filter "state = done"
clerk tasks list --filter "state in (todo, active)"
clerk tasks list --filter "title ~ 预约 and state != done"
clerk tasks list --filter "updated >= 20261001 and updated <= 20261031"
clerk tasks list --filter "(state = done or state = review) and owner = alice"
```

| 元素 | 写法 |
|---|---|
| 等值 | `state = done`；枚举可写显示名 |
| 不等 | `state != done` |
| 包含文本 | `title ~ 预约`、`title !~ 预约`，忽略大小写，键与显示名都参与 |
| 比较 | `> >= < <=`，按存储值做字符串比较，适用于日期与时间戳 |
| 集合 | `state in (todo, active)` |
| 组合 | `and`、`or`、括号 |

字段名写在表达式里，不再受命令行旗标命名限制；`--filter`本身是全局取值，任何种类都能用。
## 六、退出码

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 检查未通过，或读写失败 |
| 2 | 用法错误：命令、参数或声明不成立 |
| 3 | 取值校验失败 |
| 4 | 状态冲突，例如迁移的来源状态不符 |
| 5 | 数据过期 |

## 八、实现约束

一种行为只写一次，其余位置调用它，不各自复制一份再逐处修补。

| 行为 | 唯一实现 | 调用方 |
|---|---|---|
| 取值显示 | [value::cell](../../src/terminal/value.rs) | 终端表格、页面表格、详情块 |
| 记录加载 | [records::table](../../src/storage/records.rs)、[records::all](../../src/storage/records.rs) | 列表、页面区块、任务说明统计 |
| 排序 | [order::apply](../../src/query/order.rs) | 命令行输出与页面区块，按键与`字段:部件`同一套解析 |
| 表格排版 | [text::grid](../../src/terminal/text.rs) | 列表输出与内建命令 |
| 行投影 | [output::project](../../src/terminal/output.rs)、[output::raw_rows](../../src/terminal/output.rs) | 表格、TSV、CSV、JSON |
| 键值块 | [text::pairs](../../src/terminal/text.rs) | 详情块与帮助页 |
| 诊断报告 | [output::report_json](../../src/terminal/output.rs) | `check`与`doctor` |
| 实例校验 | [validate::problems](../../src/execution/validate.rs) | 表记录与文稿页同一遍扫描：键唯一、字段规则、文稿结构 |
| 文件遍历与链接 | [records::markdown_files](../../src/storage/records.rs)、[document::broken_links](../../src/storage/document.rs) | 文稿读取、`links check`、日志归档 |
| 列表过滤 | [filters::Keep](../../src/query/filters.rs) | 字段过滤、`--mine`、`--claimed`，身份与取值在命令开始时解析一次 |

命令行参数只解析一次：解析结果进入选项与打印结构，下游不再判断原始参数。
## 九、当前范围

已实现：工作区发现与声明加载、词表、身份、表存储、文档存储、编号分配、列表与查询、新建与修改、状态迁移、关系增删、归档与恢复、请求捕获与追加、取值校验、封存与漂移检查、自动提交与推送、同步闸门、页面区块渲染、检查与诊断、四种输出格式，以及公开仓审计与提交钩子。

写命令使用操作系统文件锁；Git工作区的锁与撤销日志位于该工作树的Git元数据目录，不提交到仓库。非Git工作区放在`.work/`。写前验证已有封存；普通命令不能接受已有漂移，显式`seal --note`保留说明后才可重建。失败操作恢复修改前的文件内容，进程被终止时由下次写命令先恢复。空目录可能保留。

编号登记在封存目录的`issued.csv`，按种类与键记录，归档和删除都不释放身份。升级时从当前在办和归档数据逐步补齐；升级前已经硬删除的历史编号需要另行恢复登记。

Git提交或推送失败返回非零并保留已接受的本地数据，不回滚已提交的内容。工作区原本有未提交修改时仍跳过自动提交并报告。只读命令和外部编辑器不参与写锁，跨克隆并发仍需Git合并；撤销日志不提供断电后的持久性保证。后续架构改进见[复核](review.md)。

## 九、文稿输入权限

文件名中的用户字段必须声明`write = "create"`，工具维护的文件名字段必须有编号或
自动生成来源。`clock`生成`HHMMSS`。短名采用非空ASCII字母、数字与中线，不能以中线
开头或结尾。文稿标题是必填用户字段；修改标题时同步更新索引，正文保持不变。
帮助显示每个字段是工具维护、创建可填或可修改。创建、修改、请求和导入复用权限规则。

## 十、协作执行

### 10.1 协作便利设施

种类可声明`[context]`，键`attachments`为路径字段数组，`repos`为引用仓库词表的字段，
`logs`为日志文稿种类。`clerk context <kind> <key>`输出记录、附件、双向一层关系、
最近五条完整编号匹配的日志、仓库与本机Git观察。`--json`包含版本、来源摘要和诊断。

关注事项`focus`支持`show`、`set`、`add`和`remove`；增删保序且幂等，新增键必须在某个单主键
种类中存在，移除允许清理失效键。工作流用`kind = "focus"`和`keys`执行同一操作。

工作流模板放在与种类目录并列的`workflows/`中，文件名是模板名。模板包含`version = 1`、
`title`、`[[param]]`与普通`[[step]]`。参数使用字段的`name/type/required/default/multi/
registry/target/display`声明与校验规则。参数仅以`${params.name}`在字符串值中展开，
不改变TOML结构；运行时新建别名继续使用`${alias.field}`。`params`是保留别名。
`workflow list/show/preview`展示模板或展开的可执行TOML；`batch --template <name>`执行，
`--dry-run`复用事务演练。预览不分配编号，不改数据；演练可能生成本机执行回执。
模板参数名不得与通用命令选项冲突。参数默认值在加载时校验，实例引用在执行时校验。

`clerk sync update`只在配置分支与干净工作树上执行安全快进。`sync fetch`只更新远端引用。
`[sync].write_update`接受`"refuse"`缺省或`"ff-only"`，可用`--sync ff-only`单次开启。
写前同步发生在业务默认值解释前，之后重新加载声明，演练不自动快进。

`clerk batch --file <路径或->`顺序执行结构化操作，共用应用入口、一次事务与Git收尾。
输入格式默认为TOML，`--input-format json`接受相同结构的JSON；`--preview`只检查计划。
写动作可用`--batch <名称>`收集本机临时草稿，创建步骤可用`--step-name`命名。
`batch show/apply/list/drop`管理草稿；成功后自动清理，持久回执防止重复写入。
完整生命周期、不兼容迁移和格式见[批处理与关注事项](../features/batch.md)。
`operation status [id]`查询本机结果，`operation retry [id]`只重试Git阶段。
完整工作流格式、别名引用、回执路径与失败恢复规则见[协作工作流](../features/workflows.md)。

### 10.2 条目关联与状态可达性

`append`动作可声明`entry_target = "tasks"`，目标必须具有单字段主键。
`--tasks APP-001,APP-002`按同一引用校验检查在办与归档对象，不能用于未声明目标的动作。
每条新日志在标题后保存带版本、独立编号与对象引用的TOML注释；无需迁移旧页。
状态域提供`terminal`布尔列时，`check`检查迁移涉及的非终态均可到达某个终态，
避免暂停或搁置后没有恢复出口。无此列的旧声明保留兼容。

### 10.3 本机会话与文件引用

终端阅读选项`--plain`与`--width <columns>`用于`context`和`workflow list/show`，
可设置命令默认值。总宽度允许20到10000列；显式宽度覆盖自动探测并启用带行号的
阅读模式。管道和重定向默认不主动换行、不添加行号。JSON以及`workflow preview`
保留机器结构，不能与阅读选项组合。`--compact`仅影响空格补齐，不控制换行和行号。

`context --full`独立控制正文选择，取消附件、关联文稿和已选日志条目的字符上限；
关系数量、展开深度和最近日志数量不变。它支持JSON，可在`[commands.context]`中
声明`full = true`，显式`--no-full`恢复摘要；不作为种类写入字段或工作流模板参数。

`session open/touch/scope/close/list/show`在Git公共元数据目录保存会话，
默认30分钟有效；过期不再占用范围，续期必须显式执行。`--strict`使范围重叠阻止写入，
普通模式报告重叠但允许登记。不同分支仍检查同一仓库的路径祖先关系。
会话不提交、不推送，不是分布式锁。

`links check/scan/repair [path...]`默认扫描Git跟踪的Markdown，非Git工作区扫描
受管路径；跳过代码块、外部URL和锚点。`repair`默认预览，`--apply`才写入，
`--linkify`同时把可确认的行内代码文件地址转成相对Markdown链接。
保留链接锚点与行号后缀；历史不唯一或目标不存在时只报告，不自动更改。
`version --json`无需工作区，报告构建提交、工作树标记和各机器格式版本。

## 十一、列表摘要与字段不可变

共用选项`--overflow truncate|more|pager|full`属于终端展示策略，可通过命令默认值声明，缺省full。仅在stdout为交互终端的文本阅读输出上生效，管道、重定向和机器格式始终完整。more和pager还要求stdin为终端；不满足时连续输出且不读按键。列表、详情、帮助与context接入同一个输出层，写入回执、错误、JSON和可执行模板预览不截断。

truncate按当前终端高度预留表头、提示行和命令提示符，列表显示完整行与已显示条目数；详情尽量按完整块截断，首块过长则标记partial。pager支持固定列表表头、上下方向、空格与PgUp/PgDn、Home/End、斜杠搜索、n继续、q或Ctrl+C退出，并在调整窗口大小后重新渲染和分配宽高。分页时使用备用屏幕，退出恢复原屏幕和终端输入模式。

more首屏按当前高度输出，回车追加一个显示行、空格追加一屏，q、Esc或Ctrl+C停止。它保留普通终端历史，不进入备用屏幕；结束后恢复输入模式。窗口变化只重排尚未输出的内容，不重读数据或重复已经输出的行。表头只输出一次，底部提示进度和按键。

pager先在内存构建完整后帧，与前帧逐行比较，只将变化行编码到同一个输出缓冲后写入。普通操作不清空整屏，未变化的表头和正文不重绘；短行覆盖长行时清除旧尾部。首次显示与窗口宽高变化时全量覆盖可见行，仍不先清空整屏。支持终端同步更新协议时，在同一更新边界内呈现一帧；其他终端仍使用行差分和缓冲写入。退出和错误路径结束同步更新并恢复终端。

列表摘要布局在`name = "list"`且无生成目标的表格视图中声明：

```toml
[[view]]
name = "list"
layout = "summary"
columns = ["id", "title", "state", "publisher", "claimant", "types", "repos", "updated"]
empty = { claimant = "unclaimed" }
```

list和long都使用现有view结构，额外声明如下：

```toml
[[view]]
name = "long"
layout = "summary"
columns = ["id", "title", "state", "publisher", "claimant", "owners", "types", "repos", "milestones", "created", "updated"]
flex = "title"
priority = ["id", "title", "state", "publisher", "claimant", "owners", "types", "repos", "updated", "milestones", "created"]
empty = { claimant = "unclaimed" }
```

columns决定显示顺序；priority从高到低决定保留顺序，未列出的字段按columns顺序排在后面；flex指定优先获得剩余宽度的字段。两项均可省略，字段必须属于该视图的columns，priority不能重复。它们只用于无生成目标的list和long表格视图。long缺省沿用list，不回退为所有字段；过滤与显式排序共用原有查询，long可声明自己的sort，否则沿用list排序。

默认、long和显式columns共用同一宽度布局器：终端下按可见宽度排版，标题等弹性列优先；单元格省略，低优先级列隐藏并在表尾提示，保持每任务一行。显式columns先保留所选列、缩短单元格；极窄窗口连每列一格加分隔符都放不下时才隐藏尾列。单列、自选顺序与其他种类共用规则。details展示全部字段的竖排详情，完整正文、依赖和日志仍通过context读取。

width指定总宽度，plain禁用宽度限制，max-width仍可限制单元格；管道默认不探测宽度，可显式提供width。机器格式按原来的声明或显式列输出，不受long、details和布局省略影响。

本轮将long从“所有字段的无限宽表”修正为“声明的短字段扩展列表”，不保留旧行为。accept、note、邮箱、完整哈希和附件路径不进入本工作区long视图，需要时用details或显式columns。此前删除的secondary、presence、relations视图键仍不恢复；owners、附件和关系数据保留。


字段新增布尔键`immutable`，缺省`false`。为发布者、发布邮箱和创建时间声明`immutable = true`，使其创建后不能被用户或工具动作改写；`auto = "now"`配合不可变字段仅初始化一次。机器声明导出包含此约束。

`layout.identity`保留为旧本机身份的迁移来源。运行时统一使用Git元数据目录的`clerk/identity`；迁移不覆盖不同的新值，冲突时保留双方并报错。


## 十二、终端策略与查询匹配

工作区可声明terminal表，键overflow、paging、color分别取full/truncate/pager/more、auto/always、auto/always/never；缺省full、auto、auto。命令行优先于具体命令默认值、命令默认值、terminal及工具缺省。只接受已知键和值。overflow短选项为-o。

paging always仅用于pager和more：短内容也进入交互；more在末尾等待回车、空格或q确认结束。非交互输入输出和机器格式永不等待。color auto按输出终端能力决定，always允许人读文本管道保留颜色，never不写颜色码；机器格式始终不染色。

case接受ignore或sensitive。未指定时filter保持原有精确比较和忽略大小写包含语义，正则默认忽略大小写。matches和not matches是正则运算符，不改写~和!~。正则在遍历前编译，多值按成员匹配，否定为没有成员匹配。

show和context默认先选原样精确主键，再忽略大小写精确匹配；match可选exact、prefix、contains、regex。只作用于声明主键，多条返回候选，不自动挑第一条。写入不采用此解析。

pager的search-mode为literal或regex，缺省literal；case缺省ignore。按匹配位置计数，n/N前后循环，搜索词、模式、当前序号、总数和off-screen均显式显示。Alt+r/c切换模式和大小写，空查询清除，Esc恢复编辑前查询，Tab补全本次阅读的搜索历史。高亮所有可见匹配并区分当前项，颜色关闭时仍提供文字定位。

语义样式角色为heading、muted、identifier、info、success、warning、error。字段可声明terminal_style为固定角色，或由字段值映射到角色；样式不改变业务值、排序、校验或生成文档。

completion powershell输出注册脚本；completion complete --words-json提供光标前完整词元数组，--current提供当前词元，--json输出候选及英文说明。命令、动词、选项、模式值、字段词表、对象编号和模板参数都从统一声明读取。路径由PowerShell补全，filter仅在明确的字段、运算符与词表值位置补全。正则内部不改写，补全不修改个人配置。


字段样式示例：

```toml
[[field]]
name = "state"
type = "enum"
registry = "state"
role = "status"
terminal_style = { todo = "muted", active = "info", review = "warning", done = "success" }
```

映射键若引用词表，必须是合法词表键；星号可作为缺省角色。编号未另声明时使用identifier，
其他字段不根据名称猜测语义。样式声明会进入机器声明导出，但不进入记录值和生成页面。


## 十三、读取参数与交互能力

本次移除旧`--archived`，使用`--data-scope current|archived|all`控制list与show的数据集合；不与业务scope字段共用参数名。选定集合先经过引用身份冲突检查，再统一过滤和排序。投影来源用保留的`@archive`标记，JSON在业务data之外提供scope与逐条provenance，字段名不允许以`@`开头。

命令元数据按Data scope、Query and order、Content、Presentation、Reading、Write and recovery与General分组。带动词帮助、补全和运行前校验复用适用入口规则；`explain <kind> --json`的command_options提供name、group、short、choices、cli_prefix、default_allowed、verbs、requires与conflicts。requires是带when、option与any_of的条件数组，不需要解析帮助文案。command_options同时导出argument、description、help_order与initial，分别表示参数占位、上下文说明、组内顺序及工具缺省；实际帮助的默认值叠加当前工作区配置。帮助采用结构化页面与选项模型，不解析展示标签恢复参数含义；布局、搜索来源及颜色由展示层统一投影，宽度变化后重新排版，所有字段保持可读。总览只列通用入口，具体选项按适用命令显示。

`--paging`短名为`-p`，`--data-scope`短名为`-S`。所有内置枚举选项均由choices声明支持CLI唯一前缀，完整值优先，歧义报告候选；input-format和sync也进入同一候选声明。字段导出的cli_prefix标记由词表、布尔类型和引用约束共同决定，引用保持精确匹配。显式CLI参数在填充默认值之前展开，捕获的草稿与请求保存完整值；过滤表达式和文件导入不做前缀展开。短选项拆分由解析与补全共用，支持紧连参数和组合开关，完整写法见[终端参数说明](../features/terminal.md#31-参数书写方式)。配置仍只接受完整枚举；`[commands."tasks.list"]`可设置data-scope默认值，不要把列表专用默认值放在会同时作用于写动作或show的通用种类段。pager内排序能力由读取投影提供，不进入工作区配置，不写入数据与Git。

完整操作与迁移示例见[读取选项与交互阅读](../features/terminal.md)。

通用`--no/-N`由选项声明的negation决定行为：reset表示取消显式值并屏蔽配置默认值，value表示使用声明的关闭值。声明字段与编号参数可取消命令默认值，基础字段缺省与校验保持生效。no自身不允许配置默认值，其choices导出当前种类候选，no_targets按动词细分适用集合，cli_prefix为true。解析、帮助和补全共用动态候选；正向与否定输入按出现顺序覆盖，禁止把目标短别名解释为另一套规则。

paging新增never，禁止pager与more交互，保持full与truncate的选择；color和paging的否定值为never，headers的否定值为hide。终端模式配置和命令默认值仍使用完整值。
