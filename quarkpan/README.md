# quarkpan

`quarkpan` 是基于 `libquarkpan` 的夸克网盘命令行工具。

它面向直接使用终端的场景，负责把底层库能力包装成可恢复、可观察、可取消的命令行体验。

## 功能概览

当前命令行支持：

- `list`
  列出目录内容
- `folder create`
  创建目录
- `download`
  下载文件
- `upload`
  上传文件

同时支持：

- 进度条显示
- `--color auto|always|never`
- Ctrl+C 取消
- 中断后保留 `.quark.task` 任务文件
- 下次通过 `-c, --continue` 恢复传输

## 安装

### 使用 Cargo

```bash
cargo install quarkpan
```

### 从源码运行

```bash
cargo run --bin quarkpan -- --help
```

## Cookie 提供方式

CLI 需要夸克登录后的 Cookie，支持三种方式：

- `--cookie 'k1=v1; k2=v2'`
- `--cookie-file ./cookie.txt`
- 环境变量 `QUARK_COOKIE`

Cookie 内容应为完整的 `key=value; key2=value2` 格式。

## 使用示例

### 列出根目录

```bash
quarkpan --cookie 'k1=v1; k2=v2' list
```

### 逐页查看更多

```bash
quarkpan --cookie 'k1=v1; k2=v2' list --folder-id 0 --more
```

### 创建目录

```bash
quarkpan --cookie 'k1=v1; k2=v2' folder create --parent-folder 0 --name 我的文档
```

### 下载文件

```bash
quarkpan --cookie 'k1=v1; k2=v2' download --file-id <fid> --output ./file.bin
```

### 恢复下载

```bash
quarkpan --cookie 'k1=v1; k2=v2' download --file-id <fid> --output ./file.bin -c
```

### 上传文件

```bash
quarkpan --cookie 'k1=v1; k2=v2' upload --file ./file.bin --parent-folder 0
```

### 恢复上传

```bash
quarkpan --cookie 'k1=v1; k2=v2' upload --file ./file.bin --parent-folder 0 -c
```

## 任务文件说明

下载和上传在中断、报错或收到 Ctrl+C 后，会保留：

```text
${filename}.quark.task
```

用途：

- 下载时记录远端文件身份信息和目标输出路径
- 上传时记录预检得到的上传会话信息和已完成的分片状态

成功完成后，任务文件会自动删除。

## 当前限制

- 目录和文件操作当前以 ID 为主
- 下载当前只支持按文件 ID
- CLI 当前不负责复杂的路径递归解析
- 上传恢复依赖本地文件内容未变化，因此继续上传前会重新校验 `size/md5/sha1`

## License

`GPL-3.0-only`
