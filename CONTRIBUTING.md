# 贡献说明

先阅读[概念模型](docs/architecture/model.md)和[声明格式](docs/architecture/declarations.md)。字段、编号、迁移和投影规则优先由声明表达；多个入口共用校验与执行逻辑。

源码、标识符、注释、CLI消息和提交信息使用英文，用户文档使用中文。Rust格式交给rustfmt。测试使用临时工作区和虚构数据，不依赖真实账号、仓库或网络服务。

提交前执行：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked
```

涉及行为时覆盖相应边界和失败恢复；涉及文档时检查链接与可执行示例。提交说明采用约定式标题，说明行为变化及验证范围。请勿提交凭据、个人工作区、构建缓存或本机运行状态。

提交贡献即表示你有权按本项目的[Apache License 2.0](LICENSE)提供这些内容。
