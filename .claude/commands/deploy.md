---
name: deploy
description: Help with Wisp project deployment tasks — SSH into the VM, check service status, view logs, manual deploy, database operations, and troubleshooting. Use this skill whenever the user mentions deploying, server status, production logs, SSH, VM, docker compose in production, cloudflare tunnel, or anything related to the Wisp production environment.
---

# Wisp Deployment Guide

You are helping with deployment and operations for the Wisp project — a Rust-based multi-platform AI assistant running on GCP.

## Infrastructure Overview

| Component | Details |
|-----------|---------|
| VM | GCP Compute Engine e2-small, asia-east1-b, Ubuntu 24.04 |
| SSH | `ssh wisp` |
| Code on VM | `/opt/wisp` |
| Domain | `wisp.miao-bao.cc` (via Cloudflare Tunnel) |
| Container Registry | `asia-east1-docker.pkg.dev/careful-broker-485510-r0/wisp/wisp` |
| App port | 8080 |
| Webhook port | 9000 (adnanh/webhook) |
| Database | PostgreSQL 17 + pgvector (localhost:5432) |

## Cloudflare Tunnel Routing

- `/hooks/*` → deploy webhook (port 9000)
- Everything else → wisp service (port 8080)

## CI/CD Pipeline

Push to `main` triggers this automated flow:

1. GitHub Actions (`.github/workflows/build.yml`) builds Docker image
2. Pushes to GCP Artifact Registry (tagged `latest` + commit SHA)
3. Calls deploy webhook at `wisp.miao-bao.cc/hooks/deploy`
4. VM runs `/opt/wisp/scripts/deploy.sh` (pulls new image + restarts)
5. Discord notification sent (success or failure)

Under normal circumstances, deployment is fully automated — just push to `main`.

## Common Operations

All commands below are run on the VM. Connect first with `ssh wisp`, then `cd /opt/wisp`.

### Check Service Status

```bash
ssh wisp
cd /opt/wisp
docker compose -f docker-compose.prod.yml ps
```

### View Application Logs

```bash
# Live logs (follow mode)
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml logs -f wisp'

# Last 100 lines
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml logs --tail=100 wisp'

# Database logs
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml logs --tail=100 db'
```

### Manual Deploy

If the automated pipeline isn't working or you need to deploy manually:

```bash
ssh wisp
cd /opt/wisp
docker compose -f docker-compose.prod.yml pull wisp
docker compose -f docker-compose.prod.yml up -d wisp
```

### Restart Services

```bash
# Restart app only
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml restart wisp'

# Restart everything (app + database)
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml restart'
```

### Environment Variables

The production `.env` file is at `/opt/wisp/.env`. To view or edit:

```bash
ssh wisp 'cat /opt/wisp/.env'
```

Required variables: `ANTHROPIC_API_KEY`, `DATABASE_URL`, `CWA_API_KEY`, `CWA_LOCATION`
Optional Discord (all seven required together to enable Discord): `DISCORD_APPLICATION_ID`, `DISCORD_PUBLIC_KEY`, `DISCORD_BOT_TOKEN`, `DISCORD_WEBHOOK_URL`, `DISCORD_CLIENT_SECRET`, `DISCORD_OAUTH_REDIRECT_URI`, `TPP_STATE_SECRET`
Optional LINE: `LINE_CHANNEL_SECRET`, `LINE_CHANNEL_ACCESS_TOKEN`
Optional: `GOOGLE_SEARCH_API_KEY`, `GOOGLE_SEARCH_ENGINE_ID`, `DEPLOY_TOKEN`

After editing `.env`, restart the app for changes to take effect.

## Database Operations

The database runs as a Docker container with a persistent volume `pgdata`.

### Connect to Database

```bash
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml exec db psql -U wisp -d wisp'
```

### Database Backup

```bash
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml exec db pg_dump -U wisp wisp > backup_$(date +%Y%m%d).sql'
```

### Database Restore

```bash
ssh wisp 'cd /opt/wisp && docker compose -f docker-compose.prod.yml exec -T db psql -U wisp -d wisp < backup.sql'
```

## Troubleshooting

### App Won't Start

1. Check logs: `docker compose -f docker-compose.prod.yml logs wisp`
2. Verify database is healthy: `docker compose -f docker-compose.prod.yml ps db`
3. Check `.env` for missing required variables
4. Ensure the image was pulled successfully: `docker images | grep wisp`

### Cloudflare Tunnel Issues

```bash
# Check cloudflared status
ssh wisp 'systemctl status cloudflared'

# View cloudflared logs
ssh wisp 'journalctl -u cloudflared --no-pager -n 50'

# Restart tunnel
ssh wisp 'sudo systemctl restart cloudflared'
```

### Webhook Not Triggering Deploy

1. Check webhook server is running: `ssh wisp 'ps aux | grep webhook'`
2. Test webhook manually:
   ```bash
   ssh wisp 'curl -s -X POST http://localhost:9000/hooks/deploy -H "X-Deploy-Token: $DEPLOY_TOKEN"'
   ```
3. Verify `DEPLOY_TOKEN` matches between GitHub Secrets and VM `.env`

### Docker Disk Space

```bash
ssh wisp 'docker system df'
ssh wisp 'docker system prune -f'  # Remove unused images/containers
```

### CI/CD Pipeline Failed

1. Check GitHub Actions: go to the repo's Actions tab
2. Common issues:
   - GCP auth failure → check Workload Identity Federation config
   - Build failure → check Cargo.toml / Dockerfile
   - Deploy webhook failure → check `DEPLOY_WEBHOOK_URL` and `DEPLOY_TOKEN` secrets
3. Discord notification shows build status with commit SHA and message

## Architecture Diagram

```
GitHub (push to main)
  → GitHub Actions (build Docker image)
    → GCP Artifact Registry (store image)
      → Deploy Webhook (wisp.miao-bao.cc/hooks/deploy)
        → /opt/wisp/scripts/deploy.sh (pull + restart)
          → Discord notification (success/failure)
```
