# quarkpan

`quarkpan` 是基于 `libquarkpan` 的夸克网盘命令行工具。

它面向直接使用终端的场景，负责把底层库能力包装成可恢复、可观察、可取消的命令行体验。

## 功能概览

当前命令行支持：

- `auth`
  管理持久化 Cookie
- `list`
  列出目录内容
- `folder create`
  创建目录
- `download`
  下载文件
- `download-dir`
  下载整个目录
- `upload`
  上传文件
- `upload-dir`
  上传整个目录

同时支持：

- 仅在交互式 TTY 中显示进度条
- `--color auto|always|never`
- Ctrl+C 取消
- 中断后保留 `.quark.task` 任务文件
- 下载中断后的显式重试与续传
- 上传分片失败后的显式重试
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
- `auth set-cookie` 持久化到系统配置目录

Cookie 内容应为完整的 `key=value; key2=value2` 格式。
`auth set-cookie` 需要显式指定来源：

- `--cookie`
- `--from-stdin`
- `--from-nano`
- `--from-vi`

## 使用示例

### 列出根目录

```bash
quarkpan --cookie 'k1=v1; k2=v2' list
```

### 持久化 Cookie

```bash
quarkpan auth set-cookie --from-stdin
quarkpan auth show-source
```

也可以显式传入：

```bash
quarkpan auth set-cookie --cookie 'k1=v1; k2=v2'
```

也支持通过标准输入：

```bash
printf 'k1=v1; k2=v2\n' | quarkpan auth set-cookie --from-stdin
```

如果直接在终端运行：

```bash
quarkpan auth set-cookie --from-stdin
```

CLI 会先提示：

```text
paste cookie, then press Enter:
```

也支持通过编辑器：

```bash
quarkpan auth set-cookie --from-nano
quarkpan auth set-cookie --from-vi
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

### 下载目录

```bash
quarkpan download-dir --folder-id <folder_id> --output ./backup
```

### 恢复或合并目录下载

```bash
quarkpan download-dir --folder-id <folder_id> --output ./backup -c
quarkpan download-dir --folder-id <folder_id> --output ./backup -c -o
```

### 上传文件

```bash
quarkpan --cookie 'k1=v1; k2=v2' upload --file ./file.bin --parent-folder 0
```

### 恢复上传

```bash
quarkpan --cookie 'k1=v1; k2=v2' upload --file ./file.bin --parent-folder 0 -c
```

### 上传目录

```bash
quarkpan upload-dir --dir ./photos --parent-folder 0
```

### 恢复或合并目录上传

```bash
quarkpan upload-dir --dir ./photos --parent-folder 0 -c
quarkpan upload-dir --dir ./photos --parent-folder 0 -c -o
```

## 任务文件说明

下载和上传在中断、报错或收到 Ctrl+C 后，会保留：

```text
${filename}.quark.task
```

目录任务使用与目录同级同名的任务文件，例如：

```text
photos.quark.task
```

用途：

- 下载时记录远端文件身份信息和目标输出路径
- 上传时记录预检得到的上传会话信息和已完成的分片状态

成功完成后，任务文件会自动删除。

`Ctrl+C` 时：

- 当前传输会尽快停止
- 已生成的任务文件会保留
- 不会把取消误当作普通失败继续重试
- 后续可使用 `-c` 恢复

进度条显示规则：

- 只在交互式终端显示
- 定时任务、管道和重定向默认不显示
- 下载和上传都会显示当前文件名

## 当前限制

- 目录和文件操作当前以 ID 为主
- 下载当前只支持按文件 ID
- CLI 当前不负责复杂的路径递归解析
- 上传恢复依赖本地文件内容未变化，因此继续上传前会重新校验 `size/md5/sha1`
- 云端覆盖当前通过删除旧文件后再上传实现
- CLI 已移除 `--json` 输出模式

## License

`GPL-3.0-only`
