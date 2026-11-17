# 版本发布与安装

## 一、发行范围

当前版本为0.1.0，公开源码位于[Clerk仓库](https://github.com/invefa/Clerk)。
当前以源码安装为主；维护者发布二进制时，Windows x64包包括clerk.exe、构建信息、文档、许可、源码与声明示例。
其他系统可从源码构建；不提供未经验证的平台二进制。
发行包发布到[GitHub Releases](https://github.com/invefa/Clerk/releases)，不发布到crates.io。

使用方法与功能场景分别见[使用手册](../guide/user-manual.md)和[场景指南](../guide/scenarios.md)。

## 二、下载与升级

若所选Release提供附件，下载clerk-0.1.0-windows-x86_64.zip和SHA256SUMS，在解压前核对压缩包的SHA-256：

```powershell
Get-FileHash ./clerk-0.1.0-windows-x86_64.zip -Algorithm SHA256
Get-Content ./SHA256SUMS
Expand-Archive ./clerk-0.1.0-windows-x86_64.zip -DestinationPath ./clerk
```

把解压目录加入用户PATH，或直接运行其中的clerk.exe。升级时先退出正在运行的Clerk进程，
替换工具安装目录；保留业务仓库的配置、资料和本机元数据。
新窗口中核对实际命中的工具和构建提交：

```powershell
Get-Command clerk
clerk version --json
```

version输出包含版本、构建提交、dirty与格式版本。正式包的dirty应为false，
commit应与Release标签指向的提交和包内build.json一致。
进入已有工作区后运行`clerk check`，再恢复写入。

## 三、命令变更

首版发行前的源码构建曾使用其他命令形状，升级时须核对以下变更：

| 旧写法 | 当前写法 |
|---|---|
| run | batch |
| 任务批次batch | 关注事项focus |
| --archived | --data-scope archived |
| -j输出JSON | --json |
| -t输出TSV | --tsv；-t现在表示template |
| -c输出CSV | --csv；-c现在表示columns |
| -m输出Markdown | --md；-m现在表示message |

不保留这些旧命令的兼容入口。迁移细节见[批处理](../features/batch.md)、
[终端参数](../features/terminal.md)和[本机状态](local-state.md)。
Agent与脚本使用完整命令、动词、长选项及枚举值。

## 四、构建与发布

在干净的源码提交上使用仓库固定的Rust工具链与PowerShell 7，依次执行：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
./scripts/package.ps1
```

[打包脚本](../../scripts/package.ps1)只支持Windows x64，拒绝脏工作树、构建身份覆盖和已存在的输出目录。
默认产物位于Git忽略的target/release-dist下；可用OutputDirectory显式指定其他新目录。
脚本核对二进制提交与dirty状态，收集工具、文档和示例，生成build.json、压缩包与SHA256SUMS。
测试与终端交互验收在打包前完成，脚本不代替这些检查。

发布前从压缩包解压到新目录，核对哈希、版本、帮助、补全与示例工作区检查。
在同一提交上创建带注解的v0.1.0标签，推送后创建GitHub Release并上传压缩包与校验文件。
发布说明包含功能范围、安装入口、命令迁移和验证平台；发布后再次下载附件核对哈希与构建身份。
已发布版本不替换为另一份构建，修复使用新版本号。
