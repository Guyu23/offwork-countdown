# Cloudflare 镜像

这个 Worker 提供两件事：

1. 托管 `docs/` 里的发布页；
2. 把 `/download/OffWorkCountdown.exe` 代理到 GitHub 最新 Release，并在 Cloudflare 边缘缓存。

## 部署

需要 Node.js 和 Cloudflare 账号。

```powershell
npx wrangler login
cd deploy/cloudflare-worker
npx wrangler deploy
```

部署后，在 Cloudflare 控制台给 Worker 绑定自定义域名，例如 `offwork.guyu23.xyz`。这样发布页和下载镜像就在同一个域名下。

## 说明

- 下载路由始终读取 GitHub 的 `releases/latest`，不需要每次手动上传 exe。
- 边缘缓存 1 小时；新版本发布后，最多 1 小时自动同步。
- 如果已经在用别的静态托管，也可以只把 `worker.js` 的下载路由搬到现有 Worker 里。
- 页面文件来自仓库的 `docs/`，Worker 不在仓库里保存任何文件。
