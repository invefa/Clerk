# Clerk

Clerk是一个声明驱动的仓库工作区管理工具。任务、契约、复核和日志保存在CSV、Markdown与TOML中，Git记录历史；Clerk负责校验、受控修改、查询和生成阅读页面。

## 快速开始

需要Git与Rust，工具链见[rust-toolchain.toml](rust-toolchain.toml)。在源码目录安装：

```sh
cargo install --path . --locked
clerk version --json
```

第一次使用可运行[社区图书馆预约系统示例](examples/library/README.md)。这是一个虚构的软件项目，用来展示多人协作与资料管理，不包含预约系统的业务实现。

```powershell
./examples/library/setup.ps1 -Destination ../library-demo
clerk --root ../library-demo tasks list
clerk --root ../library-demo context tasks APP-001 --full
clerk --root ../library-demo workflow preview start --task APP-001 --description "实现预约名额检查并验证重复预约。"
```

示例关闭自动提交、推送与远端同步，使用虚构人员和独立声明，可在空目录直接运行。

## 功能

- 通过声明定义对象字段、编号、权限、状态迁移、引用关系与列表视图。
- 查询当前和归档资料，过滤、排序并导出JSON、CSV、TSV或Markdown。
- 用上下文包汇总任务附件、关联契约、最近日志、仓库和来源信息。
- 用批处理、临时草稿和工作流模板完成多步操作，支持预览与演练。
- 用请求审核改动，用会话记录并行工作的范围，用结构化日志关联任务。
- 检查封存漂移与文件引用；依据可证明的Git重命名历史修复失效链接。
- 按策略提交与推送，安全快进同步，并在Git收尾失败后单独重试。
- 终端提供宽度适配、颜色、分页、增量阅读、搜索、补全和帮助。

完整用法见[使用手册](docs/guide/user-manual.md)、[场景指南](docs/guide/scenarios.md)和[文档导航](docs/README.md)。参数与动词随工作区声明变化，使用`clerk help`与`clerk explain tasks`核对。

## 验证与构建

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked --release
```

构建产物为`clerk`。Windows打包与安装见[发布说明](docs/maintenance/releases.md)，实现边界见[架构与限制](docs/architecture/review.md)。

## 贡献与许可

开发约定见[贡献说明](CONTRIBUTING.md)。本项目采用[Apache License 2.0](LICENSE)，署名信息见[NOTICE](NOTICE)。
