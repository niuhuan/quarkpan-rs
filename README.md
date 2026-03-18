# libquark

`libquark` 是一个围绕夸克网盘接口的 Rust workspace，当前包含一个核心库和一个命令行工具：

- `libquarkpan`
  Rust 异步库，负责夸克网盘的目录列表、目录创建、下载、上传、分片上传和恢复相关能力。
- `quarkpan`
  基于 `libquarkpan` 的命令行程序，提供可直接使用的上传、下载、列目录和建目录命令。

## 当前状态

当前 workspace 重点覆盖以下能力：

- 使用 Quark Cookie 构造客户端
- 平台标准配置目录中的 Cookie 持久化
- 列出目录内容
- 创建目录
- 删除一个或多个文件或目录项
- 重命名文件或目录项
- 按文件 ID 下载
- 按目录 ID 批量下载目录
- 上传预检和快传判断
- 非快传场景下的分片上传
- 批量上传本地目录
- 传输进度监听
- 仅在交互式终端中显示彩色进度条
- Ctrl+C 取消传输
- 基于 `${filename}.quark.task` 的 CLI 恢复机制
- 基于 `目录名.quark.task` 的目录任务恢复机制

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
- 或通过 `quarkpan auth set-cookie` 持久化到系统配置目录

Cookie 需要是完整的 `key=value; key2=value2` 形式。
`quarkpan auth set-cookie` 需要显式指定输入来源，例如 `--from-stdin`、`--from-nano` 或 `--from-vi`。
使用 `--from-stdin` 时，CLI 会先提示粘贴 Cookie 再回车。

## 典型操作步骤

首次使用：

```bash
pbpaste | quarkpan auth set-cookie --from-stdin
quarkpan auth show-source
```

单文件下载并支持恢复：

```bash
quarkpan download --fid <fid> --output ./file.bin
quarkpan download --fid <fid> --output ./file.bin -c
```

单文件上传并支持恢复：

```bash
quarkpan upload --file ./file.bin --pdir-fid 0
quarkpan upload --file ./file.bin --pdir-fid 0 -c
```

目录下载并支持恢复：

```bash
quarkpan download-dir --pdir-fid <pdir_fid> --output ./backup
quarkpan download-dir --pdir-fid <pdir_fid> --output ./backup -c
quarkpan download-dir --pdir-fid <pdir_fid> --output ./backup -c -o
```

目录上传并支持恢复：

```bash
quarkpan upload-dir --dir ./photos --pdir-fid 0
quarkpan upload-dir --dir ./photos --pdir-fid 0 -c
quarkpan upload-dir --dir ./photos --pdir-fid 0 -c -o
```

重命名文件或目录项：

```bash
quarkpan rename --fid <fid> --file-name 新名字
```

删除一个或多个文件或目录项：

```bash
quarkpan delete --fid <fid1> --fid <fid2>
```

`Ctrl+C` 行为：

- 会立即取消当前传输
- 不会删除已生成的 `.quark.task`
- 之后可用 `-c` 继续

进度条行为：

- 只在交互式 TTY 中显示
- 定时任务、管道、重定向默认不显示
- 上传和下载都会显示当前文件名

## 文档

- 根变更记录见 `CHANGELOG.md`
- 核心库说明见 `libquarkpan/README.md`
- CLI 说明见 `quarkpan/README.md`

## License

本仓库采用 `GPL-3.0-only` 协议发布，详见根目录 `LICENSE`。
