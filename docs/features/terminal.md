# 读取选项与交互阅读

## 一、选项分层

先决定读取的数据，再决定正文、展示与阅读方式。帮助按同一组别排列；带动词的帮助只显示适用选项。命令元数据同时供校验、补全与`explain <kind> --json`使用。

| 分组 | 职责 | 主要选项 |
|---|---|---|
| Data scope | 当前记录、归档或两者 | `-S, --data-scope current/archived/all` |
| Query and order | 筛选与排列完整记录 | `--filter`、`--sort`、`--desc`、`--mine`、`--claimed` |
| Content | 选择摘要或完整正文 | `context --full` |
| Presentation | 选择列、详情、格式、宽度与颜色 | `--columns`、`--long`、`--details`、`--json`、`--width`、`--color` |
| Reading | 连续输出、预览、分页与进入条件 | `--overflow`、`--paging`、`--search-mode` |
| Write and recovery | 业务写入、提案、事务与Git | `--message`、`--request`、`--batch`、`--sync` |

`--match`只负责show与context的只读编号选择；`--case`供文字过滤、编号选择和pager搜索共同使用。`--full`不等于`--overflow full`，前者控制正文选择，后者控制终端显示范围。

## 二、当前与归档

```sh
clerk tasks list
clerk tasks list --data-scope archived
clerk tasks list --data-scope all --sort updated --desc
clerk tasks show app-001 --data-scope all
clerk contracts list --data-scope all --json
```

current表示尚未归档，不等于尚未完成。归档与全量读取适用于表类和文稿类的list、show；先选择集合，再统一过滤、排序和投影。写动作不接受此选项。context保留原有在办优先、缺失时查询归档的接手逻辑，不接受数据范围选项。

归档与全量文本输出增加`@archive`列或详情项，值为true或false；它属于投影元信息。JSON的业务`data`不增加此字段，顶层`scope`标明范围，`provenance`按输出顺序提供每条记录的主键与archived布尔值。业务字段以`@`开头的名字保留给查询元信息，不可声明。

同一主键在选定集合中重复时拒绝并提示检查来源，不能静默去重或选择其中一条。可分别用current与archived核对冲突。

本次是不兼容变更：移除`--archived`，替换为`--data-scope archived`；配置中的`archived = true`相应改为`data-scope = "archived"`，不保留别名。采用data-scope是为了与复核已有的业务scope字段分开，原有`reviews new --scope ...`用法保持。

## 三、帮助与简写

```sh
clerk tas --help
clerk tas lis --help
clerk help tas lis
clerk tasks list -o p -p al
```

命令、动词和帮助使用同一条规则：完整名称优先，其次接受唯一前缀。`li`同时匹配list与link时报告歧义，写lis即可。帮助只读取声明，不执行目标动作或同步。

`-S`是data-scope的短名，`-p`是paging的短名，`-o`是overflow的短名。所有有固定候选的CLI选项值都支持唯一前缀，大小写敏感；完整值优先，歧义列出候选，空值与未知值报错。

| 选项 | 前缀示例 |
|---|---|
| data-scope | c为current，ar为archived，al为all；a有歧义 |
| overflow | f为full，t为truncate，p为pager，m为more |
| paging | au为auto，al为always，n为never；a有歧义 |
| color | n为never，au为auto，al为always；a有歧义 |
| case | i为ignore，s为sensitive |
| search-mode | l为literal，r为regex |
| match | e为exact，p为prefix，c为contains，r为regex |
| headers | s为show，h为hide |
| input-format | t为toml，j为json |
| sync | f为ff-only |

声明中由词表提供有限候选的字段、布尔字段和编号词表参数也支持前缀，工作流模板的显式CLI参数同样适用。多值字段按竖线逐项展开，如`--types 'a|d'`；候选来自实际声明，添加候选后最短可用前缀可能变长。引用编号、路径和自由文本保持原值，`--filter`表达式中的字符串不展开。可选业务字段的空字符串继续表示清空，内置枚举选项不允许空字符串。

选项长名本身不进行前缀猜测。工作区配置、CSV导入和批处理文件使用完整值，不接受这些枚举缩写；CLI收集为草稿或请求前先展开成完整值。补全填入完整候选，并保留当前采用的短选项或等号写法。

### 3.1 参数书写方式

| 写法 | 行为 |
|---|---|
| `--color never`、`--color=never` | 长选项，空格或等号连接值 |
| `-S all`、`-Sall`、`-S=all` | 短选项，空格、紧连或等号连接值 |
| `-rq` | 组合无值开关，等同`--desc --quiet` |
| `-rSal` | 先反转顺序，再把al作为data-scope的值，展开为all |
| `-rs title` | 先反转顺序，sort消费下一个参数title |
| `--json`、`--no-json` | 无值开关及其关闭形式，不能写成`--json=false` |
| `--` | 结束选项解析，之后的所有参数均作为位置参数 |

短选项组遇到需要值的选项时，余下文本全部作为它的值；没有余部时消费下一个参数。故`-Srq`会尝试将rq解析为资料范围，不会将r和q当开关。参数以双横线开头时使用等号或短选项紧连方式，避免与另一个选项混淆。

```sh
clerk tasks list -Sal -op -pal
clerk tasks list -rSal --color=n
clerk tasks new --title="-hello world"
clerk tasks show -- APP-001
```

引号由Shell处理，用于将包含空格的文本作为一个参数传入；不属于Clerk枚举语法。脚本建议使用完整值，文档优先用空格分隔，手工输入可使用紧连与组合形式。

### 3.2 用户短名

短名方便用户手工输入，在所有命令中保持同一含义；适用入口和约束与长名相同。

| 短名 | 完整选项 | 用途 |
|---|---|---|
| `-b` | `--batch` | 收集操作到本机草稿 |
| `-t` | `--template` | 选择工作流模板 |
| `-i` | `--file` | 从文件或标准输入读取正文 |
| `-m` | `--message` | 显式提交说明 |
| `-M` | `--message-file` | 从UTF-8文件读取提交说明 |
| `-C` | `--root` | 指定工作区根目录 |
| `-c` | `--columns` | 选择展示列 |
| `-w` | `--width` | 指定终端阅读总宽度 |
| `-D` | `--details` | 竖排展示完整字段 |
| `-F` | `--full` | 完整上下文正文 |
| `-n` | `--dry-run` | 演练后回滚 |
| `-P` | `--preview` | 展开并预览计划 |
| `-N` | `--no` | 关闭功能或取消配置的默认选择 |

原有`-s sort`、`-r desc`、`-l long`、`-f filter`、`-d display`、`-o overflow`、`-p paging`、`-S data-scope`、`-q quiet`、`-h help`与`-v version`保留。`--json`、`--tsv`、`--csv`、`--md`仅保留长名，`-j`空置。

```sh
clerk tasks list -c id,title,state -w100
clerk tasks list -D
clerk tasks new --title "修复预约" -b reservation
clerk batch -t start --task APP-001 -m "开始处理预约"
clerk context tasks APP-001 -F -op
```

### 3.3 不兼容迁移与Agent规范

本次不保留旧输出格式短名兼容：旧`-j`改为`--json`，旧`-t`改为`--tsv`，旧`-c`改为`--csv`，旧`-m`改为`--md`。其中`-t`现表示template、`-c`表示columns、`-m`表示message，不能依赖旧脚本继续报错来发现变化，应主动改成完整名称。`--file`与`--message-file`读取的内容不同，分别为正文和提交说明。

Agent实际调用、自动化脚本和持久工作流示例必须使用完整命令、动词、长选项及枚举值，不使用前缀、组合短选项或缩写编号。隔离测试验证简写能力与用户手工输入示例可以使用简写；工具不尝试推断调用者是否为Agent，也不增加另一套执行入口。

```sh
clerk --root ../library-workspace context tasks APP-001 --full --overflow full --json
clerk tasks list --data-scope all --columns id,title,state --width 100
```

overflow默认full。选择pager或more后，paging默认auto，超过一屏才交互；always强制进入，短结果也可搜索。always与full或truncate组合报错，配置默认值也按同一规则检查。never禁止pager和more进入交互，仍保留truncate策略；`--no paging`等价于`--paging never`。机器输出和非交互输出不等待按键。

### 3.4 帮助阅读

`clerk help`按Inspect、Workflows、Collaboration、Maintenance与Help and integration分组导航，再列出工作区种类与root、no、help、version四个通用入口。总览不铺开全部选项；使用`clerk tasks list --help`等具体入口查看参数，完整种类帮助保留字段、迁移、动作和声明来源。

选项按短名、长名及参数、说明分别保存与展示，同一页面共用列宽，没有短名时保留短名栏。分组按用途排列，常用参数优先；元数据的Presentation组在帮助中显示为Output，batch的工作流输入单列Input。枚举使用`<scope>`、`<mode>`等占位符，合法值与当前默认值放在附属行，工作区默认值会覆盖工具缺省。

```text
Data scope:
  -S --data-scope <scope> select current or archived records
                         Values: current, archived, all. Default: current.

Query and order:
  -f --filter <expr>      condition, e.g. state = done or title ~ text
  -s --sort <keys>        sort by comma separated keys, e.g. state,id:prefix
```

终端变窄时，说明移到选项下方并相对条目缩进；继续折行时保持说明缩进，不截断内容。字段表放不下时按记录竖排保留全部列。`--compact`省去补齐空格，`--width`可显式演练某个宽度；帮助支持已有的full、truncate、more与pager策略，Pager缩放重新排版。未折行逻辑文本与显示行分开，跨显示行的短语仍可搜索，高亮和字段颜色共用逻辑位置。

```sh
clerk tasks list --help --width 48
clerk batch --help --overflow pager --paging always
clerk batch show --help
```

Kinds使用名称、标题和说明三列，按中文显示宽度对齐；窄屏时说明换到下一行并相对条目缩进两个空格。该布局可用于其他无表头帮助列表。

子命令帮助只显示适用参数和示例。batch不接受request，其file表示工作流文件；tasks brief等动作的file仍表示正文文件。命令语法与枚举前缀规则放在末尾Notes，Agent和脚本继续使用完整写法。

### 3.5 通用否定参数

`--no <option>`与`-N <option>`共用一套动态候选，候选来自当前命令适用的选项、声明字段与编号参数。完整名称优先，唯一前缀可用，歧义列出候选；目标使用完整长名或它的前缀，不引用目标的单字母短名。

```sh
clerk tasks list --no filter --no sort --no columns
clerk tasks list -N colo -N pa
clerk tasks list -Ncolor -N=paging
clerk tasks list --no=json
```

例如colo唯一指向color，pa指向paging；c可能同时匹配color、columns、compact、csv或其他当前候选。新声明可能增加歧义，Agent与脚本使用`--no color`等完整写法。

| 目标 | 行为 |
|---|---|
| color | 显式设置never，关闭颜色 |
| paging | 显式设置never，禁止交互分页 |
| headers | 显式设置hide，隐藏表头 |
| 布尔开关，如json、display、desc、compact | 关闭开关，并阻止配置重新开启 |
| 默认选择，如filter、sort、columns、width、overflow | 移除该选择，并屏蔽工作区及命令默认值回填 |
| 声明字段与编号参数 | 取消对应命令默认值；字段自身缺省与必填校验仍生效 |
| root、file、message等无否定声明的运行输入 | 不参与候选，不能关闭 |

取消filter后不增加默认筛选，取消sort后使用基础记录顺序，取消columns后使用种类声明的视图，取消overflow后使用工具缺省full。这些操作只影响本次调用，不修改配置文件。

正向和否定参数按输入顺序覆盖，最后一个生效。`--color always --no color`关闭颜色，`--no color --color always`开启颜色。内置可否定选项也接受`--no-color`等完整形式；布尔开关不接受`--json=false`。重复`--no`可以取消多个选择，不使用逗号分隔目标。

补全在`--no`或`-N`后只提供当前适用目标。帮助列出动态候选，机器声明的negation说明关闭行为，no_targets按动词导出候选。进入请求或草稿前完成展开和默认值屏蔽，不把缩写存入业务字段。

## 四、pager搜索编辑

| 状态 | 按键 | 行为 |
|---|---|---|
| 浏览 | `/` | 编辑已确认的搜索词，光标放到末尾 |
| 编辑 | 左右、Home、End | 按字素移动，支持中文与组合字符 |
| 编辑 | Backspace、Delete | 删除光标前、后的字素 |
| 编辑 | 普通输入、粘贴 | 在光标位置插入，忽略控制字符 |
| 编辑 | Ctrl+u | 清空当前输入 |
| 编辑 | Enter | 确认查询并定位当前匹配，退出编辑 |
| 编辑 | Esc | 放弃编辑，恢复已确认的查询和匹配模式 |
| 浏览且有查询 | Enter、n、N | 下一处、下一处、上一处 |
| 浏览且无查询 | Enter | 向下滚动一行 |
| 浏览且有查询 | Esc | 清除查询与高亮，保持当前阅读位置 |
| 浏览且无查询 | Esc | 退出pager |
| 浏览 | q | 退出pager；编辑时q为普通输入字符 |
| 任意状态 | Ctrl+C | 退出pager |
| 编辑 | Tab | 补全本次阅读的搜索历史 |

输入过程中更新高亮与计数；非法正则保留输入和错误，Enter不退出编辑。输入框超过底栏宽度时随光标水平滚动，左侧小于号表示隐藏了前面的文字。回到首个或末个匹配时底栏显示Wrapped提示。

Alt+r切换literal与regex，Alt+c切换大小写。搜索只处理原始逻辑文字，不检索ANSI码。普通匹配叠加下划线与加粗，当前匹配叠加反显，同时保留原语义颜色；离开匹配后恢复原样。关闭颜色仍可搜索和跳转，结果计数可见，输出不含颜色码。

搜索面向当前投影；隐藏列与省略的文字不产生不可见匹配。完整正文使用context --full，完整字段使用details。

## 五、pager内排序

仅kind list提供排序入口，默认列表、long与details均使用完整查询快照；帮助、context和普通文字页不提供此能力。

| 按键 | 行为 |
|---|---|
| s | 编辑逗号分隔的排序字段，例如state,updated |
| 排序编辑中的Tab | 按当前末字段前缀补全合法字段与编号分段 |
| Enter | 校验并应用；空输入或未知字段保留编辑并报错 |
| Esc | 取消输入，保留原顺序 |
| r | 反转当前顺序 |
| R | 恢复进入pager时的初始排序 |

排序框与搜索框共用编辑、光标和粘贴规则。先对完整快照排序，再重新投影，所以可以按未显示字段排序。重排尽量保留当前记录位置，保留搜索词并重建匹配。排序仅作用于本次阅读，不修改文件、默认值或Git；窗口变化继续使用本次顺序，不重新读取业务文件。

更多终端宽度、颜色声明与配置规则见[使用手册](../guide/user-manual.md)和[声明格式](../architecture/declarations.md)。
