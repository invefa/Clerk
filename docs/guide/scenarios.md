# Clerk场景指南

- 日期：[20261101]
- 目的：从实际问题理解功能用途，配合[用户手册](user-manual.md)使用

## 一、场景前提与选择

本章以社区图书馆预约系统为例，团队示例假定工作区提供`tasks`、`contracts`、`logs`和`start/review/finish`
模板，任务编号采用`APP-001`形状，配置与[声明样例](../../examples/library)相近。
先用`kinds`、`help tasks`和`workflow show`核对实际配置；缺少某项声明就不能直接
执行对应命令。文中的编号、分支、提交哈希、任务证据和路径均须替换为实际值。

所有场景彼此独立，不要求在正式仓库全部执行。要从空目录体验，先完成主手册第三章
的`TASK-001`练习；那个最小工作区没有认领、日志、关系和团队模板。

| 眼前的问题 | 对应场景 |
|---|---|
| 第一次进仓库，不知道资料在哪里 | 第二章：接手已有工作 |
| 开工、日志和完成要敲很多命令 | 第三章：一项任务的完整推进 |
| 同一负责人有多个Agent，容易改到同一文件 | 第四章：并行协作 |
| 想提交方案，但应先由维护者审核 | 第五章：提案与审核 |
| 要录入一批任务，又怕半批成功 | 第六章：批量录入 |
| 数据写成功，网络或钩子让命令失败 | 第七章：Git失败恢复 |
| 文档移动后很多地址打不开 | 第八章：文件引用维护 |
| 状态页、资源和历史记录需要整理 | 第九章：维护工作区 |

团队示例使用`--message-file`满足必填策略。先在已被Git忽略的工作目录准备说明，
每次正式写入都描述这次改动，而非重复同一段固定文案。以下PowerShell例子用于开工：

```powershell
New-Item -ItemType Directory -Force .work | Out-Null
git check-ignore .work/start-message.txt
$message = @'
chore(tasks): begin the reservation boundary task

Claim the approved task and record its scope, acceptance conditions and
initial focus membership so other collaborators can see the planned work.

Verified the current task state and expanded workflow before execution.
'@
Set-Content -Encoding utf8 .work/start-message.txt $message
```

`git check-ignore`应确认文件会被忽略；若没有，先按团队约定配置工作目录，或把说明
文件放到仓库外。不要为了执行示例而覆盖既有说明。后续的其他说明文件也须预先
准备；演练可使用与计划执行相同的说明，正式说明中的验证事实必须真实。

## 二、接手已有工作

接手同事的任务时，先运行`clerk tasks list --state active --width 100`。
publisher说明谁创建了任务，claimant说明当前谁在执行；`unclaimed`表示尚未认领。
默认每任务一行，适合扫读来源与执行状态。横向比较负责人、里程碑和创建时间等短字段时用`--long`，终端较窄时自动省略并提示隐藏列；
逐个核对验收、备注与证据路径时用`--details`。要看长期负责人，可使用
`clerk tasks list --columns id,title,owners`，或在竖排详情里查owners。
完整正文、依赖与日志使用`clerk context tasks <编号>`。

工具默认完整输出。任务多于一屏时，用`-o truncate`只看预览，底部计数说明还有多少结果。希望保留输出历史、按需继续时用`clerk tasks list --overflow more`：回车追加一行、空格追加一屏、q停止。需要来回逐项浏览时运行
`clerk tasks list --overflow pager`，表头固定，用空格翻页、`/`搜索、`n`跳到下一处、`q`退出。
需要复制全部结果或在终端回滚中查看时用`--overflow full`。
给Agent或脚本使用`--json`，或把输出重定向到文件，结果始终完整。
`clerk context tasks <编号> --overflow pager`也可逐屏查看任务说明和关联资料，
源行号继续区分原文行与显示折行。


同一人换Git账号后继续工作，先用`whoami`核对Git邮箱和person归并，再按需用`id`设置本机operator。
新记录的发布者取新Git账号，旧记录保留原发布者与创建时间；认领与释放只改变当前执行人。
因此历史作者、长期负责人和当前认领者能分别回答“谁创建”“归谁负责”“谁正在做”。


同事交来任务编号，但说明、设计依据和验证记录分散在多处。先收集资料，避免仅凭
任务标题推测范围，也避免重复已经完成的工作。

```sh
clerk version --json
clerk whoami
clerk kinds
clerk repos --check
clerk brief
clerk tasks show APP-001
clerk context tasks APP-001
clerk workflow show start
```

先确认构建版本和身份，再阅读任务状态、验收条件、说明附件、关系、最近日志与仓库
观察。上下文显示`truncated`时打开来源全文；出现缺失资料诊断时先补查，不把省略
的内容当作不存在。远端引用是预约检查，需要最新远端时另行执行`clerk sync fetch`。

如果任务已`active`且认领正确，继续当前任务，不重新执行要求空认领的开工模板。
若任务`held`，先查阻塞是否解除，再依据迁移声明恢复。负责人是人员身份，不要为
每个Agent伪造一个负责人账号；对话与修改范围使用活动会话。

成功的结果是能够回答：任务要交付什么、验收依据是什么、依赖谁、已经做过什么、
现在处在哪个状态。`context`提供可追溯的资料集合，是否满足接手条件仍需阅读判断。

终端较窄时，默认排版会按可见宽度换行，源行号只标在原文行的开头。需要把内容交给
Agent或复制时，可用`clerk context tasks APP-001 --plain`关闭这两项排版，或直接
用`--json`读取原文和来源。要固定展示效果，使用`--width 80`；它与`--plain`互斥。

## 三、一项任务的完整推进

一项改动通常要同时更新认领、状态、说明、关注事项、日志和页面。模板把团队约定固定
下来，`batch`保证这些管理文件修改共同成功或失败，并减少重复提交和推送。

### 3.1 建立范围并开工

前提是已有获准执行的`todo`任务，尚未认领。新任务先按`help tasks`提供合法人员、
分类与仓库字段执行`new`，记录实际返回编号。接下来预览与演练：

```sh
clerk sync update
clerk workflow preview start --task APP-001 --description "范围：修正预约校验边界；验收：边界回归通过。"
clerk batch --template start --task APP-001 --description "范围：修正预约校验边界；验收：边界回归通过。" --dry-run --message-file .work/start-message.txt
clerk batch --template start --task APP-001 --description "范围：修正预约校验边界；验收：边界回归通过。" --message-file .work/start-message.txt --sync ff-only
clerk context tasks APP-001
clerk focus show
```

`preview`只检查参数和展开，不能保证当前状态允许开工；演练执行实际检查后回滚。
正式执行应显示一次提交，任务进入`active`，说明、关注事项和日志一起更新。
`batch`要求干净工作树；代码改动和模板文件先按团队流程整理，不能混入其他人的修改。

### 3.2 留下能被接手者找到的依据

实现中发现还需某份契约，建立明确关系；阶段验证记录显式关联任务：

```sh
clerk tasks link APP-001 contract contracts 01 --message-file .work/contract-link-message.txt
clerk logs add --title "预约校验边界回归" --tasks APP-001 --body "边界回归通过，证据已写入完成说明。" --message-file .work/validation-message.txt
clerk context tasks APP-001
```

前提是契约`28`实际存在，且任务声明了`contract`关系。日志不能只写“预约检查问题”，
否则旧式正文检索可能找不到它；显式关联让任务编号与条目一起保存。
可以关联多项任务，但每个关联都应表示这条日志确实涉及该任务。

### 3.3 送审与完成

先保存真实验证结果和代码提交。送审模板通常接受`task/report`；完成模板通常还要
`evidence/commit`。实际参数以`workflow show`为准。下面PowerShell示例中`$report`
为事先整理好的交付说明，`$codeCommit`是本次实现提交，不能用模板替你编造证据。

```powershell
$report = "交付：预约校验边界修正。验证：边界回归通过。限制：无新增。"
$codeCommit = git rev-parse --short HEAD
clerk workflow preview review --task APP-001 --report $report
clerk batch --template review --task APP-001 --report $report --message-file .work/review-message.txt
clerk workflow preview finish --task APP-001 --report $report --evidence tasks/APP-001-done.md --commit $codeCommit
clerk batch --template finish --task APP-001 --report $report --evidence tasks/APP-001-done.md --commit $codeCommit --message-file .work/finish-message.txt
clerk tasks show APP-001
clerk check
git status --short
```

仅在复核或合并要求都满足后执行完成。完成模板保存报告、迁移状态、移除关注事项、
写日志并渲染；部分模板先由`finish`步骤生成上例的证据附件，所以路径可在事务中
成为有效文件。若证据是其他文件，应填写真实存在的路径。
预期任务为`done`，验收证据和实现提交可查，Git提交及推送阶段成功，工作树无未解释改动。

## 四、同一负责人下的并行协作

两个Agent同属人员`alice`，分别处理预约检查实现与文档。人员身份用于责任归属，
会话登记用于表达两个对话的活动和修改范围，避免把负责人列表变成对话列表。

```sh
clerk session open --chat reservation-code --tasks APP-001 --paths src/reservation,tests/reservation.rs --ttl 3600 --json
clerk session open --chat reservation-docs --tasks APP-002 --paths docs/reservation --ttl 3600 --json
clerk session list
```

记录各自返回的会话编号。文档Agent准备扩大到实现目录时先查看已有会话：

```sh
clerk session show CODE-SESSION-ID
clerk session scope DOCS-SESSION-ID --add src/reservation --strict
```

若代码会话尚有效，严格模式应拒绝重叠范围，原登记不变。先协调谁处理共同文件，
确定后调整范围。继续工作时`touch`续期，完成时`close`。

```sh
clerk session touch CODE-SESSION-ID --ttl 3600
clerk session close DOCS-SESSION-ID
```

过期会话不永久占用路径，但也不证明对方已停止外部编辑。这些登记仅在同一克隆及
其worktree间共享；不同机器需通过日志、Git和团队沟通协调。会话不会产生Git提交。

## 五、把改动作为提案交给维护者

协作者想补任务，但维护者应先检查范围。请求捕获保存“要执行的操作”，让审核时
仍能按当前规则、目标变化与Git基线判断，适合暂不直接落业务数据的改动。

```sh
clerk tasks new --title "补充预约校验边界示例" --prefix APP --request add-reservation-example
clerk request list
clerk request show add-reservation-example
git diff
```

请求捕获不创建业务任务，也不自动提交；通过团队已有的分支、提交与审核流程传递
请求文件。捕获后正文已经保存在请求中，后续修改原输入文件不会改变提案。

维护者在包含请求的干净基线上检查并演练：

```sh
clerk request show add-reservation-example
clerk request apply add-reservation-example --dry-run --message-file .work/accept-message.txt
clerk request apply add-reservation-example --message-file .work/accept-message.txt
clerk tasks list --filter "title ~ 预约检查"
clerk check
```

成功后才分配实际任务编号，请求被移除，回执与业务修改共同提交。多个请求一起
`apply`时共用事务，某项失败不保留半批结果。没有名称则选择所有待处理请求，审核
时应确认清单。基线冲突时核对差异后重提，不用`--force`绕过；相同内容不能重复应用。
不接受的提案用`request reject <name> --reason ...`留理由，提交说明由审核执行者提供。

## 六、批量录入并验证失败回滚

从规划表录入几十项任务时，CSV比逐条新建更方便。先在主手册第三章的练习工作区
准备UTF-8文件，以下示例使用该工作区已有的`title/note`字段：

```csv
title,note
补充预约检查说明,验收为例子可执行
检查错误提示,验收为提示可定位问题
```

```sh
clerk tasks new --from import.csv --dry-run
clerk tasks list
clerk tasks new --from import.csv
clerk tasks list --sort id --csv
clerk check
```

演练之后记录数量和编号分配应保持不变；正式执行后增加两条任务。工作区若要求编号
前缀，命令中也要提供。要理解回滚，可在隔离练习中把其中一行的必填标题清空再
演练，预期整批拒绝，前一行也不留下。不要在正式仓库制造非法数据来测试。

实际批量执行失败后先看回执：业务阶段失败可修正CSV重试；Git阶段失败应走下一章
的恢复流程，不能再次导入同一批来重试网络。

## 七、Git失败后的安全恢复

网络中断时，一条命令可能已经写完数据、创建提交，只是推送结果未知。回执把这些
阶段分别记录，避免用户因为退出码非零就重复创建任务或追加日志。

```sh
clerk operation status --json
git status --short
git log -1
```

| 看到的情况 | 操作 |
|---|---|
| 数据未写入或已回滚 | 处理原错误，重新执行业务命令 |
| 数据已保存、提交钩子失败 | 查看钩子输出，修复原因，保留原业务文件再重试Git |
| 提交成功、推送未知 | 恢复网络后重试Git；工具先核对远端是否已经接收 |
| 工作区出现别人的改动 | 先协调并检查，不把它们暂存到本次提交 |
| HEAD或受影响文件已改动 | 自动重试会停止，需要人工审查 |

确认只剩Git收尾后：

```sh
clerk operation retry op-EXAMPLE-ID
clerk operation status op-EXAMPLE-ID --json
```

把编号替换为失败命令返回的实际回执；若最近一条就是目标，可以省略编号。
重试保留原提交说明，不重放业务动作，也不会接受新的`--message`。
结果应保持业务记录数量不变，提交不重复，推送成功。旧回执若未保存说明，按错误
提示人工处理，不能根据新配置猜测当时意图。

若原命令是在写前发现远端领先而被拦截，数据尚未写入，则使用`sync update`
安全快进，再重新执行原业务命令。工作树不干净或分支已分叉时停止并保留现场。

## 八、文件搬家后修复阅读路径

设计文件改名后，其他文档仍引用旧地址。手工逐处查找容易遗漏，Git重命名历史可以
提供可核对的修复依据。先按团队流程完成文件移动并保留Git重命名证据，再预览：

```sh
clerk links check docs
clerk links repair docs --linkify --json
```

阅读输出中的文件、行号、原地址、候选和状态。确定唯一的修复后，在干净基线上应用：

```sh
clerk links repair docs --linkify --apply --message-file .work/link-message.txt
clerk links check docs
git show --stat HEAD
```

`--linkify`同时把确认存在的行内代码文件路径变成Markdown链接，方便直接打开。
`ambiguous`表示历史不唯一，`missing`表示没有当前目标或证据，均需要人工判断。
生成区块应改原数据或渲染声明再`render`。本地文件存在性检查不会证明章节锚点、
行号含义或外部URL可访问；还要在Markdown阅读器中抽查关键链接。

## 九、维护状态页、资源和历史

维护者要回答“当前能力是什么、哪些限制已解决、资源是否发生变化”，可用不同种类
分别记录，再投影到页面；避免把所有信息都塞进任务标题或每日长日志。

```sh
clerk kinds
clerk help milestones
clerk help verify
clerk help limits
clerk help assets
clerk scope
clerk doctor
```

里程碑组织阶段目标，验证记录保存证据，限制用声明的解决和重开迁移维护。
资源哈希检查说明文件是否变化，来源与许可仍须人工提供，确认动作按Git身份留痕。
`assets verify`只比较，不应为消除差异而随意更新登记。

记录修改后先检查再渲染：

```sh
clerk check
clerk render --dry-run --message-file .work/status-message.txt
clerk render --message-file .work/status-message.txt
```

已完成任务很多时，可按允许归档的状态整理：

```sh
clerk tasks archive APP-001 --message-file .work/archive-message.txt
clerk tasks list --data-scope archived
clerk context tasks APP-001
clerk tasks restore APP-001 --message-file .work/restore-message.txt
```

归档保留身份和历史，恢复不把状态重置为待办。需要重新开工时还要看合法迁移。
若外部编辑造成封存漂移，先审查差异；可恢复误改，或在确认合理后带理由`seal`。
不要用重新封存掩盖未知变化，也不要直接修改现状页的生成区块。

交付前核对`check`结果、渲染差异、Git提交和推送状态；干净工作树只是最后一项
检查，不能替代验收证据。命令与边界的完整介绍见[用户手册](user-manual.md)。

## 十、边操作边组成一批修改

Agent准备同时创建任务、补说明和登记关注事项时，可以先收集成临时草稿，全部确认后再写入。
收集阶段不需要提交说明，不占用正式编号，也不会每条命令产生一次提交。

```sh
clerk tasks new --prefix APP --title '校验预约边界' --batch reservation --step-name task
clerk tasks brief @task --body '范围：输入检查。验收：非法输入被拒绝。' --batch reservation
clerk focus add @task --batch reservation
clerk render --batch reservation
clerk batch show reservation --json
clerk batch apply reservation --preview
clerk batch apply reservation --dry-run --message-file .work/reservation-message.txt
clerk batch apply reservation --message-file .work/reservation-message.txt
```

收集结果明确显示“尚未执行”，`@task`在执行时替换为正式编号。预览只检查计划；演练会
执行真实校验后回滚，不消耗草稿。正式成功后只有一次Git提交，草稿自动删除，审计回执保留。
执行失败时读取步骤号，检查`batch show reservation`；需要重做时先`batch drop reservation`，
再按新意图收集。目标已有并行修改时拒绝旧草稿，不自动覆盖。Git失败时用回执中的编号
执行`operation retry`，不能重新应用草稿来解决推送失败。

程序已经能生成完整计划时，直接通过`batch --file - --input-format json`提交标准输入即可，
不必创建草稿。共享关注集合用`focus`维护，与临时`batch`分开。迁移与格式见[批处理说明](../features/batch.md)。


## 十一、发现可用输入与定位资料

不知道命令接受什么时，在PowerShell加载`clerk completion powershell | Out-String | Invoke-Expression`，
然后输入`clerk tasks show app-`按Tab，从实际编号候选中选择；词表和模板参数也直接按Tab查看。
接手一个小写编号时直接`clerk tasks show app-043`，只有部分编号时加`--match contains`；
多条候选不会自动选中，先核对标题再补全规范编号。

任务列表很短但仍想搜索时，用`clerk tasks list -o pager --paging always`。
输入`/Clerk`回车，底部显示当前第几处与总数，n/N前后跳转，Alt+r可切换到正则，
例如`Clerk|预约`同时查两个关键词。搜索高亮只定位已有结果，不过滤列表。
只想保留符合条件的对象时使用`--filter 'title matches "Clerk|预约"'`，
过滤作用于完整字段，即使该字段未进入当前表格也能筛选。

长期偏好more时，在工作区terminal声明中设置overflow为more；
需要复制纯文本时加`--color never`，交给Agent时使用JSON，不包含颜色码。
如果上下文出现`Truncated: true`，这是正文摘要达到字符上限，继续执行
`clerk context tasks APP-001 --full --overflow pager`即可在分页中阅读完整正文；
交给Agent则使用`clerk context tasks APP-001 --full --json`。这两种方式仍保持
一层关系与最近五条日志的范围；单独设置`--overflow full`不能取消正文摘要。
