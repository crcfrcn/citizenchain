// 新增市注册局管理员 Modal
// 当 selectedCity 有值时，城市字段预填并锁定（已在某市详情页内新增）

import { useEffect } from 'react';
import { Button, Form, Input, Modal, Select } from 'antd';
import { decodeSs58 } from '../utils/ss58';
import { ScanAccountModal } from '../core/ScanAccountModal';
import { CID_MODAL_Z_INDEX } from '../core/modalStack';
import { MAX_CITY_REGISTRY_ADMINS_PER_CITY, type RegistryAdminsSharedState } from './adminUtils';

interface AddCityRegistryAdminModalProps {
  state: RegistryAdminsSharedState;
}

export function AddCityRegistryAdminModal({ state }: AddCityRegistryAdminModalProps) {
  const {
    addCityRegistryOpen,
    setAddCityRegistryOpen,
    addCityRegistryLoading,
    addCityRegistryForm,
    cityRegistryAdmins,
    cityRegistryAdminCities,
    cityRegistryAdminCitiesLoading,
    selectedCity,
    onCreateCityRegistry,
    accountScanTarget,
    setAccountScanTarget,
  } = state;
  const selectedCityRegistryCity = Form.useWatch('city_scope_city_name', addCityRegistryForm);
  // 新增弹窗只做提前拦截,单市 30 人上限最终以后端校验为准。
  const cityCityRegistryCount = (city_name: string) => cityRegistryAdmins.filter((item) => item.city_name === city_name).length;
  const selectedCityLimitReached = selectedCityRegistryCity
    ? cityCityRegistryCount(selectedCityRegistryCity) >= MAX_CITY_REGISTRY_ADMINS_PER_CITY
    : false;

  // selectedCity 有值时预填城市字段
  useEffect(() => {
    if (addCityRegistryOpen && selectedCity) {
      addCityRegistryForm.setFieldsValue({ city_scope_city_name: selectedCity });
    }
  }, [addCityRegistryOpen, selectedCity, addCityRegistryForm]);

  return (
    <>
      <Modal
        title={<div style={{ textAlign: 'center', width: '100%' }}>新增市注册局管理员</div>}
        open={addCityRegistryOpen}
        onCancel={() => {
          if (addCityRegistryLoading) return;
          addCityRegistryForm.resetFields();
          setAddCityRegistryOpen(false);
        }}
        footer={[
          <Button
            key="cancel"
            disabled={addCityRegistryLoading}
            onClick={() => {
              addCityRegistryForm.resetFields();
              setAddCityRegistryOpen(false);
            }}
          >
            取消新增
          </Button>,
          <Button
            key="submit"
            type="primary"
            loading={addCityRegistryLoading}
            disabled={selectedCityLimitReached}
            title={selectedCityLimitReached ? `本市市注册局管理员已满 ${MAX_CITY_REGISTRY_ADMINS_PER_CITY} 人` : undefined}
            onClick={() => addCityRegistryForm.submit()}
          >
            确认新增
          </Button>,
        ]}
        destroyOnClose
        closable={!addCityRegistryLoading}
        maskClosable={!addCityRegistryLoading}
        zIndex={CID_MODAL_Z_INDEX.business}
      >
        <Form
          form={addCityRegistryForm}
          layout="vertical"
          onFinish={(values: { family_name: string; given_name: string; city_registry_account: string; city_scope_city_name: string }) =>
            onCreateCityRegistry({
              family_name: values.family_name,
              given_name: values.given_name,
              city_registry_account: values.city_registry_account,
              city_name: values.city_scope_city_name,
            })
          }
        >
          <Form.Item
            label="姓"
            name="family_name"
            rules={[{ required: true, message: '请输入市注册局管理员姓' }]}
          >
            <Input placeholder="请输入市注册局管理员姓" />
          </Form.Item>
          <Form.Item
            label="名"
            name="given_name"
            rules={[{ required: true, message: '请输入市注册局管理员名' }]}
          >
            <Input placeholder="请输入市注册局管理员名" />
          </Form.Item>
          <Form.Item
            label="市"
            name="city_scope_city_name"
            rules={[
              { required: true, message: '请选择市' },
              {
                validator: async (_rule, value) => {
                  if (!value) return;
                  if (cityCityRegistryCount(String(value)) >= MAX_CITY_REGISTRY_ADMINS_PER_CITY) {
                    throw new Error(`本市市注册局管理员已满 ${MAX_CITY_REGISTRY_ADMINS_PER_CITY} 人`);
                  }
                },
              },
            ]}
          >
            <Select
              placeholder="请选择市"
              loading={cityRegistryAdminCitiesLoading}
              disabled={selectedCity !== null}
              options={cityRegistryAdminCities
                .filter((c) => c.city_code !== '000')
                .map((c) => {
                  const count = cityCityRegistryCount(c.city_name);
                  return {
                    label: `${c.city_name} (${c.city_code}) ${count}/${MAX_CITY_REGISTRY_ADMINS_PER_CITY}`,
                    value: c.city_name,
                    disabled: count >= MAX_CITY_REGISTRY_ADMINS_PER_CITY,
                  };
                })}
            />
          </Form.Item>
          <Form.Item
            label="账户"
            name="city_registry_account"
            rules={[
              { required: true, message: '请输入市注册局管理员账户' },
              {
                validator: async (_rule, value) => {
                  if (!value) return;
                  try {
                    decodeSs58(String(value));
                  } catch (err) {
                    throw new Error(err instanceof Error ? err.message : '账户格式无效');
                  }
                },
              },
            ]}
          >
            <Input
              placeholder="请输入市注册局管理员账户(SS58)"
              suffix={
                <span
                  title="扫码识别用户码"
                  style={{ cursor: 'pointer', display: 'inline-flex', color: '#0d9488' }}
                  onClick={() => setAccountScanTarget('city_registry')}
                >
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M3 7V5a2 2 0 0 1 2-2h2" />
                    <path d="M17 3h2a2 2 0 0 1 2 2v2" />
                    <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
                    <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
                    <rect x="7" y="7" width="10" height="10" rx="1" />
                  </svg>
                </span>
              }
            />
          </Form.Item>
        </Form>
      </Modal>

      <ScanAccountModal
        open={accountScanTarget !== null}
        onClose={() => setAccountScanTarget(null)}
        onResolved={(addr) => {
          if (accountScanTarget === 'city_registry') {
            addCityRegistryForm.setFieldsValue({ city_registry_account: addr });
          }
          setAccountScanTarget(null);
        }}
      />
    </>
  );
}
