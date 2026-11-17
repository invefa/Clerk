# 一、批处理与关注事项

本版包含不兼容变更：原执行器`run`改名为`batch`，原任务批次命令改名为`focus`。不保留旧命令别名。工作流中的`kind = "batch"`、视图渲染器和集合文件名相应改成`focus`、`focus.csv`；配置默认值中的命令名也须迁移。历史回执不改写。

`focus`是共享的关注事项集合，支持`show`、`set`、`add`、`remove`。`batch`是一组按顺序执行的操作，复用普通命令的校验与事务：全部成功后一次提交、推送，业务失败整体回滚。两者不共享生命周期。

升级前先用旧二进制记录关注成员、处理未完成操作并保持工作树干净。升级后迁移配置中的命令默认值、工作流步骤、视图的渲染器与`frag-focus`引用；将原集合文件移动为`focus.csv`，保留原始内容与顺序，提交配置迁移，再执行`check`与`render`核对。工具不会自动解释旧命令或读取旧集合文件，不能只更新二进制。历史任务正文、日志和操作回执保留当时写法。

## 1.1 直接执行

```sh
clerk batch --file changes.toml --preview
clerk batch --file changes.toml --dry-run
clerk batch --file changes.toml --message-file commit.txt
clerk batch --file changes.json --input-format json
clerk batch --template start --task APP-001 --description '落实验收范围'
```

TOML与JSON使用相同的`version = 1`和`step`结构，字段类型严格一致。`--json`控制输出格式，`--input-format`控制输入格式。输入文件为`-`时从标准输入读取；相对正文文件路径以计划文件目录为基准。模板与参数声明见[便利设施](context-and-templates.md)。

例如JSON计划：

```json
{"version":1,"step":[{"kind":"tasks","verb":"new","as":"task","values":{"title":"预约校验"},"tokens":{"prefix":"ENG"}},{"kind":"focus","verb":"add","keys":["@task"]}]}
```

`required`提交策略下，正式执行和演练仍须提供`--message`或`--message-file`；预览与收集不要求说明。草稿支持已声明种类的写动作、`focus`成员变更和`render`，不收集Shell、`seal`、身份设置或请求审批。CSV导入继续使用`new --from`，需要多条创建步骤时使用结构化计划。

`--preview`只解析、展开与静态校验，不同步、不分配编号、不执行、不要求提交说明。`--dry-run`经过真实业务规则与封存检查，然后回滚；它仍可能因状态或权限失败。

## 1.2 边操作边收集

```sh
clerk tasks new --prefix APP --title '预约校验' --batch reservation --step-name task
clerk tasks brief @task --file brief.md --batch reservation
clerk focus add @task --batch reservation
clerk render --batch reservation
clerk batch show reservation
clerk batch apply reservation --dry-run
clerk batch apply reservation --message-file commit.txt
```

传入`--batch <名称>`只追加本机草稿，不修改业务、不提交或推送。文件和标准输入正文在追加时读取并保存，后续修改原文件不会改变已收集的步骤。创建步骤可用`--step-name`命名；未指定时自动命名为`step1`、`step2`等。编号在正式执行时才分配，后续步骤用`@task`引用完整编号，字段值和正文可用`${task.id}`引用。该语法与直接文件执行共用。

`batch list`列出本机草稿，`batch drop <名称>`删除草稿。草稿是临时文件，位于当前Git工作树元数据目录，不进入版本控制、不推送、不跨克隆同步，也不替代正式任务资料。成功执行后自动删除；演练、业务失败或Git失败保留草稿，便于检查。Git重试成功后也会清理对应草稿。清理失败只提示，不重放已经接受的操作。

追加步骤受同一写锁保护，以原子替换方式保存。执行前检查操作者、声明和已有目标的快照；无关任务变化不会仅因HEAD变化而阻止执行。`focus add/remove`和日志追加在锁内读取最新内容并保留其他人的成员与正文；`focus set`替换整个集合，需核对原集合未变化。文稿修改、归档等暂以该种类目录为校验范围。发现相关变化时重新收集并检查，不自动覆盖。

正式执行的本机回执通过事务接受标记与业务修改保持一致，包含草稿身份与内容摘要。已经执行的草稿不能重放；Git收尾失败时使用`operation status`和`operation retry`，不要再执行`batch apply`。演练和失败回滚不消耗草稿。

## 1.3 回执与提交摘要

执行结果列出每一步的序号、动作与解析后的对象编号；详细记录位于Git元数据的`clerk/operations/`本机回执。错误指出失败步骤，前面步骤不会留下业务残留。

自动提交只汇总新增、更新等对象编号和修改文件，合并重复对象、限制展示长度，不抄录任务标题、字段值或正文。调用者提供的提交说明保持原样。`auto`、`required`、`default`策略见[提交说明](../maintenance/commit-messages.md)。

本机布局、接受一致性和旧回执删除步骤见[本机状态与操作恢复](../maintenance/local-state.md)。新的步骤回执不进入Git历史。
