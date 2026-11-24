// 展示机构链上账户列表，并按权限提供关闭自定义账户的提案入口。

import React from 'react';
import { Button, Popconfirm, Table, Tag, type TableColumnsType } from 'antd';
import {
  type InstitutionAccount,
  type MultisigChainStatus,
} from './api';
import { tryEncodeSs58 } from '../codes/ss58';
function formatBalance(fen?: string | null): string {
  if (fen == null) return '';
  try {
    const value = BigInt(fen);
    const negative = value < 0n;
    const abs = negative ? -value : value;
    const yuan = abs / 100n;
    const cents = abs % 100n;
    const yuanText = yuan.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
    return `${negative ? '-' : ''}${yuanText}.${cents.toString().padStart(2, '0')} 元`;
  } catch { return ''; }
}

interface Props {
  accounts: InstitutionAccount[];
  loading: boolean;
  /** 是否允许在本视图发起自定义账户关闭提案；注册局详情页为只读。 */
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
  const columns: TableColumnsType<InstitutionAccount> = [
    {
      title: '序号',
      width: 70,
      align: 'center',
      render: (_v, _row, index) => index + 1,
    },
    { title: '账户名称', dataIndex: 'accountName', width: 200 },
    {
      title: '账户地址',
      dataIndex: 'accountId',
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
      title: '账户余额',
      dataIndex: 'balanceFen',
      width: 180,
      align: 'right',
      render: (value: string | null | undefined) => formatBalance(value) || '-',
    },
    {
      title: '链上状态',
      dataIndex: 'chainStatus',
      width: 120,
      render: (v: MultisigChainStatus) => (
        <Tag color={STATUS_COLOR[v] || 'default'}>{STATUS_LABEL[v] || v}</Tag>
      ),
    },
    {
      title: '创建时间',
      dataIndex: 'createdAt',
      width: 170,
      // 账户读侧为链上真源,链上无创建时间戳:空值显示 '-',不再回退成 1970。
      render: (v: string | null) => (v ? new Date(v).toLocaleString('zh-CN') : '-'),
    },
  ];
  // 只读机构详情及仅有协议账户的列表没有可执行操作，不保留空列。
  if (canDelete && onDelete && accounts.some(account => account.canDelete)) {
    columns.push({
      title: '操作', width: 160, align: 'center',
      render: (_value, row) => row.canDelete ? (
        <Popconfirm
          title={`确认发起关闭账户 "${row.accountName}" 提案?`}
          description="发起本机构「关闭账户」内部投票提案,通过后才在链上生效"
          onConfirm={() => onDelete?.(row.accountName)}
          okText="发起提案" okButtonProps={{ danger: true }} cancelText="取消"
        >
          <Button size="small" danger type="link">发起关闭提案</Button>
        </Popconfirm>
      ) : <span style={{ color: '#999', fontSize: 12 }}>-</span>,
    });
  }
  return <Table<InstitutionAccount>
    rowKey={row => `${row.cidNumber}|${row.accountName}`}
    loading={loading} dataSource={accounts} pagination={false} columns={columns}
  />;
};
