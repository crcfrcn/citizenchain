// 注册局占号建档弹窗(占即绑,两次扫码)。
//
// 极简表单:人主体类型(公民 CTZN / 居民 NATP)+ 岗位码。居住省市由办理注册局 scope 派生;
// 姓名/性别/出生/护照等档案占号后为空,后续在详情页「编辑资料」补齐(D4a 档案选填)。
//
// 流程:prepare(发号)→ 公民本人钱包/公民App 扫「占号签名」(占即绑,自填本账户)
//   → submit(组装 occupy_cid)→ 管理员冷签 → chain/submit 进块建匿名记录。

import { useState } from 'react';
import { Alert, Button, Form, Input, Modal, Select } from 'antd';

import type { AdminAuth } from '../auth/types';
import { submitChainSign, useChainSign } from '../core/useChainSign';
import {
  prepareCitizenOccupy,
  submitCitizenOccupy,
  type CreateCitizenInput,
  type CreateCitizenResult,
  type CitizenType,
} from './api';
import { notice } from '../utils/notice';

interface Props {
  auth: AdminAuth | null;
  open: boolean;
  provinceName: string | null;
  cityName: string | null;
  onClose: () => void;
  /** 占号成功后回填新身份 CID 并刷新列表。 */
  onCreated: (cidNumber: string) => Promise<void> | void;
}

interface FormValues {
  cid_type: CitizenType;
  actor_role_code: string;
}

export function CitizenCreateModal({
  auth,
  open,
  provinceName,
  cityName,
  onClose,
  onCreated,
}: Props) {
  const [form] = Form.useForm<FormValues>();
  const [submitting, setSubmitting] = useState(false);
  // 两次扫码:公民本人扫「占号签名」,注册局管理员扫「链上占即绑冷签」。
  const { signChain: signCitizen, chainSignModal: citizenSignModal } = useChainSign(
    '请公民本人用公民钱包 / 公民App 扫码占号',
  );
  const { signChain: signAdmin, chainSignModal: adminSignModal } = useChainSign(
    '注册局管理员冷签占号交易',
  );

  const scopeReady = Boolean(auth && provinceName && cityName);

  const onSubmit = async (values: FormValues) => {
    if (!auth) {
      notice.error('请先登录');
      return;
    }
    if (!scopeReady) {
      notice.error('当前登录缺少办理城市');
      return;
    }
    const payload: CreateCitizenInput = {
      actor_role_code: values.actor_role_code.trim(),
      cid_type: values.cid_type,
    };
    setSubmitting(true);
    try {
      // 段1:发号 + 公民钱包域签名占号 QR(占即绑,b.u 空、钱包自填本账户)。
      const prepared = await prepareCitizenOccupy(auth, payload);
      // 段1→2:公民本人扫码占号,返回其钱包账户与占号签名。
      const citizenSigned = await signCitizen(
        prepared.request_id,
        prepared.citizen_sign_request,
      );
      const admin = await submitCitizenOccupy(
        auth,
        prepared.request_id,
        citizenSigned.account_id,
        citizenSigned.signature,
      );
      // 段2→3:注册局管理员冷签占即绑 extrinsic 并提交进块。
      const adminSigned = await signAdmin(admin.request_id, admin.sign_request);
      const submitted = await submitChainSign<CreateCitizenResult>(
        auth,
        admin.request_id,
        adminSigned.account_id,
        adminSigned.signature,
      );
      const result = submitted.citizen;
      if (!result) {
        throw new Error('占号已上链,但记录落库结果缺失');
      }
      notice.success(`占号上链成功,身份CID：${result.cid_number}`);
      form.resetFields();
      onClose();
      await onCreated(result.cid_number);
    } catch (err) {
      notice.error(err, '占号失败');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <Modal
        title={<div style={{ textAlign: 'center', width: '100%' }}>注册局占号(新增公民 / 居民)</div>}
        open={open}
        onCancel={onClose}
        destroyOnClose
        width={560}
        footer={[
          <Button key="cancel" onClick={onClose}>
            取消
          </Button>,
          <Button
            key="submit"
            type="primary"
            loading={submitting}
            disabled={!scopeReady}
            onClick={() => form.submit()}
          >
            {submitting ? '占号中...' : '发起占号'}
          </Button>,
        ]}
      >
        {!scopeReady && (
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 16 }}
            message="请先选择办理城市后再占号"
          />
        )}
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message="占即绑:先由公民本人钱包扫码占号,再由注册局管理员冷签上链。姓名 / 性别 / 出生等档案占号后可在详情页「编辑资料」补齐。"
        />
        <Form form={form} layout="vertical" onFinish={onSubmit} initialValues={{ cid_type: 'CTZN' }}>
          <Form.Item
            label="人主体类型"
            name="cid_type"
            rules={[{ required: true, message: '请选择人主体类型' }]}
          >
            <Select
              options={[
                { value: 'CTZN', label: '公民(CTZN)' },
                { value: 'NATP', label: '居民(NATP)' },
              ]}
            />
          </Form.Item>
          <Form.Item
            label="注册局岗位码"
            name="actor_role_code"
            rules={[{ required: true, message: '请输入当前任职岗位码' }]}
          >
            <Input placeholder="岗位码" allowClear maxLength={64} />
          </Form.Item>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', columnGap: 16 }}>
            <Form.Item label="居住省">
              <Input readOnly value={provinceName ?? ''} />
            </Form.Item>
            <Form.Item label="居住市">
              <Input readOnly value={cityName ?? ''} />
            </Form.Item>
          </div>
        </Form>
      </Modal>
      {citizenSignModal}
      {adminSignModal}
    </>
  );
}
