// 新建账户弹窗。
//
// 新增机构自定义账户 = 发起本机构「新增账户」内部投票提案:
//   - 提交后不立即上链,而是由发起管理员使用签名钱包冷签一笔普通 extrinsic,
//     机构内部投票通过后账户才在链上生效
//   - 授权由 runtime 在 origin 处以 is_institution_admin + 岗位码(proposer_role_code)校验
//   - 链上派生公式由链端按 account_name 路由,前端不做地址预览(避免与链端公式漂移)
//
// 约束:
//   - 同一 cid_number 下 account_name 唯一(后端硬校验,前端做一次即时预校验)
//   - 协议保留名(主/费用/两和基金/安全基金/永久质押/清算)不能作为自定义账户名

import React, { useEffect, useState } from 'react';
import { Button, Form, Input, Modal, Typography } from 'antd';
import type { AdminAuth } from '../auth/types';
import { submitChainSign, useChainSign } from '../core/useChainSign';
import { createAccount, type InstitutionAccount } from './api';
import { notice } from '../utils/notice';

interface Props {
  auth: AdminAuth;
  cidNumber: string;
  cidFullName: string;
  /** 当前已有的账户列表(用于前端唯一性预校验) */
  existingAccounts: InstitutionAccount[];
  open: boolean;
  onCancel: () => void;
  onCreated: () => void;
}

interface FormValues {
  account_name: string;
  proposer_role_code: string;
}

export const CreateAccountModal: React.FC<Props> = ({
  auth,
  cidNumber,
  cidFullName,
  existingAccounts,
  open,
  onCancel,
  onCreated,
}) => {
  const [form] = Form.useForm<FormValues>();
  const { signChain, chainSignModal } = useChainSign('新增账户提案签名');
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (open) {
      form.resetFields();
    }
  }, [open]);

  const onSubmit = async (values: FormValues) => {
    const name = values.account_name.trim();
    const roleCode = values.proposer_role_code.trim();
    if (!name) {
      notice.error('账户名称不能为空');
      return;
    }
    if (name.length > 30) {
      notice.error('账户名称最多 30 字');
      return;
    }
    if (!roleCode) {
      notice.error('请输入发起岗位码');
      return;
    }
    // 前端预校验:同 cid 下不能重名(含协议账户)
    if (existingAccounts.some((a) => a.account_name === name)) {
      notice.error(`账户名称"${name}"在本机构下已存在`);
      return;
    }
    setSubmitting(true);
    try {
      const prepared = await createAccount(auth, cidNumber, name, roleCode);
      const signed = await signChain(prepared.request_id, prepared.sign_request);
      await submitChainSign(auth, prepared.request_id, signed.account_id, signed.signature);
      notice.success('新增账户提案已提交,机构内部投票通过后生效');
      onCreated();
    } catch (err) {
      notice.error(err, '发起新增账户提案失败');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <>
      <Modal
        title={<div style={{ textAlign: 'center', width: '100%' }}>新建账户</div>}
        open={open}
        onCancel={onCancel}
        footer={[
          <Button key="cancel" onClick={onCancel}>
            取消
          </Button>,
          <Button key="submit" type="primary" loading={submitting} onClick={() => form.submit()}>
            {submitting ? '提交中...' : '发起提案'}
          </Button>,
        ]}
        destroyOnClose
      >
        <div style={{ marginBottom: 12 }}>
          <Typography.Text type="secondary">机构:{cidFullName}</Typography.Text>
          <br />
          <Typography.Text type="secondary" code style={{ fontSize: 11 }}>
            {cidNumber}
          </Typography.Text>
        </div>
        <Form form={form} layout="vertical" onFinish={onSubmit}>
          <Form.Item
            label="账户名称"
            name="account_name"
            rules={[
              { required: true, message: '请输入账户名称' },
              { max: 30, message: '最多 30 个字' },
            ]}
            extra={'发起提案,机构内部投票通过后账户才在链上生效。'}
          >
            <Input placeholder="如:办案账户、工资账户、采购账户..." maxLength={30} />
          </Form.Item>
          <Form.Item
            label="发起岗位码"
            name="proposer_role_code"
            rules={[
              { required: true, message: '请输入发起岗位码' },
              { max: 64, message: '最多 64 字节' },
            ]}
            extra={'你在本机构当前任职的岗位码;runtime 据此校验发起提案权限。'}
          >
            <Input placeholder="如:ROWNER、RFINANCE..." maxLength={64} />
          </Form.Item>
        </Form>
      </Modal>
      {chainSignModal}
    </>
  );
};
