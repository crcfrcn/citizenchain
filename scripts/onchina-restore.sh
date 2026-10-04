#!/usr/bin/env bash
# Card 05:PITR 恢复——从某次 pg_basebackup 全量 + WAL 归档恢复到目标时间点。
# 用法(目标数据目录须为空):
#   ONCHINA_PG_DATA_DIR=<新数据目录> \
#   ONCHINA_PG_WAL_ARCHIVE_DIR=<NAS WAL 归档目录> \
#   RESTORE_BASEBACKUP=<某次 basebackup_* 全量目录(含 base.tar.gz)> \
#   [RECOVERY_TARGET_TIME='2026-06-26 12:00:00+08'] \
#   citizenchain/scripts/onchina-restore.sh
set -euo pipefail

: "${ONCHINA_PG_DATA_DIR:?目标数据目录(须为空,或先把旧目录移走)}"
: "${ONCHINA_PG_WAL_ARCHIVE_DIR:?WAL 归档目录(NAS)}"
: "${RESTORE_BASEBACKUP:?某次 pg_basebackup 全量目录(含 base.tar.gz)}"

mkdir -p "$ONCHINA_PG_DATA_DIR" "$ONCHINA_PG_DATA_DIR/pg_wal"
tar -xzf "$RESTORE_BASEBACKUP/base.tar.gz" -C "$ONCHINA_PG_DATA_DIR"
[ -f "$RESTORE_BASEBACKUP/pg_wal.tar.gz" ] && tar -xzf "$RESTORE_BASEBACKUP/pg_wal.tar.gz" -C "$ONCHINA_PG_DATA_DIR/pg_wal"

{
  echo "restore_command = 'cp $ONCHINA_PG_WAL_ARCHIVE_DIR/%f %p'"
  if [ -n "${RECOVERY_TARGET_TIME:-}" ]; then
    echo "recovery_target_time = '$RECOVERY_TARGET_TIME'"
    echo "recovery_target_action = 'promote'"
  fi
} >> "$ONCHINA_PG_DATA_DIR/postgresql.conf"
touch "$ONCHINA_PG_DATA_DIR/recovery.signal"

echo "[restore] 就绪。启动 postgres 即进入 PITR 恢复;到达目标点后自动 promote 转正常。"
