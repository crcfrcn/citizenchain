import { Empty, Alert, Button, Card, Input, Radio, Space, Spin, Typography } from "antd";
import type { AdminAuth } from "@gmb/onchina-platform/authentication/types";
import { OwnInstitutionAdminsView } from "@gmb/onchina-platform/institutions/AdminsView";
import { AccountManageSection } from "@gmb/onchina-platform/accounts/AccountManageSection";
import { OwnInstitutionInfoPanel } from "@gmb/onchina-platform/institutions/InfoPanel";
import { WorkspaceShell } from "@gmb/onchina-platform/workspace/WorkspaceShell";
import { useCallback, useEffect, useState } from "react";
import { glassCardHeadStyle, glassCardStyle } from "@gmb/onchina-platform/controls/cardStyles";
import { submitChainSign, useChainSign } from "@gmb/onchina-platform/qr/useChainSign";
import { notice } from "@gmb/onchina-platform/controls/notice";
import { adminRequest } from "@gmb/onchina-platform/http";
import { passkeySubmitHeaders } from "@gmb/onchina-platform/authentication/security";

// 私权机构本机构工作台。
//
// 本页面只展示当前登录机构自身信息与已授权模块，不提供注册局使用的省市筛选、
// 私权机构目录或其它机构详情入口。所有写权限仍由后端按准确机构 CID 独立校验。









export type PrivateInstitutionWorkspaceProps = {
  auth: AdminAuth;
};

function PrivateOperations({ auth }: PrivateInstitutionWorkspaceProps) {
  // 账户管理归本机构在册管理员;平台会员定价仅在后端开放该模块时叠加显示。
  const showPlatformPrice = auth.workspace?.workspace_modules.includes('platform_membership_price');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <AccountManageSection auth={auth} />
      {showPlatformPrice ? <PlatformPricePanel auth={auth} /> : null}
    </div>
  );
}

function PrivateDisplay({ auth }: PrivateInstitutionWorkspaceProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <OwnInstitutionInfoPanel auth={auth} />
      {auth.capabilities?.canViewOwnAdmins ? (
        <OwnInstitutionAdminsView layout="cards" />
      ) : null}
    </div>
  );
}

function PrivateRecords() {
  return <Empty description="当前机构暂无操作记录" />;
}

export function PrivateInstitutionWorkspace({ auth }: PrivateInstitutionWorkspaceProps) {
  if (!auth.workspace || auth.workspace.workspace_kind !== 'private') return null;
  return (
    <WorkspaceShell
      workspace={auth.workspace}
      operations={<PrivateOperations auth={auth} />}
      display={<PrivateDisplay auth={auth} />}
      records={<PrivateRecords />}
    />
  );
}

// 公民链基金会平台会员价格面板。
//
// 页面只展示 finalized 链上真源并进入统一冷签链路：钱包签名一次并显示响应二维码，
// OnChina 回扫后统一验签、预检和提交，不把表单值当作调价成功结果。










const levelLabels: Record<PlatformMembershipLevel, string> = {
  freedom: '自由会员',
  democracy: '民主会员',
  spark: '薪火会员',
};

function fenText(value: string): string {
  const normalized = value.replace(/^0+(?=\d)/, '') || '0';
  if (!/^\d+$/.test(normalized)) return '—';
  const padded = normalized.padStart(3, '0');
  return `${padded.slice(0, -2)}.${padded.slice(-2)} 元`;
}

export function PlatformPricePanel({ auth }: { auth: AdminAuth }) {
  const [prices, setPrices] = useState<PlatformPrices | null>(null);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [membershipLevel, setMembershipLevel] = useState<PlatformMembershipLevel>('freedom');
  const [proposerRoleCode, setProposerRoleCode] = useState('GENESIS_PRODUCT_MANAGER');
  const [newPriceFen, setNewPriceFen] = useState('');
  const { signChain, chainSignModal } = useChainSign('平台会员调价提案签名');

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setPrices(await getPlatformPrices(auth));
    } catch (error) {
      setPrices(null);
      notice.error(error, '读取链上平台价格失败');
    } finally {
      setLoading(false);
    }
  }, [auth]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const submit = async () => {
    const roleCode = proposerRoleCode.trim();
    if (!roleCode) {
      notice.error('必须填写提案发起岗位码');
      return;
    }
    const value = newPriceFen.trim();
    if (!/^[1-9]\d*$/.test(value)) {
      notice.error('新价格必须是正整数分');
      return;
    }
    setSubmitting(true);
    try {
      const result = await proposePlatformPrice(auth, roleCode, membershipLevel, value);
      const signed = await signChain(result.requestId, result.signRequest);
      const submitted = await submitChainSign(
        auth,
        result.requestId,
        signed.accountId,
        signed.signature,
      );
      notice.success(`调价提案交易已提交：${submitted.txHash}`);
      setNewPriceFen('');
    } catch (error) {
      notice.error(error, '生成平台调价提案失败');
    } finally {
      setSubmitting(false);
    }
  };

  if (loading) return <Spin />;
  if (!prices) {
    return <Alert type="error" showIcon message="无法确认 finalized 平台价格，已拒绝操作" />;
  }

  return (
    <>
      <Card
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
        title="平台会员价格"
        extra={<Button onClick={() => void reload()}>刷新链上价格</Button>}
      >
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="提交后进入本机构内部投票，不会立即修改价格"
          description="公民钱包扫描后只签名一次并显示响应二维码；链上中国回扫响应后统一验签、预检并提交链上。页面只以之后读取到的 finalized 链上价格为准。"
        />
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <div>
            <Typography.Text type="secondary">当前 finalized 价格：</Typography.Text>
            <Space wrap style={{ marginLeft: 12 }}>
              <span>自由会员 {fenText(prices.freedomPriceFen)}</span>
              <span>民主会员 {fenText(prices.democracyPriceFen)}</span>
              <span>薪火会员 {fenText(prices.sparkPriceFen)}</span>
            </Space>
          </div>
          <Radio.Group
            value={membershipLevel}
            onChange={(event) => setMembershipLevel(event.target.value as PlatformMembershipLevel)}
            options={(Object.keys(levelLabels) as PlatformMembershipLevel[]).map((value) => ({
              value,
              label: levelLabels[value],
            }))}
          />
          <Input
            value={proposerRoleCode}
            onChange={(event) => setProposerRoleCode(event.target.value)}
            placeholder="提案发起岗位码"
            maxLength={64}
            style={{ width: 320 }}
          />
          <Space wrap>
            <Input
              value={newPriceFen}
              onChange={(event) => setNewPriceFen(event.target.value.replace(/\D/g, ''))}
              placeholder="输入新价格（整数分）"
              style={{ width: 240 }}
            />
            <Button type="primary" loading={submitting} onClick={() => void submit()}>
              发起调价提案
            </Button>
          </Space>
          <Typography.Text type="secondary" style={{ wordBreak: 'break-all' }}>
            平台机构 CID：{prices.platformCidNumber}
          </Typography.Text>
        </Space>
      </Card>
      {chainSignModal}
    </>
  );
}

// 公民链基金会平台价格 API；通用 Bearer 请求仍复用 platformweb/workspace/http.ts。






export function getPlatformPrices(auth: AdminAuth): Promise<PlatformPrices> {
  return adminRequest<PlatformPrices>('/api/membership/platform-prices', auth);
}

/** 调价提案先完成一次现有Passkey认证，原金额字段与钱包签名流程保持。 */
export async function proposePlatformPrice(
  auth: AdminAuth,
  proposerRoleCode: string,
  membershipLevel: PlatformMembershipLevel,
  newPriceFen: string,
): Promise<ProposePlatformPriceResult> {
  return adminRequest<ProposePlatformPriceResult>(
    '/api/membership/platform-prices/propose',
    auth,
    {
      method: 'POST',
      headers: await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }),
      body: JSON.stringify({
        proposerRoleCode: proposerRoleCode,
        membershipLevel: membershipLevel,
        newPriceFen: newPriceFen,
      }),
    },
  );
}

// 平台会员价格模块 DTO。金额一律使用十进制字符串，避免 JavaScript 数字精度损失。

export type PlatformMembershipLevel = 'freedom' | 'democracy' | 'spark';

export type PlatformPrices = {
  platformCidNumber: string;
  freedomPriceFen: string;
  democracyPriceFen: string;
  sparkPriceFen: string;
  finalizedBlockHash: string;
};

export type ProposePlatformPriceResult = {
  requestId: string;
  signRequest: string;
};
