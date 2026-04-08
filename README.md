# TPN Worker Dashboard

集中式多 Worker 管理面板 — 单一 Rust 二进制 + 嵌入式 SQLite + 内嵌 Web UI。

## 功能

- **多 Worker 管理** — 通过 Web 界面增删改 Worker 条目（URL、名称、备注）
- **自动轮询** — 每 30 秒采集所有 Worker 的 `/api/dashboard` 数据
- **总览卡片** — 一目了然查看所有 Worker 在线状态、版本、WG peers、代理可用数
- **Worker 详情** — 完整指标（WireGuard、代理、网络、收款地址）+ 注册历史
- **轮询历史** — 每 Worker 保留最近 1000 条快照，含延迟和状态点阵图
- **手动触发** — 即时轮询指定 Worker，验证连通性
- **零外部依赖** — 无需安装数据库或前端框架，单二进制即可运行

## 快速开始

### 从源码构建

```bash
# 构建
cargo build --release

# 或使用构建脚本
./build.sh

# 运行（默认端口 8080）
./tpn-worker-dashboard
```

### 从 Release 下载

```bash
# Ubuntu 22.04 - AMD64
wget https://github.com/<REPO>/releases/download/latest/tpn-worker-dashboard-linux-ubuntu22-amd64 \
  -O tpn-worker-dashboard && chmod +x tpn-worker-dashboard

# 运行
./tpn-worker-dashboard
```

### Docker

```bash
docker build -t tpn-worker-dashboard .
docker run -d -p 8080:8080 tpn-worker-dashboard
```

启动后打开浏览器访问 `http://<IP>:8080/dashboard`。

## 配置

通过环境变量或 `~/.config/tpn-worker-dashboard/.env` 文件配置：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `DASHBOARD_PORT` | `8080` | HTTP 监听端口 |
| `DASHBOARD_SQLITE_PATH` | `~/.config/tpn-worker-dashboard/dashboard.db` | 数据库文件路径 |
| `DASHBOARD_POLL_INTERVAL` | `30` | 轮询间隔（秒） |
| `DASHBOARD_POLL_TIMEOUT` | `10` | 单次请求超时（秒） |
| `LOG_LEVEL` | `info` | 日志级别（trace/debug/info/warn/error） |

首次启动时会自动创建配置目录和默认 `.env` 文件。

## API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/dashboard` | Web 管理界面 |
| GET | `/api/workers` | 所有 Worker + 最新快照 |
| POST | `/api/workers` | 添加 Worker |
| PUT | `/api/workers/{id}` | 编辑 Worker |
| DELETE | `/api/workers/{id}` | 删除 Worker（级联删除快照） |
| GET | `/api/workers/{id}/history` | 轮询历史（?limit=100&offset=0） |
| POST | `/api/workers/{id}/poll` | 手动触发轮询 |

### 添加 Worker 示例

```bash
curl -X POST http://localhost:8080/api/workers \
  -H "Content-Type: application/json" \
  -d '{"name": "Worker-US-1", "url": "http://10.0.0.5:3000"}'
```

### 查看所有 Worker 状态

```bash
curl http://localhost:8080/api/workers | jq .
```

## Web 界面

暗色主题，三个视图：

1. **总览** — Worker 卡片网格，显示在线状态、版本、WG peers 进度条、代理可用数、轮询延迟
2. **添加/编辑** — 弹窗表单，填写名称、URL、备注、API Key
3. **详情** — 完整指标卡片 + 最近 50 次轮询状态点阵图 + 注册历史表 + 轮询历史表

页面每 5 秒自动刷新。

## 数据存储

使用嵌入式 SQLite（WAL 模式），包含 3 张表：

| 表 | 说明 |
|----|------|
| `dashboard_workers` | Worker 条目（ID、名称、URL、备注、API Key） |
| `dashboard_snapshots` | 轮询快照（完整 JSON + 冗余聚合字段），每 Worker 保留 1000 条 |
| `dashboard_settings` | 键值设置 |

## 项目结构

```
tpn-worker-dashboard/
├── Cargo.toml
├── build.sh                  # 本地构建脚本
├── Dockerfile                # 两阶段 Docker 构建
├── .github/workflows/
│   └── build-release.yml     # CI: 四平台交叉编译 + GitHub Release
└── src/
    ├── main.rs               # 入口、路由注册、后台轮询启动
    ├── config.rs             # 环境变量配置
    ├── error.rs              # 错误类型
    ├── models.rs             # Worker /api/dashboard 响应结构体
    ├── dashboard.html        # 内嵌前端（include_str!）
    ├── db/
    │   ├── pool.rs           # SQLite 连接池
    │   ├── init.rs           # 建表
    │   ├── workers.rs        # Worker CRUD
    │   └── snapshots.rs      # 快照读写
    ├── api/
    │   ├── workers.rs        # REST API handlers
    │   └── dashboard.rs      # HTML 页面
    └── poller/
        └── mod.rs            # 后台轮询引擎
```

## 与 tpn-worker 的关系

Dashboard 通过 HTTP 轮询每个 Worker 的 `GET /api/dashboard` 接口获取数据，不需要对 Worker 做任何配置改动。只要 Worker 正常运行且网络可达，添加其 URL 即可开始监控。

## License

与 tpn-subnet 项目保持一致。
