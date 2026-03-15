# libquark

`libquark` 是一个围绕夸克网盘接口的 Rust workspace，当前包含一个核心库和一个命令行工具：

- `libquarkpan`
  Rust 异步库，负责夸克网盘的目录列表、目录创建、下载、上传、分片上传和恢复相关能力。
- `quarkpan`
  基于 `libquarkpan` 的命令行程序，提供可直接使用的上传、下载、列目录和建目录命令。

## 当前状态

当前 workspace 重点覆盖以下能力：

- 使用 Quark Cookie 构造客户端
- 列出目录内容
- 创建目录
- 按文件 ID 下载
- 上传预检和快传判断
- 非快传场景下的分片上传
- 传输进度监听
- Ctrl+C 取消传输
- 基于 `${filename}.quark.task` 的 CLI 恢复机制

目前接口仍然以文件 ID 和目录 ID 为主，路径解析和更高层缓存策略预留给上层应用。

## Workspace 结构

### `libquarkpan`

适合以下场景：

- 你需要在自己的程序里直接接入夸克网盘
- 你希望自行管理上传流、下载流和恢复策略
- 你希望把目录同步、备份或其他业务逻辑放在自己的应用层

### `quarkpan`

适合以下场景：

- 你只需要一个可执行文件
- 你希望直接在终端完成上传、下载和目录操作
- 你希望中断后依靠 `.quark.task` 文件恢复传输

## Cookie 说明

当前客户端使用浏览器或官方客户端中登录后的 Cookie 发起请求。

常见使用方式：

- 直接通过 `--cookie 'k1=v1; k2=v2'` 传入
- 或写入文件后通过 `--cookie-file ./cookie.txt` 读取
- 或在 CLI 中使用环境变量 `QUARK_COOKIE`

Cookie 需要是完整的 `key=value; key2=value2` 形式。

## 文档

- 根变更记录见 `CHANGELOG.md`
- 核心库说明见 `libquarkpan/README.md`
- CLI 说明见 `quarkpan/README.md`

## License

本仓库采用 `GPL-3.0-only` 协议发布，详见根目录 `LICENSE`。
