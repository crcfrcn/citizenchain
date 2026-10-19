import { useEffect, useState } from 'react';
import { KeyOutlined } from '@ant-design/icons';
import { Badge, Button, Card, Empty, Grid, Modal, Spin, Table, Tooltip } from 'antd';
import { useAuth } from './hooks_useAuth';

import { type OwnInstitutionAdminListOutput, type OwnInstitutionAdminRow, listOwnInstitutionAdmins } from './admins_api';

import { sameHexAccount } from './admins_adminUtils';

import { CID_MODAL_Z_INDEX } from './core_modalStack';
import { notice } from './helpers_notice';

import { usePasskeyRegistration } from './authentication_usePasskey';
import { InstitutionAssignmentCard, InstitutionAssignmentDetails, assignmentDisplayLabel, formatAdminBalanceFen } from './admins_InstitutionAssignmentCard';
// 本机构管理员通用视图；注册局专属页面归注册局模块。

export interface OwnInstitutionAdminsViewProps {
  layout?: 'table' | 'cards';
}

export function OwnInstitutionAdminsView({ layout = 'table' }: OwnInstitutionAdminsViewProps) {
  const { auth } = useAuth();
  const { registered: passkeyRegistered, busy: passkeyBusy, register: doRegisterPasskey } =
    usePasskeyRegistration();
  const [data, setData] = useState<OwnInstitutionAdminListOutput | null>(null);
  const [loading, setLoading] = useState(false);
  const [detailTarget, setDetailTarget] = useState<OwnInstitutionAdminRow | null>(null);
  const screens = Grid.useBreakpoint();
  const isCardLayout = layout === 'cards';

  useEffect(() => {
    if (!auth) {
      setData(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    listOwnInstitutionAdmins(auth)
      .then((result) => {
        if (!cancelled) setData(result);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '加载本机构管理员失败');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth?.access_token]);

  const passkeyAction = (row: OwnInstitutionAdminRow) => {
    const isSelf = row.is_self || sameHexAccount(row.account_id, auth?.account_id);
    if (!isSelf) return null;
    const button = (
      <Button size="small" icon={<KeyOutlined />} loading={passkeyBusy} onClick={doRegisterPasskey}>
        密钥
      </Button>
    );
    return (
      <Badge dot={passkeyRegistered === false} size="small">
        {isCardLayout ? (
          <Tooltip title={passkeyRegistered ? '更新 passkey 密钥' : '设置 passkey 密钥'}>{button}</Tooltip>
        ) : (
          button
        )}
      </Badge>
    );
  };

  const rows = data?.rows ?? [];

  return (
    <Card
      title={`${data?.cid_short_name || '本机构'}管理员列表`}
      bordered={false}
      style={{ background: '#ffffff', borderRadius: 8 }}
    >
      {isCardLayout ? (
        <Spin spinning={loading}>
          {rows.length === 0 && !loading ? (
            <Empty description="暂无管理员" />
          ) : (
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: screens.lg ? 'repeat(2, minmax(0, 1fr))' : 'minmax(0, 1fr)',
                gap: 14,
              }}
            >
              {rows.map((row, index) => (
                <InstitutionAssignmentCard
                  key={row.account_id}
                  assignment={row}
                  index={index + 1}
                  action={passkeyAction(row)}
                  actionPlacement="balance-row"
                />
              ))}
            </div>
          )}
        </Spin>
      ) : (
        <Table<OwnInstitutionAdminRow>
          rowKey={(row) => row.account_id}
          loading={loading}
          dataSource={rows}
          pagination={false}
          columns={[
            {
              title: '序号',
              width: 72,
              align: 'center',
              render: (_value: string, _row, index) => index + 1,
            },
            {
              title: '岗位',
              dataIndex: 'role_name',
              render: (_value: string, row) => assignmentDisplayLabel(row) || '-',
            },
            {
              title: '余额',
              dataIndex: 'balance_fen',
              width: 160,
              align: 'right',
              render: (_value: string, row) => formatAdminBalanceFen(row.balance_fen) || '-',
            },
            {
              title: '操作',
              width: 220,
              align: 'center',
              render: (_v: unknown, row) => (
                <span onClick={(event) => event.stopPropagation()}>{passkeyAction(row)}</span>
              ),
            },
          ]}
          onRow={(row) => ({
            onClick: () => setDetailTarget(row),
            style: { cursor: 'pointer' },
          })}
        />
      )}
      <Modal
        title="管理员完整信息"
        open={!!detailTarget}
        footer={null}
        centered
        onCancel={() => setDetailTarget(null)}
        zIndex={CID_MODAL_Z_INDEX.business}
      >
        {detailTarget ? <InstitutionAssignmentDetails assignment={detailTarget} /> : null}
      </Modal>
    </Card>
  );
}
