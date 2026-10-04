// 机构操作记录共享组件。公权、注册局、教育和私权机构详情页统一复用。

import React, { useEffect, useState } from 'react';
import { Card, Table, Typography } from 'antd';
import type { AdminAuth } from '../auth/types';
import {
  PARTNERSHIP_KIND_LABEL,
  PRIVATE_TYPE_LABEL,
  SUBJECT_PROPERTY_LABEL,
} from '../subjects/labels';
import {
  useInstitutionCodeLabels,
  type InstitutionCodeLabelMap,
} from '../subjects/institutionLabels';
import { adminRequest } from '../utils/http';
import { tryEncodeSs58 } from '../utils/ss58';

type AuditLogEntry = {
  seq: number;
  action: string;
  actor_account_id: string;
  target_cid?: string | null;
  /** 结构化事实字段(后端 append_audit_log 只存事实);旧文本行/异常值回退 string */
  detail: Record<string, unknown> | string;
  created_at: string;
};

// 审计日志操作类型中文映射(代码不上前端)。
// 单一来源 = 后端 append_audit_log 各调用点的 action 字面量;
// 后端新增 action 必须同步补这里,未知值回退显示原标识兜底。
const AUDIT_ACTION_LABEL: Record<string, string> = {
  CITIZEN_CREATE: '注册局录入公民',
  CITIZEN_BIND: '历史公民绑定',
  PUBLIC_IDENTITY_SEARCH: '公开身份查询',
  APP_VOTERS_COUNT: 'App 选民人数查询',
  APP_VOTE_CREDENTIAL: 'App 投票凭证签发',
  INSTITUTION_CREATE: '创建机构',
  INSTITUTION_UPDATE: '编辑机构详情',
  INSTITUTION_ACCOUNT_CREATE: '新建账户',
  INSTITUTION_ACCOUNT_DELETE: '删除账户',
  INSTITUTION_DOCUMENT_UPLOAD: '上传资料',
  INSTITUTION_DOCUMENT_DOWNLOAD: '下载资料',
  INSTITUTION_DOCUMENT_DELETE: '删除资料',
};

const CATEGORY_LABEL: Record<string, string> = {
  GOV_INSTITUTION: '公权机构',
  PRIVATE_INSTITUTION: '私权机构',
  EDUCATION_FORM: '教育机构',
};

// 审计详情"事实字段"的人话翻译(代码不上前端)。
// 后端 detail 只存结构化事实(键小写蛇形,值为系统原值),展示翻译全在这里。
const AUDIT_DETAIL_KEY_LABEL: Record<string, string> = {
  city: '市',
  institution: '机构',
  cid_full_name: '机构全称',
  old_cid_full_name: '原机构全称',
  new_cid_full_name: '新机构全称',
  subject_property: '主体属性',
  category: '机构分类',
  private_type: '私权类型',
  partnership_kind: '合伙类型',
  parent_cid_number: '所属法人',
  old_parent_cid_number: '原所属法人',
  family_name: '姓',
  given_name: '名',
  legal_representative_cid_number: '法定代表人身份ID',
  account_name: '账户名称',
  doc_id: '资料ID',
  file_name: '文件名',
  doc_type: '资料类型',
  file_size: '文件大小',
  found: '查询命中',
  request_id: '请求ID',
  actor_ip: '来源IP',
  eligible_total: '选民总数',
  mode: '绑定方式',
  cid_number: '身份ID',
  proposal_id: '提案ID',
  eligible: '有选举权',
  year: '年度',
  batch: '批次',
  result: '结果',
  message: '说明',
  status: '状态',
  reason: '原因',
  updates: '更新条数',
  releases: '解除绑定数',
  unmatched_bindings: '未匹配绑定数',
  unmatched_releases: '未匹配解除数',
};

// 枚举值翻译:按键名选择值映射。机构代码(institution)改由后端单源标签在运行期解析(见 formatAuditDetailValue)。
const AUDIT_DETAIL_VALUE_LABEL: Record<string, Record<string, string>> = {
  subject_property: SUBJECT_PROPERTY_LABEL,
  category: CATEGORY_LABEL,
  private_type: PRIVATE_TYPE_LABEL as Record<string, string>,
  partnership_kind: PARTNERSHIP_KIND_LABEL as Record<string, string>,
  status: {
    PENDING: '待安装',
    USED: '已使用',
    ACTIVE: '已启用',
    DISABLED: '已禁用',
    REVOKED: '已吊销',
  },
  mode: { create: '新增绑定', replace: '更换绑定' },
  result: { SUCCESS: '成功', FAILED: '失败' },
};

function formatAuditDetailValue(
  key: string,
  value: unknown,
  institutionLabels: InstitutionCodeLabelMap,
): string | null {
  if (value === null || value === undefined || value === '') return null;
  if (typeof value === 'boolean') return value ? '是' : '否';
  const text = String(value);
  if (key === 'institution') return institutionLabels[text] ?? text;
  return AUDIT_DETAIL_VALUE_LABEL[key]?.[text] ?? text;
}

/** 结构化事实 → 人话(「市：锦程市；机构：政府」);旧文本行原样兜底。 */
function formatAuditDetail(
  detail: AuditLogEntry['detail'],
  institutionLabels: InstitutionCodeLabelMap,
): string {
  if (typeof detail === 'string') return detail;
  if (!detail || typeof detail !== 'object') return '';
  const parts: string[] = [];
  for (const [key, value] of Object.entries(detail)) {
    const text = formatAuditDetailValue(key, value, institutionLabels);
    if (text === null) continue;
    parts.push(`${AUDIT_DETAIL_KEY_LABEL[key] ?? key}：${text}`);
  }
  return parts.join('；');
}

interface Props {
  auth: AdminAuth;
  cidNumber: string;
}

export const OperationRecords: React.FC<Props> = ({ auth, cidNumber }) => {
  const [rows, setRows] = useState<AuditLogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const institutionLabels = useInstitutionCodeLabels();

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    adminRequest<AuditLogEntry[]>(
      `/api/admin/audit-logs?target_cid=${encodeURIComponent(cidNumber)}&limit=1000`,
      auth,
    )
      .then((next) => {
        if (!cancelled) setRows(next);
      })
      .catch(() => {
        if (!cancelled) setRows([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth.access_token, cidNumber]);

  return (
    <Card type="inner" title={`操作记录(${rows.length})`}>
      <Table<AuditLogEntry>
        rowKey="seq"
        loading={loading}
        dataSource={rows}
        pagination={rows.length > 10 ? { pageSize: 10 } : false}
        columns={[
          {
            title: '操作',
            dataIndex: 'action',
            width: 160,
            render: (v: string) => AUDIT_ACTION_LABEL[v] || v,
          },
          {
            title: '操作者账户',
            dataIndex: 'actor_account_id',
            width: 240,
            // 账户给人看时优先转 SS58;完整显示不截断,允许换行。
            render: (v: string) => (
              <Typography.Text style={{ fontSize: 12, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                {tryEncodeSs58(v) || v}
              </Typography.Text>
            ),
          },
          {
            title: '详情',
            dataIndex: 'detail',
            ellipsis: true,
            render: (v: AuditLogEntry['detail']) => formatAuditDetail(v, institutionLabels),
          },
          {
            title: '时间',
            dataIndex: 'created_at',
            width: 170,
            render: (v: string) => new Date(v).toLocaleString('zh-CN'),
          },
        ]}
      />
    </Card>
  );
};
