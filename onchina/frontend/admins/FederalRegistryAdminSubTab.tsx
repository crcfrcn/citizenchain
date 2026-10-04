// 联邦注册局管理员只读目录。管理员换届必须由治理业务写入 entity 任职结果，
// OnChina 不提供本地替换或直接改管理员集合入口。

import { useMemo, useState } from 'react';
import { Badge, Button, Card, Modal, Space, Table } from 'antd';
import { KeyOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import { useAuth } from '../hooks/useAuth';
import { normalizeScopeProvinceName } from '../hooks/useScope';
import { CID_MODAL_Z_INDEX } from '../core/modalStack';
import type { FederalRegistryAdminRow } from './api';
import { sameHexAccount } from './adminUtils';
import { usePasskeyRegistration } from '../auth/passkey/usePasskey';
import {
  InstitutionAssignmentDetails,
  assignmentDisplayLabel,
  formatAdminBalanceFen,
} from './InstitutionAssignmentCard';

interface FederalRegistryAdminSubTabProps {
  /** 可为 null：tab 外壳先渲染，链上定位/读链完成前列表内部显示加载态，不阻塞 tab 出现。 */
  selectedFederalRegistry: FederalRegistryAdminRow | null;
  federalRegistryAdmins: FederalRegistryAdminRow[];
  federalRegistryAdminsLoading: boolean;
  /** 该机构 cid_short_name 单一字段，如“联邦注册局”。 */
  federalRegistryCidShortName?: string | null;
}

const FEDERAL_REGISTRY_PAGE_SIZE = 20;

export function FederalRegistryAdminSubTab({
  selectedFederalRegistry,
  federalRegistryAdmins,
  federalRegistryAdminsLoading,
  federalRegistryCidShortName,
}: FederalRegistryAdminSubTabProps) {
  const { auth } = useAuth();
  const { registered: passkeyRegistered, busy: passkeyBusy, register: doRegisterPasskey } =
    usePasskeyRegistration();
  const [detailTarget, setDetailTarget] = useState<FederalRegistryAdminRow | null>(null);
  const [federalListPage, setFederalListPage] = useState(1);
  const currentProvinceName =
    normalizeScopeProvinceName(auth?.scope_province_name) || selectedFederalRegistry?.province_name || '';

  const displayedFederalRegistryAdmins = useMemo(
    () => federalRegistryAdmins
      .map((row, index) => ({ row, index }))
      .sort((left, right) => {
        const leftCurrent = normalizeScopeProvinceName(left.row.province_name) === currentProvinceName;
        const rightCurrent = normalizeScopeProvinceName(right.row.province_name) === currentProvinceName;
        if (leftCurrent !== rightCurrent) return leftCurrent ? -1 : 1;
        return left.index - right.index;
      })
      .map((entry) => entry.row),
    [currentProvinceName, federalRegistryAdmins],
  );

  const columns: ColumnsType<FederalRegistryAdminRow> = [
    {
      title: '序号',
      width: 72,
      align: 'center',
      render: (_value, _row, index) =>
        (federalListPage - 1) * FEDERAL_REGISTRY_PAGE_SIZE + index + 1,
    },
    { title: '省份', dataIndex: 'province_name', align: 'center', width: 120 },
    {
      title: '岗位',
      render: (_value, row) => assignmentDisplayLabel(row) || '-',
    },
    {
      title: '余额',
      dataIndex: 'balance_fen',
      width: 160,
      align: 'right',
      render: (_value, row) => formatAdminBalanceFen(row.balance_fen) || '-',
    },
    {
      title: '操作',
      width: 100,
      align: 'center',
      render: (_value, row) => sameHexAccount(row.account_id, auth?.account_id) ? (
        <Badge dot={passkeyRegistered === false} size="small">
          <Button
            size="small"
            icon={<KeyOutlined />}
            loading={passkeyBusy}
            onClick={(event) => {
              event.stopPropagation();
              void doRegisterPasskey();
            }}
          >
            密钥
          </Button>
        </Badge>
      ) : null,
    },
  ];

  return (
    <>
      <Card
        type="inner"
        title={(
          <Space size={6} align="center">
            <span>{federalRegistryCidShortName || '联邦注册局'}</span>
            {currentProvinceName ? <><span>·</span><span>{currentProvinceName}</span></> : null}
          </Space>
        )}
      >
        <Table<FederalRegistryAdminRow>
          rowKey={(row) => `${row.id}-${row.account_id}-${row.role_code}`}
          loading={federalRegistryAdminsLoading}
          dataSource={displayedFederalRegistryAdmins}
          pagination={{
            pageSize: FEDERAL_REGISTRY_PAGE_SIZE,
            current: federalListPage,
            showSizeChanger: false,
            showTotal: (total) => `共 ${total} 条`,
            onChange: (page) => setFederalListPage(page),
          }}
          columns={columns}
          onRow={(row) => ({
            onClick: () => setDetailTarget(row),
            style: { cursor: 'pointer' },
          })}
        />
      </Card>

      <Modal
        title="管理员岗位任职"
        open={!!detailTarget}
        footer={null}
        centered
        onCancel={() => setDetailTarget(null)}
        zIndex={CID_MODAL_Z_INDEX.business}
      >
        {detailTarget ? (
          <InstitutionAssignmentDetails
            assignment={detailTarget}
            areaLabel="省份"
            areaValue={detailTarget.province_name}
          />
        ) : null}
      </Modal>
    </>
  );
}
