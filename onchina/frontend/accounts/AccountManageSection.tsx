// 机构自助账户管理区块。机构自定义账户的新增/删除归机构自己的在册管理员,
// 挂在机构自己的工作区(私权/通用/司法等),注册局详情页只读账户、不在此增删。
//
// 数据源:cid_number/全称用 getOwnInstitution(基于激活节点绑定,不接受前端传 cid_number);
// 账户列表读侧已切链上真源,改调 /accounts(后端链读)。增删都不再本地直写,而是发起本机构
// 内部投票提案:由发起管理员使用签名钱包冷签一笔普通 extrinsic,授权由 runtime 在 origin 处以
// is_institution_admin + 岗位码(proposer_role_code)校验,机构内部投票通过后才生效。
// 协议账户(主/费用/两和基金/安全基金/永久质押/清算)由后端 can_delete=false 标记,
// AccountList 据此自动隐藏删除按钮。

import { useEffect, useState } from 'react';
import { Alert, Button, Card, Input, Space } from 'antd';
import type { AdminAuth } from '../auth/types';
import { getOwnInstitution } from '../admins/api';
import { submitChainSign, useChainSign } from '../core/useChainSign';
import { deleteAccount, listAccounts, type InstitutionAccount } from './api';
import { AccountList } from './AccountList';
import { CreateAccountModal } from './CreateAccountModal';
import { notice } from '../utils/notice';

export type AccountManageSectionProps = {
  auth: AdminAuth;
};

export function AccountManageSection({ auth }: AccountManageSectionProps) {
  const [cidNumber, setCidNumber] = useState('');
  const [cidFullName, setCidFullName] = useState('');
  const [accounts, setAccounts] = useState<InstitutionAccount[]>([]);
  const [loading, setLoading] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  // 删除账户提案的发起岗位码;runtime 据此校验发起提案权限。
  const [proposerRoleCode, setProposerRoleCode] = useState('');
  // refreshKey 自增触发本机构账户重载(增删成功后调用 reload)。
  const [refreshKey, setRefreshKey] = useState(0);
  // 增/删都是发起内部投票提案,复用统一冷签扫码 hook。
  const { signChain, chainSignModal } = useChainSign('账户提案签名');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getOwnInstitution(auth)
      .then(async (detail) => {
        if (cancelled) return;
        const cid = detail.institution.cid_number;
        setCidNumber(cid);
        setCidFullName(detail.institution.cid_full_name ?? '');
        // 账户明细一律从链上真源读取(/accounts 后端已改链读)。
        const list = await listAccounts(auth, cid);
        if (!cancelled) setAccounts(list);
      })
      .catch((err) => {
        if (!cancelled) notice.error(err, '加载本机构账户失败');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [auth.access_token, refreshKey]);

  const reload = () => setRefreshKey((v) => v + 1);

  const onDeleteAccount = async (accountName: string) => {
    const roleCode = proposerRoleCode.trim();
    if (!roleCode) {
      notice.error('请先填写发起岗位码');
      return;
    }
    try {
      const prepared = await deleteAccount(auth, cidNumber, accountName, roleCode);
      const signed = await signChain(prepared.request_id, prepared.sign_request);
      await submitChainSign(auth, prepared.request_id, signed.account_id, signed.signature);
      notice.success(`账户 "${accountName}" 关闭提案已提交,机构内部投票通过后生效`);
      reload();
    } catch (err) {
      notice.error(err, '');
    }
  };

  return (
    <Card
      title={`账户管理(${accounts.length})`}
      extra={
        <Button type="primary" disabled={!cidNumber} onClick={() => setCreateOpen(true)}>
          + 新建账户
        </Button>
      }
    >
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        <Alert
          type="info"
          showIcon
          message="账户增删都是发起本机构内部投票提案,不会立即生效"
          description="管理员公民钱包扫码后只签名一次并显示响应二维码;链上中国回扫响应后统一验签、预检并提交链上。机构内部投票通过后账户才在链上生效。"
        />
        <Input
          placeholder="发起岗位码(删除账户提案需要)"
          value={proposerRoleCode}
          onChange={(e) => setProposerRoleCode(e.target.value)}
          maxLength={64}
          style={{ maxWidth: 320 }}
        />
        <AccountList accounts={accounts} loading={loading} canDelete onDelete={onDeleteAccount} />
      </Space>
      <CreateAccountModal
        auth={auth}
        cidNumber={cidNumber}
        cidFullName={cidFullName}
        existingAccounts={accounts}
        open={createOpen}
        onCancel={() => setCreateOpen(false)}
        onCreated={() => {
          setCreateOpen(false);
          reload();
        }}
      />
      {chainSignModal}
    </Card>
  );
}
