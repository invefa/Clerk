# 一、协作登记与文件引用

本章接续[用户手册](../guide/user-manual.md)与[上下文及模板](context-and-templates.md)，说明日志、
本机会话和文件链接的日常用法。任务负责人仍是人员词表中的身份；会话只补充当前对话
正在做什么，不替换认领和状态迁移。

## 二、构建身份

```sh
clerk version
clerk version --json
```

不需要工作区。JSON包含软件版本、构建提交、构建时是否有未提交修改，以及工作区、
工作流、上下文、日志条目和会话的格式版本。提交与脏标记来自构建过程，不读取运行
目录的Git身份。无法确定的构建信息输出`null`；版本号相同不能证明二进制来自同一提交。
发布构建可用`CLERK_BUILD_COMMIT`与`CLERK_BUILD_DIRTY`提供身份。

## 三、日志显式关联

日志种类的追加动作声明目标种类：

```toml
[[action]]
name = "add"
handler = "append"
heading = "## [T{time}] {title}"
entry_target = "tasks"
```

```sh
clerk logs add --title "预约实例验证" --tasks APP-026,APP-027 --body "实现与验证说明"
clerk context tasks APP-026 --json
```

`--tasks`接受逗号或竖线分隔的主键，目标必须已经存在，可以是在办或归档对象。
输入包含错误主键时整条追加失败，不会先创建日志页。每条新日志在标题后保存带版本的
TOML注释，包含独立编号、目标种类和关联键。关联只属于该条日志，不扩散到整日页面。
用户正文中的小节仍归同一条目。`check`检查关联、元数据格式和重复条目编号。

旧日志无需迁移，仍按完整编号检索正文。上下文中的`log explicit reference`表示显式
关联，`log text mention`表示正文匹配；两类都保留来源、行号、摘要和截断说明。
模板中的关联放在步骤的`tokens`中：

```toml
[[step]]
kind = "logs"
verb = "add"
tokens = { title = "任务推进", tasks = "${params.task}" }
body = "阶段结果与验证记录"
```

## 四、活动会话

```sh
clerk session open --chat reservation-chat --tasks APP-026 --paths src/reservation,tests/reservation.rs --json
clerk session list --json
clerk session show SESSION-ID
clerk session touch SESSION-ID
clerk session scope SESSION-ID --add src/storage --remove tests/reservation.rs --strict
clerk session close SESSION-ID
```

把`SESSION-ID`替换为`open`返回的编号。登记保存负责人、Git邮箱、对话、任务、当前
工作树与分支、修改范围、开始时间、最后活动和到期时间。任务引用共用正式引用校验。
范围使用仓库相对路径，目录范围覆盖子路径，拒绝绝对路径与`..`。

默认30分钟有效，可用`--ttl 3600`指定秒数，允许1到86400秒。`touch`明确续期并刷新
分支；范围调整也续期。过期会话保留记录，但不再产生占用提示，已关闭会话不能续期。
只有登记时的Git身份和工作树可以修改会话。

同一Git克隆的worktree共享会话文件。新建、扩展范围和续期时检查路径的祖先关系，
分支不同也检查。通常报告重叠编号后允许登记；`--strict`在重叠时拒绝本次变更，保留
原会话。它不阻止编辑器写文件，也不能检测其他机器或独立克隆，因此不能当作分布式锁。

会话保存在Git公共元数据目录的`clerk-sessions/`中，不纳入提交、推送或业务事务。
会话命令之间使用文件锁和原子替换；没有后台心跳，Agent应在继续工作时显式续期。
任务上下文附带本机对应会话及到期信息，可辅助接手者判断登记是否仍有效。

## 五、文件引用维护

```sh
clerk links check
clerk links scan --linkify --json
clerk links repair topics tasks --linkify --json
clerk links repair topics tasks --linkify --apply --json
```

默认扫描Git跟踪的Markdown；指定路径时扫描该文件或目录，目录遍历跳过子模块、
符号链接、构建与缓存目录。非Git工作区默认扫描受管路径。`check`仅检查链接，
`scan`与`repair`默认只读，输出路径、行号、原地址、替换方案和历史候选。
`--apply`只用于`repair`，写入共用撤销、封存、操作回执和Git收尾；不支持请求模式。

解析支持行内链接、图片目标、引用式链接定义、括号目标、尖括号目标和百分号编码，
跳过代码块、普通代码示例中的伪链接、HTML注释、外部URL与页内锚点。
`--linkify`把确认存在的行内代码文件地址变成Markdown链接，优先按文档所在目录解释，
带目录的地址其次按仓库根目录解释；单独文件名只按文档目录解释，避免误指根目录同名文件。
命令、通配符、模板占位符和无法确认的代码标识符保留原样。
新文档应直接使用相对Markdown链接，不依赖维护命令猜测任意自然语言中的文件名。

历史修复依据当前`HEAD`的Git重命名记录与已暂存的工作区重命名，同时处理引用目标
改名和文档搬家造成的相对地址变化。只有历史唯一且目标当前存在时自动生成替换；
同一旧名多次用于不同文件时报告`ambiguous`，删除或没有重命名证据时报告`missing`。
保留链接的`#fragment`及`:行号`后缀，但不验证章节或行号是否仍有相同语义。
浅克隆、历史重写、内容变化过大而未被Git识别的移动可能无法恢复，需要人工确认。

生成区域的修复报告`generated`，不会直接覆盖；应修正原记录或渲染声明后运行`render`。
生成Markdown中的路径字段在目标存在时自动显示为相对链接，CSV与JSON保留原值。
只读扫描可查看工作区外的文件，应用只允许写入本工作区。

## 六、状态恢复

状态域提供`terminal`布尔列时，`check`检查已声明迁移的非终态是否有到达终态的路径。
例如`hold`进入`held`，就应提供从`held`恢复的迁移。设计示例的`resume`同时接受
`paused`和`held`；`start`仍服务于首次开工与暂停后开始。

```sh
clerk tasks show APP-026
clerk tasks resume APP-026
```

这只是命令示意，不应恢复已经完成的任务。具体合法来源以当前种类声明为准。
没有迁移的台账种类与没有`terminal`列的旧声明不启用可达性诊断。
