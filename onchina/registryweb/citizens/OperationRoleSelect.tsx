import { useEffect, useState } from 'react';
import { Button, Select, Space, Typography } from 'antd';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { fetchCitizenOperationRoles, type CitizenOperation, type CitizenOperationRole } from './api';

/** 唯一候选自动确定，多候选必须从服务端给出的岗位中明确选择。 */
export function operationRoleValue(roles: CitizenOperationRole[], selected?: string): string | undefined {
  if (roles.length === 1) return roles[0].roleCode;
  return roles.find((role) => role.roleCode === selected)?.roleCode;
}

export function useCitizenOperationRoles(
  auth: AdminAuth, cidNumber: string, operation: CitizenOperation, enabled: boolean, bindingRevision: number,
) {
  const [revision, setRevision] = useState(0);
  const key = JSON.stringify([auth.accessToken, auth.accountId, auth.institutionCidNumber, cidNumber, operation, enabled, bindingRevision, revision]);
  const [result, setResult] = useState<{ key: string; roles: CitizenOperationRole[]; error?: string }>({ key: '', roles: [] });
  const [selection, setSelection] = useState<{ key: string; value: string }>();
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    fetchCitizenOperationRoles(auth, cidNumber, operation).then((output) => {
      if (!cancelled) setResult({ key, roles: output.roles });
    }).catch(() => {
      if (!cancelled) setResult({ key, roles: [], error: '办理岗位加载失败' });
    });
    return () => { cancelled = true; };
  }, [key]);
  const current = result.key === key;
  const roles = current && enabled ? result.roles : [];
  const selected = selection?.key === key ? selection.value : undefined;
  const roleCode = operationRoleValue(roles, selected);
  return {
    roles, roleCode,
    loading: enabled && !current,
    error: current ? result.error : undefined,
    ready: enabled && current && !result.error && !!roleCode,
    onChange: (value: string) => setSelection({ key, value }),
    retry: () => setRevision((value) => value + 1),
  };
}

type Props = {
  control: ReturnType<typeof useCitizenOperationRoles>;
  label?: string;
  disabled?: boolean;
};

export function OperationRoleSelect({ control, label = '办理岗位', disabled }: Props) {
  if (control.loading) return <Typography.Text type="secondary">正在加载{label}…</Typography.Text>;
  if (control.error) return <Space><Typography.Text type="danger">{control.error}</Typography.Text><Button size="small" onClick={control.retry} disabled={disabled}>重试</Button></Space>;
  if (!control.roles.length) return <Typography.Text type="secondary">无此业务办理权限</Typography.Text>;
  if (control.roles.length === 1) return <Typography.Text>{label}：{control.roles[0].roleName}</Typography.Text>;
  return <Select
    aria-label={label}
    placeholder={`请选择${label}`}
    value={control.roleCode}
    onChange={control.onChange}
    disabled={disabled}
    style={{ minWidth: 220 }}
    options={control.roles.map((role) => ({ value: role.roleCode, label: role.roleName }))}
  />;
}
