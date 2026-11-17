# 社区图书馆预约系统示例

这是一个虚构的软件开发项目。团队准备给社区图书馆增加活动预约功能，Alice负责名额与重复预约检查，Bob负责管理员指南。Clerk用于管理开发资料；示例不实现预约服务。

## 一、建立独立工作区

需要PowerShell 7、Git与已安装的Clerk。从Clerk源码根目录执行：

```powershell
./examples/library/setup.ps1 -Destination ../library-demo
```

目标目录必须不存在。脚本只在该目录设置虚构的Git身份，不修改全局配置；复制本示例声明、执行[初始化批处理](seed.toml)，然后建立本地初始提交。没有远端，自动提交、推送与同步均关闭。

脚本将[workspace.toml](workspace.toml)放进新工作区的`.clerk/`，把[kinds](kinds)、[registry](registry)、[workflows](workflows)复制到`.clerk/config/`，把[templates](templates)复制到`.clerk/`。它还建立契约索引、状态页的五个区块和配置基线提交，再执行批处理；手动建立工作区时也需要这些准备。

## 二、接手任务

进入新工作区：

```sh
clerk whoami
clerk tasks list
clerk context tasks APP-001 --full
clerk contracts list
clerk logs list
clerk check
```

`APP-001`是预约校验任务，`DOC-001`是操作指南任务。上下文包含任务说明、预约规则契约和显式关联日志。页面生成结果见新工作区的`status.md`。

## 三、开始、送审与完成

先预览模板，再执行：

```sh
clerk workflow preview start --task APP-001 --description "实现预约校验，覆盖名额不足、重复预约与成功预约。"
clerk batch --template start --task APP-001 --description "实现预约校验，覆盖名额不足、重复预约与成功预约。"
clerk logs add --title "预约校验进展" --tasks APP-001 --body "已确定检查顺序，正在实现失败提示。"
clerk workflow preview review --task APP-001 --report "说明实现与验证结果，再提交复核。"
```

确认实际工作达到送审条件后执行review模板。完成模板还需要实际证据路径和真实代码提交；不要把虚构验证结果或占位哈希当成已完成事实。模板结构见[start](workflows/start.toml)、[review](workflows/review.toml)与[finish](workflows/finish.toml)。

## 四、复核与批处理

复核正文必须具备声明的五个章节。可把以下Markdown保存到临时文件，再执行`clerk reviews new --slug reservation-validation --title "预约校验复核" --file <文件路径>`：

```markdown
## 结论
待验证。

## 依据与方法
检查预约规则契约及实际测试记录。

## 发现
记录观察到的行为。

## 建议与验收
列出需要修复的情况及验收标准。

## 未验证边界
列出本轮未覆盖的情况。
```

一组操作也可先收集到临时草稿：

```sh
clerk tasks new --prefix APP --title "补充取消预约规则" --types ability --owners bob --repos app --batch cancellation --step-name task
clerk tasks brief @task --body "明确取消截止时间及名额释放规则。" --batch cancellation
clerk focus add @task --batch cancellation
clerk batch show cancellation
clerk batch apply cancellation --preview
clerk batch apply cancellation --dry-run
clerk batch apply cancellation
clerk check
```

草稿属于本机临时状态，成功执行后自动清理。更多用法见[使用手册](../../docs/guide/user-manual.md)与[场景指南](../../docs/guide/scenarios.md)。
