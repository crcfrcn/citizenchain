import React, { useEffect, useMemo, useState } from 'react';
import { Button, Card, Form, Input, Select, Space, Table, Typography } from 'antd';
import { CopyOutlined, ReloadOutlined } from '@ant-design/icons';
import type { AdminAuth } from '../auth/types';
import { glassCardHeadStyle, glassCardStyle } from '../core/cardStyles';
import { notice } from '../utils/notice';
import {
  listAddressItems,
  listAddressNames,
  prepareAddressChainCall,
  type AddressChainAction,
  type AddressNameRow,
  type AddressRow,
} from './api';

type Props = {
  auth: AdminAuth;
};

type ScopeForm = {
  province_code: string;
  city_code: string;
  town_code: string;
};

const emptyScope: ScopeForm = {
  province_code: '',
  city_code: '',
  town_code: '',
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
  }, [auth.account_id]);

  const scopeReady = useMemo(
    () => Boolean(scope.province_code && scope.city_code && scope.town_code),
    [scope],
  );

  const loadNames = async (nextScope = scope) => {
    if (!nextScope.province_code || !nextScope.city_code || !nextScope.town_code) {
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
      province_code: row.province_code,
      city_code: row.city_code,
      town_code: row.town_code,
      address_name_code: row.address_name_code,
      address_name: row.address_name,
    });
    try {
      const page = await listAddressItems(auth, {
        province_code: row.province_code,
        city_code: row.city_code,
        town_code: row.town_code,
        address_name_code: row.address_name_code,
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
      province_code: values.province_code.trim(),
      city_code: values.city_code.trim(),
      town_code: values.town_code.trim(),
    };
    setScope(next);
    await loadNames(next);
  };

  const onPrepareChainCall = async () => {
    const values = await chainForm.validateFields();
    try {
      const output = await prepareAddressChainCall(auth, {
        actor_role_code: values.actor_role_code,
        action: values.action as AddressChainAction,
        catalog_version: values.catalog_version,
        catalog_hash: values.catalog_hash,
        province_code: values.province_code,
        city_code: values.city_code,
        town_code: values.town_code,
        address_name_code: values.address_name_code,
        address_name: values.address_name,
        address_local_no: values.address_local_no,
        address_detail: values.address_detail,
      });
      setCallData(output.call_data_hex);
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
        <Form.Item name="province_code" rules={[{ required: true, message: '请输入省码' }]}>
          <Input placeholder="省码" style={{ width: 120 }} />
        </Form.Item>
        <Form.Item name="city_code" rules={[{ required: true, message: '请输入市码' }]}>
          <Input placeholder="市码" style={{ width: 120 }} />
        </Form.Item>
        <Form.Item name="town_code" rules={[{ required: true, message: '请输入镇码' }]}>
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
          rowKey={(row) => row.address_name_code}
          size="small"
          loading={loadingNames}
          dataSource={names}
          pagination={false}
          onRow={(record) => ({ onClick: () => loadItems(record) })}
          columns={[
            { title: '编号', dataIndex: 'address_name_code', width: 72 },
            { title: '名称', dataIndex: 'address_name' },
            { title: '地址数', dataIndex: 'address_count', width: 76 },
          ]}
        />
        <Table<AddressRow>
          rowKey={(row) => `${row.address_name_code}-${row.address_local_no}-${row.address_detail}`}
          size="small"
          loading={loadingItems}
          dataSource={items}
          pagination={false}
          columns={[
            { title: '名称编号', dataIndex: 'address_name_code', width: 92 },
            { title: '名称', dataIndex: 'address_name', width: 160 },
            { title: '地址号', dataIndex: 'address_local_no', width: 96 },
            { title: '详细地址', dataIndex: 'address_detail' },
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
              name="actor_role_code"
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
            <Form.Item name="province_code" label="省码">
              <Input placeholder={scope.province_code || '省码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="city_code" label="市码">
              <Input placeholder={scope.city_code || '市码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="town_code" label="镇码">
              <Input placeholder={scope.town_code || '镇码'} disabled={!scopeReady} />
            </Form.Item>
            <Form.Item name="address_name_code" label="地址名称编号">
              <Input placeholder={selectedName?.address_name_code || '001'} />
            </Form.Item>
            <Form.Item name="address_name" label="地址名称">
              <Input placeholder={selectedName?.address_name || '国酒路'} />
            </Form.Item>
            <Form.Item name="address_local_no" label="地址号">
              <Input placeholder="0001" />
            </Form.Item>
            <Form.Item name="address_detail" label="详细地址">
              <Input placeholder="可为空" />
            </Form.Item>
            <Form.Item name="catalog_version" label="地址库版本">
              <Input placeholder="v1.0.0" />
            </Form.Item>
            <Form.Item name="catalog_hash" label="地址库哈希">
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
