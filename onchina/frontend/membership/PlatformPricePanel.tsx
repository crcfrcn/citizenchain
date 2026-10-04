// 公民链基金会平台会员价格面板。
//
// 页面只展示 finalized 链上真源并进入统一冷签链路：钱包签名一次并显示响应二维码，
// OnChina 回扫后统一验签、预检和提交，不把表单值当作调价成功结果。

import { useCallback, useEffect, useState } from 'react';
import { Alert, Button, Card, Input, Radio, Space, Spin, Typography } from 'antd';
import type { AdminAuth } from '../auth/types';
import { glassCardHeadStyle, glassCardStyle } from '../core/cardStyles';
import { submitChainSign, useChainSign } from '../core/useChainSign';
import { notice } from '../utils/notice';
import { getPlatformPrices, proposePlatformPrice } from './api';
import type { PlatformMembershipLevel, PlatformPrices } from './types';

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
      const signed = await signChain(result.request_id, result.sign_request);
      const submitted = await submitChainSign(
        auth,
        result.request_id,
        signed.account_id,
        signed.signature,
      );
      notice.success(`调价提案交易已提交：${submitted.tx_hash}`);
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
              <span>自由会员 {fenText(prices.freedom_price_fen)}</span>
              <span>民主会员 {fenText(prices.democracy_price_fen)}</span>
              <span>薪火会员 {fenText(prices.spark_price_fen)}</span>
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
            平台机构 CID：{prices.platform_cid_number}
          </Typography.Text>
        </Space>
      </Card>
      {chainSignModal}
    </>
  );
}
