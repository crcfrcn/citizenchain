// 编辑/补齐公民档案弹窗。
//
// 可变:姓、名、居住市、居住镇(人会改名、会搬家)、选举资格。
// 不可变(现实不可变):性别、出生日期、出生地、护照号——本地现存非空即只读锁定,
//   现存为空(如创世公民程伟)时才放开录入用于初始化。护照号与有效期由服务端在出生
//   日期就绪时确定性签发,本弹窗不展示为可填。
// 固定:居住省(= CID 省 = 分区键)只读;跨省居住迁移属"跨地区",后续单独处理。

import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, DatePicker, Form, Input, Modal, Select, Switch } from 'antd';
import dayjs, { type Dayjs } from 'dayjs';

import type { AdminAuth } from '../auth/types';
import { editCitizen, type CitizenRow, type CitizenSex, type EditCitizenInput } from './api';
import {
  getCidMeta,
  listCidCities,
  listCidTowns,
  type CidCityItem,
  type CidProvinceItem,
  type CidTownItem,
} from '../china/api';
import { notice } from '../utils/notice';

interface Props {
  auth: AdminAuth | null;
  open: boolean;
  citizen: CitizenRow;
  onClose: () => void;
  /** 保存成功后回传最新档案行,用于刷新详情与列表。 */
  onSaved: (next: CitizenRow) => void;
}

const DATE_FORMAT = 'YYYY-MM-DD';

interface FormValues {
  family_name: string;
  given_name: string;
  citizen_sex: CitizenSex | '';
  citizen_birth_date: Dayjs | null;
  birth_province_code: string;
  birth_city_code: string;
  birth_town_code: string;
  city_code: string;
  town_code: string;
  voting_eligible: boolean;
}

function isSet(value?: string | null): boolean {
  return typeof value === 'string' && value.trim() !== '';
}

function ageAtToday(birth?: Dayjs | null): number | null {
  if (!birth) return null;
  const today = new Date();
  let age = today.getFullYear() - birth.year();
  const month = today.getMonth() + 1;
  const day = today.getDate();
  if (month < birth.month() + 1 || (month === birth.month() + 1 && day < birth.date())) {
    age -= 1;
  }
  return age;
}

export function EditCitizenModal({ auth, open, citizen, onClose, onSaved }: Props) {
  const [form] = Form.useForm<FormValues>();
  const [submitting, setSubmitting] = useState(false);
  const [birthProvinces, setBirthProvinces] = useState<CidProvinceItem[]>([]);
  const [birthCities, setBirthCities] = useState<CidCityItem[]>([]);
  const [birthTowns, setBirthTowns] = useState<CidTownItem[]>([]);
  const [residenceCities, setResidenceCities] = useState<CidCityItem[]>([]);
  const [residenceTowns, setResidenceTowns] = useState<CidTownItem[]>([]);

  // 锁定判定:不可变字段现存非空即锁定,只有创世等未初始化档案放开对应字段。
  const sexLocked = isSet(citizen.citizen_sex);
  const birthDateLocked = isSet(citizen.citizen_birth_date);
  const birthPlaceLocked =
    isSet(citizen.birth_province_code) &&
    isSet(citizen.birth_city_code) &&
    isSet(citizen.birth_town_code);

  const birthProvinceCode = Form.useWatch('birth_province_code', form);
  const birthCityCode = Form.useWatch('birth_city_code', form);
  const residenceCityCode = Form.useWatch('city_code', form);
  const birthDate = Form.useWatch('citizen_birth_date', form);
  const age = useMemo(() => ageAtToday(birthDate), [birthDate]);

  // 打开时回填现有档案,加载居住省下的市列表 + 现居市的镇列表;出生地未锁定才拉省份。
  useEffect(() => {
    if (!open || !auth) return;
    form.setFieldsValue({
      family_name: citizen.family_name ?? '',
      given_name: citizen.given_name ?? '',
      citizen_sex: (citizen.citizen_sex as CitizenSex) ?? '',
      citizen_birth_date: isSet(citizen.citizen_birth_date)
        ? dayjs(citizen.citizen_birth_date, DATE_FORMAT)
        : null,
      birth_province_code: citizen.birth_province_code ?? '',
      birth_city_code: citizen.birth_city_code ?? '',
      birth_town_code: citizen.birth_town_code ?? '',
      city_code: citizen.city_code ?? '',
      town_code: citizen.town_code ?? '',
      voting_eligible: Boolean(citizen.voting_eligible),
    });
    setBirthCities([]);
    setBirthTowns([]);
    setResidenceTowns([]);
    // 居住市列表:固定居住省下全部市。
    if (citizen.province_name) {
      listCidCities(auth, citizen.province_name)
        .then(setResidenceCities)
        .catch((err) => notice.error(err, '居住市加载失败'));
      // 现居市的镇列表(初始化,不清空已选镇)。
      if (isSet(citizen.city_code)) {
        listCidTowns(auth, citizen.province_name, citizen.city_code)
          .then(setResidenceTowns)
          .catch((err) => notice.error(err, '居住镇加载失败'));
      }
    }
    if (!birthPlaceLocked) {
      getCidMeta(auth)
        .then((meta) =>
          setBirthProvinces(meta.all_provinces?.length ? meta.all_provinces : meta.provinces),
        )
        .catch((err) => notice.error(err, '行政区加载失败'));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, auth, citizen]);

  // 居住市变更(用户改动,非初始值)→ 重新加载居住镇并清空已选镇。
  useEffect(() => {
    if (!auth || !citizen.province_name || !residenceCityCode) return;
    if (residenceCityCode === citizen.city_code) return; // 初始值由打开时的 effect 处理
    listCidTowns(auth, citizen.province_name, residenceCityCode)
      .then((rows) => {
        setResidenceTowns(rows);
        form.setFieldsValue({ town_code: '' });
      })
      .catch((err) => notice.error(err, '居住镇加载失败'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth, residenceCityCode]);

  // 出生省变更 → 加载出生市(仅出生地未锁定时生效)。
  useEffect(() => {
    if (!auth || birthPlaceLocked || !birthProvinceCode) return;
    const province = birthProvinces.find((p) => p.province_code === birthProvinceCode);
    if (!province) return;
    listCidCities(auth, province.province_name)
      .then((rows) => {
        setBirthCities(rows);
        setBirthTowns([]);
        form.setFieldsValue({ birth_city_code: '', birth_town_code: '' });
      })
      .catch((err) => notice.error(err, '出生城市加载失败'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth, birthProvinceCode, birthProvinces, birthPlaceLocked]);

  // 出生市变更 → 加载出生镇。
  useEffect(() => {
    if (!auth || birthPlaceLocked || !birthProvinceCode || !birthCityCode) return;
    const province = birthProvinces.find((p) => p.province_code === birthProvinceCode);
    if (!province) return;
    listCidTowns(auth, province.province_name, birthCityCode)
      .then((rows) => {
        setBirthTowns(rows);
        form.setFieldsValue({ birth_town_code: '' });
      })
      .catch((err) => notice.error(err, '出生镇加载失败'));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [auth, birthProvinceCode, birthCityCode, birthProvinces, birthPlaceLocked]);

  // 未满 16 周岁强制清除选举资格。
  useEffect(() => {
    if (age !== null && age < 16) {
      form.setFieldsValue({ voting_eligible: false });
    }
  }, [age, form]);

  const onSubmit = async (values: FormValues) => {
    if (!auth) {
      notice.error('请先登录');
      return;
    }
    const payload: EditCitizenInput = {
      family_name: values.family_name?.trim() ?? '',
      given_name: values.given_name?.trim() ?? '',
      citizen_sex: values.citizen_sex ?? '',
      citizen_birth_date: values.citizen_birth_date
        ? values.citizen_birth_date.format(DATE_FORMAT)
        : '',
      birth_province_code: values.birth_province_code ?? '',
      birth_city_code: values.birth_city_code ?? '',
      birth_town_code: values.birth_town_code ?? '',
      city_code: values.city_code ?? '',
      town_code: values.town_code ?? '',
      voting_eligible: Boolean(values.voting_eligible),
    };
    setSubmitting(true);
    try {
      const updated = await editCitizen(auth, citizen.cid_number, payload);
      notice.success('公民资料已保存');
      onClose();
      onSaved(updated);
    } catch (err) {
      notice.error(err, '公民资料保存失败');
    } finally {
      setSubmitting(false);
    }
  };

  const birthPlaceText = [citizen.birth_province_name, citizen.birth_city_name, citizen.birth_town_name]
    .filter((v) => v?.trim())
    .join(' / ');

  return (
    <Modal
      title={<div style={{ textAlign: 'center', width: '100%' }}>编辑公民资料</div>}
      open={open}
      onCancel={onClose}
      destroyOnClose
      width={680}
      footer={[
        <Button key="cancel" onClick={onClose}>
          取消
        </Button>,
        <Button key="submit" type="primary" loading={submitting} onClick={() => form.submit()}>
          {submitting ? '保存中...' : '保存'}
        </Button>,
      ]}
    >
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="性别、出生日期、出生地一经保存即锁定,不可再改;护照号由系统在出生日期就绪时自动签发。姓名与居住市镇可修改。"
      />
      <Form form={form} layout="vertical" onFinish={onSubmit}>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', columnGap: 16 }}>
          <Form.Item label="姓" name="family_name" rules={[{ required: true, message: '请输入姓' }]}>
            <Input placeholder="姓" allowClear />
          </Form.Item>
          <Form.Item label="名" name="given_name" rules={[{ required: true, message: '请输入名' }]}>
            <Input placeholder="名" allowClear />
          </Form.Item>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', columnGap: 16 }}>
          <Form.Item label="性别" name="citizen_sex" rules={[{ required: true, message: '请选择性别' }]}>
            <Select
              placeholder="性别"
              disabled={sexLocked}
              options={[
                { value: 'MALE', label: '男' },
                { value: 'FEMALE', label: '女' },
              ]}
            />
          </Form.Item>
          <Form.Item
            label="出生日期"
            name="citizen_birth_date"
            rules={[{ required: true, message: '请选择出生日期' }]}
          >
            <DatePicker style={{ width: '100%' }} format={DATE_FORMAT} disabled={birthDateLocked} />
          </Form.Item>
        </div>

        {birthPlaceLocked ? (
          <Form.Item label="出生地">
            <Input readOnly value={birthPlaceText || '-'} />
          </Form.Item>
        ) : (
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', columnGap: 16 }}>
            <Form.Item
              label="出生省"
              name="birth_province_code"
              rules={[{ required: true, message: '请选择出生省' }]}
            >
              <Select
                placeholder="出生省"
                showSearch
                optionFilterProp="label"
                options={birthProvinces.map((p) => ({ value: p.province_code, label: p.province_name }))}
              />
            </Form.Item>
            <Form.Item
              label="出生市"
              name="birth_city_code"
              rules={[{ required: true, message: '请选择出生市' }]}
            >
              <Select
                placeholder="出生市"
                showSearch
                optionFilterProp="label"
                disabled={!birthProvinceCode}
                options={birthCities.map((city) => ({ value: city.city_code, label: city.city_name }))}
              />
            </Form.Item>
            <Form.Item
              label="出生镇"
              name="birth_town_code"
              rules={[{ required: true, message: '请选择出生镇' }]}
            >
              <Select
                placeholder="出生镇"
                showSearch
                optionFilterProp="label"
                disabled={!birthCityCode}
                options={birthTowns.map((town) => ({ value: town.town_code, label: town.town_name }))}
              />
            </Form.Item>
          </div>
        )}

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', columnGap: 16 }}>
          <Form.Item label="居住省">
            <Input readOnly value={citizen.province_name ?? ''} />
          </Form.Item>
          <Form.Item label="居住市" name="city_code" rules={[{ required: true, message: '请选择居住市' }]}>
            <Select
              placeholder="居住市"
              showSearch
              optionFilterProp="label"
              options={residenceCities.map((city) => ({ value: city.city_code, label: city.city_name }))}
            />
          </Form.Item>
          <Form.Item label="居住镇" name="town_code" rules={[{ required: true, message: '请选择居住镇' }]}>
            <Select
              placeholder="居住镇"
              showSearch
              optionFilterProp="label"
              disabled={!residenceCityCode}
              options={residenceTowns.map((town) => ({ value: town.town_code, label: town.town_name }))}
            />
          </Form.Item>
        </div>

        <Form.Item label="选举资格" name="voting_eligible" valuePropName="checked">
          <Switch checkedChildren="有" unCheckedChildren="无" disabled={age !== null && age < 16} />
        </Form.Item>
      </Form>
    </Modal>
  );
}
