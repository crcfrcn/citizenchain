// 司法显示页组合原有公共面板，组件顺序、样式和行为保持不变。
import { OwnInstitutionAdminsView } from '../../platform/frontend/admins_RegistryAdminsView';
import { OwnInstitutionInfoPanel, type OwnInstitutionInfoProps } from '../../platform/frontend/institutions_OwnInstitutionInfoPanel';

export function JudicialDisplay({ auth }: OwnInstitutionInfoProps) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <OwnInstitutionInfoPanel auth={auth} />
      <OwnInstitutionAdminsView layout="cards" />
    </div>
  );
}
