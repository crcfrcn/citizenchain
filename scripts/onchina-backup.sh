#!/usr/bin/env bash
# Card 05:大市机房——每日 pg_basebackup 全量 + 持续 WAL 归档(= PITR)。
# 全量落 NAS;WAL 由内嵌 PG 的 archive_command 持续归档到同 NAS(见 onchina 自管 postgresql.conf)。
# 建议 cron 每日执行本脚本。
set -euo pipefail

: "${ONCHINA_PG_BIN_DIR:?需指向内嵌 PG 的 bin 目录(含 pg_basebackup)}"
: "${ONCHINA_PG_PORT:=5433}"
: "${ONCHINA_PG_BACKUP_DIR:?需指向 NAS 全量备份根目录}"

STAMP="$(date +%Y%m%d_%H%M%S)"
DEST="$ONCHINA_PG_BACKUP_DIR/basebackup_$STAMP"
mkdir -p "$DEST"

"$ONCHINA_PG_BIN_DIR/pg_basebackup" \
  -h 127.0.0.1 -p "$ONCHINA_PG_PORT" -U postgres \
  -D "$DEST" -Ft -z -Xs -P

echo "[backup] 全量备份完成: $DEST"
echo "[backup] WAL 持续归档由 postgresql.conf 的 archive_command 落 NAS(ONCHINA_PG_WAL_ARCHIVE_DIR)。"

# 可选:保留最近 N 份全量,清理更旧的(默认保留 14 份)。
KEEP="${ONCHINA_PG_BACKUP_KEEP:-14}"
ls -1dt "$ONCHINA_PG_BACKUP_DIR"/basebackup_* 2>/dev/null | tail -n +"$((KEEP + 1))" | xargs -r rm -rf
