// 某机构下的账户列表。
//
// CID 账户列表只展示链上同步状态,不提供后台手动激活入口。
// 链上注册/注销由区块链软件完成后同步回 CID。

import React from 'react';
import { Button, Popconfirm, Space, Table, Tag } from 'antd';
import {
  type InstitutionAccount,
  type MultisigChainStatus,
} from './api';
import { tryEncodeSs58 } from '../utils/ss58';

interface Props {
  accounts: InstitutionAccount[];
  loading: boolean;
  /** 是否允许在本视图删除自定义账户;注册局详情页传 false 即为只读。 */
  canDelete: boolean;
  /** 仅可管理视图注入;canDelete=false 的只读视图不需要。 */
  onDelete?: (accountName: string) => void;
}

const STATUS_LABEL: Record<MultisigChainStatus, string> = {
  NOT_ON_CHAIN: '未上链',
  PENDING_ON_CHAIN: '上链中',
  ACTIVE_ON_CHAIN: '已上链',
  REVOKED_ON_CHAIN: '已注销',
};

const STATUS_COLOR: Record<MultisigChainStatus, string> = {
  NOT_ON_CHAIN: 'default',
  PENDING_ON_CHAIN: 'orange',
  ACTIVE_ON_CHAIN: 'green',
  REVOKED_ON_CHAIN: 'purple',
};

export const AccountList: React.FC<Props> = ({
  accounts,
  loading,
  canDelete,
  onDelete,
}) => {
  return (
    <Table<InstitutionAccount>
      rowKey={(r) => `${r.cid_number}|${r.account_name}`}
      loading={loading}
      dataSource={accounts}
      pagination={false}
      columns={[
        {
          title: '序号',
          width: 70,
          align: 'center',
          render: (_v, _row, index) => index + 1,
        },
        { title: '账户名称', dataIndex: 'account_name', width: 200 },
        {
          title: '账户地址',
          dataIndex: 'account_id',
          // SS58 地址完整显示不截断(地址是给人核对的),小号等宽字体允许换行
          render: (v: string | null) => {
            if (!v) return '-';
            return (
              <span style={{ fontSize: 11, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                {tryEncodeSs58(v) || v}
              </span>
            );
          },
        },
        {
          title: '链上状态',
          dataIndex: 'chain_status',
          width: 120,
          render: (v: MultisigChainStatus) => (
            <Tag color={STATUS_COLOR[v] || 'default'}>{STATUS_LABEL[v] || v}</Tag>
          ),
        },
        {
          title: '交易哈希',
          dataIndex: 'chain_tx_hash',
          render: (v: string | null) =>
            v ? (
              <span style={{ fontSize: 11, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                {v.slice(0, 14)}...{v.slice(-8)}
              </span>
            ) : (
              '-'
            ),
        },
        {
          title: '创建时间',
          dataIndex: 'created_at',
          width: 170,
          // 账户读侧为链上真源,链上无创建时间戳:空值显示 '-',不再回退成 1970。
          render: (v: string | null) => (v ? new Date(v).toLocaleString('zh-CN') : '-'),
        },
        {
          title: '操作',
          width: 160,
          align: 'center',
          render: (_v, row) => {
            const canDeleteRow = canDelete && row.can_delete;
            // 删除按钮：仅后端判定可删除的自定义命名账户显示。
            const deleteCell =
              canDeleteRow ? (
                <Popconfirm
                  title={`确认发起关闭账户 "${row.account_name}" 提案?`}
                  description="发起本机构「关闭账户」内部投票提案,通过后才在链上生效"
                  onConfirm={() => onDelete?.(row.account_name)}
                  okText="发起提案"
                  okButtonProps={{ danger: true }}
                  cancelText="取消"
                >
                  <Button size="small" danger type="link">
                    删除
                  </Button>
                </Popconfirm>
              ) : null;
            if (deleteCell) return <Space size={4}>{deleteCell}</Space>;
            if (row.can_close && row.chain_status === 'ACTIVE_ON_CHAIN') {
              return <span style={{ color: '#999', fontSize: 12 }}>链上账户不可删</span>;
            }
            return <span style={{ color: '#999', fontSize: 12 }}>-</span>;
          },
        },
      ]}
    />
  );
};
