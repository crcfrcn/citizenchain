import React, { useEffect, useMemo, useState } from 'react';
import { Button, Card, Form, Input, Select, Space, Table, Typography } from 'antd';
import { CopyOutlined, ReloadOutlined } from '@ant-design/icons';
import type { AdminAuth } from '../../platform/frontend/authentication_types';
import { glassCardHeadStyle, glassCardStyle } from '../../platform/frontend/core_cardStyles';
import { notice } from '../../platform/frontend/helpers_notice';
import {
  listAddressItems,
  listAddressNames,
  prepareAddressChainCall,
  type AddressChainAction,
  type AddressNameRow,
  type AddressRow,
} from './address_api';

type Props = {
  auth: AdminAuth;
};

type ScopeForm = {
  provinceCode: string;
  cityCode: string;
  townCode: string;
};

const emptyScope: ScopeForm = {
  provinceCode: '',
  cityCode: '',
  townCode: '',
};

export const AddressManageView: React.FC<Props> = ({ auth }) => {
  const [scopeForm] = Form.useForm<ScopeForm>();
  const [chainForm] = Form.useForm();
  const [scope, setScope] = useState<ScopeForm>(emptyScope);
  const [names, setNames] = useState<AddressNameRow[]>([]);
  const [items, setItems] = useState<AddressRow[]>([]);
  const [selectedName, setSelectedName] = useState<AddressNameRow | null>(null);
  const [loadingNames, setLoadingNames] = useState(false);
  const [loadingItems, setLoadingItems] = useState(false);
  const [callData, setCallData] = useState('');

  useEffect(() => {
    scopeForm.setFieldsValue(emptyScope);
    chainForm.resetFields();
    setScope(emptyScope);
    setNames([]);
    setItems([]);
    setSelectedName(null);
    setCallData('');
  }, [auth.accountId]);

  const scopeReady = useMemo(
    () => Boolean(scope.provinceCode && scope.cityCode && scope.townCode),
    [scope],
  );

  const loadNames = async (nextScope = scope) => {
    if (!nextScope.provinceCode || !nextScope.cityCode || !nextScope.townCode) {
      notice.warning('请先填写省码、市码、镇码');
      return;
    }
    setLoadingNames(true);
    setItems([]);
    setSelectedName(null);
    try {
      const page = await listAddressNames(auth, nextScope);
      setNames(page.items);
      chainForm.setFieldsValue(nextScope);
    } catch (err) {
      notice.error(err, '地址名称查询失败');
    } finally {
      setLoadingNames(false);
    }
  };

  const loadItems = async (row: AddressNameRow) => {
    setSelectedName(row);
    setLoadingItems(true);
    chainForm.setFieldsValue({
      provinceCode: row.provinceCode,
      cityCode: row.cityCode,
      townCode: row.townCode,
      addressNameCode: row.addressNameCode,
      addressName: row.addressName,
    });
    try {
      const page = await listAddressItems(auth, {
        provinceCode: row.provinceCode,
        cityCode: row.cityCode,
        townCode: row.townCode,
        addressNameCode: row.addressNameCode,
      });
      setItems(page.items);
    } catch (err) {
      notice.error(err, '完整地址查询失败');
    } finally {
      setLoadingItems(false);
    }
  };

  const onScopeFinish = async (values: ScopeForm) => {
    const next = {
      provinceCode: values.provinceCode.trim(),
      cityCode: values.cityCode.trim(),
      townCode: values.townCode.trim(),
    };
    setScope(next);
    await loadNames(next);
  };

  const onPrepareChainCall = async () => {
    const values = await chainForm.validateFields();
    try {
      const output = await prepareAddressChainCall(auth, {
        actorRoleCode: values.actorRoleCode,
        action: values.action as AddressChainAction,
        catalogVersion: values.catalogVersion,
        catalogHash: values.catalogHash,
        provinceCode: values.provinceCode,
        cityCode: values.cityCode,
        townCode: values.townCode,
        addressNameCode: values.addressNameCode,
        addressName: values.addressName,
        addressLocalNo: values.addressLocalNo,
        addressDetail: values.addressDetail,
      });
      setCallData(output.callDataHex);
      notice.success('链上调用已生成');
    } catch (err) {
      notice.error(err, '生成链上调用失败');
    }
  };

  return (
    <Card title="地址库" bordered={false} style={glassCardStyle} headStyle={glassCardHeadStyle}>
      <Form
        form={scopeForm}
        layout="inline"
        onFinish={onScopeFinish}
        style={{ marginBottom: 16, rowGap: 12 }}
      >
        <Form.Item name="provinceCode" rules={[{ required: true, message: '请输入省码' }]}>
          <Input placeholder="省码" style={{ width: 120 }} />
        </Form.Item>
        <Form.Item name="cityCode" rules={[{ required: true, message: '请输入市码' }]}>
          <Input placeholder="市码" style={{ width: 120 }} />
        </Form.Item>
        <Form.Item name="townCode" rules={[{ required: true, message: '请输入镇码' }]}>
          <Input placeholder="镇码" style={{ width: 120 }} />
        </Form.Item>
        <Form.Item>
          <Button type="primary" htmlType="submit" icon={<ReloadOutlined />}>
            查询
          </Button>
        </Form.Item>
      </Form>

      <div style={{ display: 'grid', gridTemplateColumns: '360px minmax(0, 1fr)', gap: 16 }}>
        <Table<AddressNameRow>
          rowKey={(row) => row.addressNameCode}
          size="small"
          loading={loadingNames}
          dataSource={names}
          pagination={false}
          onRow={(record) => ({ onClick: () => loadItems(record) })}
          columns={[
            { title: '编号', dataIndex: 'addressNameCode', width: 72 },
            { title: '名称', dataIndex: 'addressName' },
            { title: '地址数', dataIndex: 'addressCount', width: 76 },
          ]}
        />
        <Table<AddressRow>
          rowKey={(row) => `${row.addressNameCode}-${row.addressLocalNo}-${row.addressDetail}`}
          size="small"
          loading={loadingItems}
          dataSource={items}
          pagination={false}
          columns={[
            { title: '名称编号', dataIndex: 'addressNameCode', width: 92 },
            { title: '名称', dataIndex: 'addressName', width: 160 },
            { title: '地址号', dataIndex: 'addressLocalNo', width: 96 },
            { title: '详细地址', dataIndex: 'addressDetail' },
          ]}
        />
      </div>

      <div style={{ marginTop: 18, borderTop: '1px solid #e5e7eb', paddingTop: 16 }}>
        <Typography.Title level={5} style={{ marginTop: 0 }}>
          链上地址变更
        </Typography.Title>
        <Form
          form={chainForm}
          layout="vertical"
          initialValues={{ action: 'set_address' }}
        >
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, minmax(150px, 1fr))', gap: 12 }}>
            <Form.Item
              name="actorRoleCode"
              label="任职岗位码"
              rules={[{ required: true, message: '请输入当前任职岗位码' }]}
            >
              <Input placeholder="岗位码" maxLength={64} />
            </Form.Item>
            <Form.Item name="action" label="动作" rules={[{ required: true }]}>
              <Select
                options={[
                  { label: '设置地址库版本', value: 'set_catalog_version' },
                  { label: '设置地址名称', value: 'set_address_name' },
                  { label: '删除地址名称', value: 'remove_address_name' },
                  { label: '设置完整地址', value: 'set_address' },
                  { label: '删除完整地址', value: 'remove_address' },
                ]}
              />
            </Form.Item>
            <Form.Item name="provinceCode" label="省码">
              <Input placeholder={scope.provinceCode || '省码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="cityCode" label="市码">
              <Input placeholder={scope.cityCode || '市码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="townCode" label="镇码">
              <Input placeholder={scope.townCode || '镇码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="addressNameCode" label="地址名称编号">
              <Input placeholder={selectedName?.addressNameCode || '001'} />
            </Form.Item>
            <Form.Item name="addressName" label="地址名称">
              <Input placeholder={selectedName?.addressName || '国酒路'} />
            </Form.Item>
            <Form.Item name="addressLocalNo" label="地址号">
              <Input placeholder="0001" />
            </Form.Item>
            <Form.Item name="addressDetail" label="详细地址">
              <Input placeholder="可为空" />
            </Form.Item>
            <Form.Item name="catalogVersion" label="地址库版本">
              <Input placeholder="v1.0.0" />
            </Form.Item>
            <Form.Item name="catalogHash" label="地址库哈希">
              <Input placeholder="默认使用当前 china.sqlite 哈希" />
            </Form.Item>
          </div>
          <Space>
            <Button type="primary" onClick={onPrepareChainCall}>
              生成调用
            </Button>
            <Button
              icon={<CopyOutlined />}
              disabled={!callData}
              onClick={() => {
                navigator.clipboard.writeText(callData);
                notice.success('已复制');
              }}
            >
              复制
            </Button>
          </Space>
          {callData && (
            <Input.TextArea
              value={callData}
              readOnly
              rows={4}
              style={{ marginTop: 12, fontFamily: 'monospace' }}
            />
          )}
        </Form>
      </div>
    </Card>
  );
};
