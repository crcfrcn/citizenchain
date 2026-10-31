import scanIcon from '../../../icons/scan.svg?url';
import { useEffect } from 'react';
import { Button, Form, Input, Modal, Select } from 'antd';
import { decodeSs58 } from '@gmb/onchina-platform/codes/ss58';
import { ScanAccountModal } from '@gmb/onchina-platform/qr/ScanAccountModal';
import { CID_MODAL_Z_INDEX } from '@gmb/onchina-platform/controls/modalStack';
import { MAX_CITY_REGISTRY_ADMINS_PER_CITY, type RegistryAdminsSharedState } from './cityRegistryAdminsApi';

// 新增市注册局管理员 Modal
// 当 selectedCity 有值时，城市字段预填并锁定（已在某市详情页内新增）

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
  const selectedCityRegistryCity = Form.useWatch('cityScopeCityName', addCityRegistryForm);
  // 新增弹窗只做提前拦截,单市 30 人上限最终以后端校验为准。
  const cityCityRegistryCount = (cityName: string) => cityRegistryAdmins.filter((item) => item.cityName === cityName).length;
  const selectedCityLimitReached = selectedCityRegistryCity
    ? cityCityRegistryCount(selectedCityRegistryCity) >= MAX_CITY_REGISTRY_ADMINS_PER_CITY
    : false;

  // selectedCity 有值时预填城市字段
  useEffect(() => {
    if (addCityRegistryOpen && selectedCity) {
      addCityRegistryForm.setFieldsValue({ cityScopeCityName: selectedCity });
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
          onFinish={(values: { familyName: string; givenName: string; cityRegistryAccount: string; cityScopeCityName: string }) =>
            onCreateCityRegistry({
              familyName: values.familyName,
              givenName: values.givenName,
              cityRegistryAccount: values.cityRegistryAccount,
              cityName: values.cityScopeCityName,
            })
          }
        >
          <Form.Item
            label="姓"
            name="familyName"
            rules={[{ required: true, message: '请输入市注册局管理员姓' }]}
          >
            <Input placeholder="请输入市注册局管理员姓" />
          </Form.Item>
          <Form.Item
            label="名"
            name="givenName"
            rules={[{ required: true, message: '请输入市注册局管理员名' }]}
          >
            <Input placeholder="请输入市注册局管理员名" />
          </Form.Item>
          <Form.Item
            label="市"
            name="cityScopeCityName"
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
                .filter((c) => c.cityCode !== '000')
                .map((c) => {
                  const count = cityCityRegistryCount(c.cityName);
                  return {
                    label: `${c.cityName} (${c.cityCode}) ${count}/${MAX_CITY_REGISTRY_ADMINS_PER_CITY}`,
                    value: c.cityName,
                    disabled: count >= MAX_CITY_REGISTRY_ADMINS_PER_CITY,
                  };
                })}
            />
          </Form.Item>
          <Form.Item
            label="账户"
            name="cityRegistryAccount"
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
                  <span aria-hidden="true" style={{display:"inline-block",width:18,height:18,backgroundColor:"currentColor",maskImage:`url(${scanIcon})`,WebkitMaskImage:`url(${scanIcon})`,maskSize:"contain",WebkitMaskSize:"contain",maskRepeat:"no-repeat",WebkitMaskRepeat:"no-repeat"}} />
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
            addCityRegistryForm.setFieldsValue({ cityRegistryAccount: addr });
          }
          setAccountScanTarget(null);
        }}
      />
    </>
  );
}
