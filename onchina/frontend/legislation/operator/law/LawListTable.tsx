// 本级法律列表(只读)。会话派生 scope,调 /api/legislation/laws/mine;
// 行点击进详情。无缓存库,沿用 useEffect + useState + 取消标志的取数范式。

import React, { useEffect, useMemo, useState } from "react";
import { Alert, Table } from "antd";
import type { ColumnsType } from "antd/es/table";
import type { AdminAuth } from "../../../auth/types";
import { listMyLaws } from "../../api";
import type { LawView } from "../../types";
import { statusTag, tierLabel, voteTypeLabel } from "./labels";

interface Props {
  auth: AdminAuth;
  onOpen: (lawId: number) => void;
}

function versionLabel(row: LawView): string {
  const currentName = row.versionTitle?.trim() || `v${row.version}`;
  const nameOf = (version: number) =>
    version === row.version ? currentName : `v${version}`;
  const parts = [currentName];
  if (row.effectiveVersion) {
    parts.push(`生效 ${nameOf(row.effectiveVersion)}`);
  }
  if (row.pendingVersion) {
    parts.push(`待生效 ${nameOf(row.pendingVersion)}`);
  }
  return parts.join(" / ");
}

/** 本级法律列表表格。 */
export function LawListTable({ auth, onOpen }: Props) {
  const [rows, setRows] = useState<LawView[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    listMyLaws(auth)
      .then((data) => {
        if (!cancelled) {
          setRows(data);
          setLoading(false);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "加载法律列表失败");
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    auth.access_token,
    auth.institution_code,
    auth.scope_province_name,
    auth.scope_city_name,
  ]);

  const columns = useMemo<ColumnsType<LawView>>(
    () => [
      {
        title: "法律名称",
        dataIndex: "title",
        key: "title",
        render: (title: string) => (
          <span style={{ fontWeight: 600 }}>{title}</span>
        ),
      },
      { title: "层级", key: "tier", render: (_, row) => tierLabel(row.tier) },
      {
        title: "表决类型",
        key: "voteType",
        render: (_, row) => voteTypeLabel(row.voteType),
      },
      {
        title: "状态",
        key: "status",
        render: (_, row) => statusTag(row.status),
      },
      { title: "版本", key: "version", render: (_, row) => versionLabel(row) },
    ],
    [],
  );

  if (error) {
    return <Alert type="error" message={error} showIcon />;
  }

  return (
    <Table<LawView>
      rowKey={(row) => row.lawId}
      loading={loading}
      dataSource={rows}
      columns={columns}
      pagination={false}
      onRow={(row) => ({
        onClick: () => onOpen(row.lawId),
        style: { cursor: "pointer" },
      })}
      locale={{ emptyText: "本级暂无法律" }}
    />
  );
}
