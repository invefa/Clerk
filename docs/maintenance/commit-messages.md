# 提交说明

## 一、调用方式

自动提交开启时，写命令接受`--message <text>`或`--message-file <path|->`。
两者互斥；文件按UTF-8读取，`-`读取标准输入。说明包含首行和可选正文，
工具追加操作编号，用于Git失败后的安全恢复。首行必须非空，不允许控制字符或
调用方伪造`Clerk-Operation:`标记。输入错误在业务写入前拒绝。
任务的`--commit`仍表示验收证据中的代码提交，与本次自动提交说明分别传入。

```sh
clerk tasks start APP-001 --message 'chore(tasks): start APP-001'
clerk batch --template start --task APP-001 --description '范围与验收' --message-file commit.txt
clerk operation retry
```

`commit.txt`是调用方准备的说明文件；不要在工作区留下未提交的说明文件，以免
触发已有改动保护。可以使用Git忽略的工作目录，文件路径相对调用目录解析。
提交风格由工作区的校验规则约束；启用严格审计时应提供包含验证依据的完整正文。

标准输入只能有一个消费者。`--message-file -`不能同时用于工作流输入、CSV导入、
列表输入或文稿正文；需要正文的命令须显式提供`--body`或普通文件路径。

## 二、工作区策略

在[工作区声明](../architecture/declarations.md)的`[policy]`中配置：

```toml
[policy]
auto_commit = true
commit_mode = "required"
```

| 模式 | 未提供说明时的行为 |
|---|---|
| `required` | 拒绝写入，要求本次调用提供说明 |
| `auto` | 根据整批对象编号与受影响路径生成简短说明 |
| `default` | 使用`commit_message`固定说明 |

新配置缺省`auto`。旧配置只设置`commit_message`时保留固定说明行为；切换到
自动生成须显式设置`commit_mode = "auto"`。固定说明只有首行时，工具补充操作
摘要和路径；提供完整正文时保留正文。显式说明始终优先于工作区的固定说明。

自动摘要按种类与新增、更新、删除、归档、恢复归并编号。同一对象新建后的说明、
认领与状态操作合并到新增项，不复述标题、字段值或正文。每组最多显示五个编号，
文件最多显示五条路径，其余以数量表示；最多展示八组，单项长度最多80字符。
中文编号或路径保留UTF-8，不转成Unicode转义。工作区另有ASCII提交规范时，
应提供符合该规范的显式说明。自动摘要示例：

```text
chore(clerk): add 3 managed object(s)

Added tasks (3): APP-001, APP-002, APP-003.

Files (5): .clerk/data/tasks.csv, .clerk/seal/issued.csv, .clerk/seal/seal.csv, status.md, tasks/APP-001.md.

Verified managed-file validation before Git finalization.
```
`explain <kind> --json`导出当前提交策略及对应选项，便于Agent构造命令。

关闭自动提交时不强制提供说明。只读命令、活动会话和请求捕获不进行Git提交，
拒绝提交说明选项，避免误以为发生了提交；请求在`request apply`或`reject`时
由执行者提供说明。`--dry-run`按相同策略校验，但不提交。预览模板不要求说明。

## 三、事务与恢复

单个命令、CSV批量导入、请求整批应用和`batch`均经过同一写入管线，一次事务只
生成一次提交。自动说明汇总整批修改；工作流仍保留逐项审计回执。

最终说明在接受本地数据前保存到本机操作回执。提交钩子或推送失败后，通过
`operation status`检查数据、提交与推送阶段，再运行`operation retry`。重试
使用原说明，不接受替换说明，也不重放业务动作。后续配置变更不会重写说明。
旧版尚未记录说明的回执在重试时明确拒绝自动重建，需要人工核对Git收尾。

`Clerk-Operation:`是操作回执编号，用于把Git提交与本机写入记录对应起来。
新编号形如`op-...-...`，将原来的时间戳和进程号压缩编码；旧的长数字编号继续
有效。`operation status`和`operation retry`不带编号时定位最近一次操作，
通常无需复制编号。`operation status --json`同时给出保存的完整提交说明。

操作恢复回执与批处理步骤统一保存在Git元数据的`clerk/operations/`，不进入业务提交。接受一致性和升级流程见[本机状态](local-state.md)。
