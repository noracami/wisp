#!/bin/bash
cd /opt/wisp
docker compose -f docker-compose.prod.yml pull wisp 2>&1
docker compose -f docker-compose.prod.yml up -d wisp 2>&1
echo "Deploy completed at $(date)"
