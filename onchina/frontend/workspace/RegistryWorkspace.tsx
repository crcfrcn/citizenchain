// 注册局工作台挂载层。私权目录是注册局登记与管理入口，不是私权机构自己的工作台。

import { useEffect, useState } from 'react';
import type { AdminAuth } from '../auth/types';
import type { CapabilitySet } from '../auth/AuthContext';
import type { CidMetaResult } from '../china/api';
import { loadCachedCidMeta } from '../china/metaCache';
import { notice } from '../utils/notice';
import { GovView } from '../gov/GovView';
import { PrivateShell } from '../private/PrivateShell';
import { EducationView } from '../education/EducationView';
import { OwnInstitutionAdminsView, RegistryAdminsView } from '../admins/RegistryAdminsView';
import { isSubordinateRegistry, isTier1Registry } from '../platform/registryTier';
import { CitizensView } from '../citizens/CitizensView';
import { AddressManageView } from '../address/AddressManageView';
import { LegislationView } from '../legislation/operator/LegislationView';
import type { PrivateType } from '../subjects/api';

type ActiveView =
  | 'citizens'
  | 'gov'
  | 'private-sole'
  | 'private-partnership'
  | 'private-company'
  | 'private-corporation'
  | 'private-welfare'
  | 'private-association'
  | 'education'
  | 'address'
  | 'own-admins'
  | 'city-registry'
  | 'federal-registry'
  | 'legislation';

export type RegistryWorkspaceProps = {
  auth: AdminAuth;
  capabilities: CapabilitySet;
  passkeyRegistered: boolean | null;
  cidMeta: CidMetaResult | null;
  setCidMeta: (next: CidMetaResult | null) => void;
};

function privateTypeForView(view: ActiveView): PrivateType | null {
  switch (view) {
    case 'private-sole':
      return 'SOLE';
    case 'private-partnership':
      return 'PARTNERSHIP';
    case 'private-company':
      return 'COMPANY';
    case 'private-corporation':
      return 'CORPORATION';
    case 'private-welfare':
      return 'WELFARE';
    case 'private-association':
      return 'ASSOCIATION';
    default:
      return null;
  }
}

function registryAdminListView(institutionCode: string | null | undefined): ActiveView | null {
  if (isTier1Registry(institutionCode)) return 'federal-registry';
  if (isSubordinateRegistry(institutionCode)) return 'city-registry';
  return null;
}

function firstBusinessView(capabilities: CapabilitySet): ActiveView {
  if (capabilities.canViewCitizens) return 'citizens';
  if (capabilities.canViewInstitutions) return 'gov';
  if (capabilities.canViewPrivate) return 'private-sole';
  if (capabilities.canViewEducation) return 'education';
  if (capabilities.canViewCityRegistry) return 'city-registry';
  if (capabilities.canViewFederalRegistry) return 'federal-registry';
  if (capabilities.canViewLegislation) return 'legislation';
  if (capabilities.canViewOwnAdmins) return 'own-admins';
  return 'citizens';
}

export function RegistryWorkspace({
  auth,
  capabilities,
  passkeyRegistered,
  cidMeta,
  setCidMeta,
}: RegistryWorkspaceProps) {
  const [activeView, setActiveView] = useState<ActiveView>('citizens');
  const [viewResetToken, setViewResetToken] = useState(0);
  const [hasInitializedView, setHasInitializedView] = useState(false);

  useEffect(() => {
    setHasInitializedView(false);
    setActiveView('citizens');
    setViewResetToken((v) => v + 1);
  }, [auth.account_id, auth.institution_code]);

  useEffect(() => {
    if (!auth.institution_code) return;
    if (passkeyRegistered === null) return;
    if (hasInitializedView) return;
    setHasInitializedView(true);
    const adminListTab = registryAdminListView(auth.institution_code);
    if (!passkeyRegistered) {
      setActiveView(adminListTab ?? firstBusinessView(capabilities));
      return;
    }
    setActiveView(firstBusinessView(capabilities));
  }, [auth.institution_code, capabilities, passkeyRegistered, hasInitializedView]);

  const loadCidMetaForInstitutions = async () => {
    if (cidMeta) return;
    try {
      const meta = await loadCachedCidMeta(auth);
      setCidMeta(meta);
    } catch (err) {
      notice.error(err, '');
    }
  };

  const switchView = async (view: ActiveView, options?: { loadCidMeta?: boolean }) => {
    setActiveView(view);
    setViewResetToken((v) => v + 1);
    if (options?.loadCidMeta) await loadCidMetaForInstitutions();
  };

  const routedPrivateType = privateTypeForView(activeView);
  const passkeyLockedRegistryView = passkeyRegistered === false ? registryAdminListView(auth.institution_code) : null;

  return (
    <>
      <div
        style={{
          display: 'flex',
          gap: 6,
          marginBottom: 20,
          padding: '8px 12px',
          background: 'rgba(255,255,255,0.08)',
          backdropFilter: 'blur(12px)',
          borderRadius: 14,
          border: '1px solid rgba(255,255,255,0.1)',
          width: 'fit-content',
          flexWrap: 'wrap',
        }}
      >
        {([
          { key: 'citizens' as const, title: '公民', visible: capabilities.canViewCitizens, onClick: () => switchView('citizens') },
          { key: 'private-sole' as const, title: '个体经营', visible: capabilities.canViewPrivate, onClick: () => switchView('private-sole', { loadCidMeta: true }) },
          { key: 'private-partnership' as const, title: '合伙企业', visible: capabilities.canViewPrivate, onClick: () => switchView('private-partnership', { loadCidMeta: true }) },
          { key: 'private-company' as const, title: '股权公司', visible: capabilities.canViewPrivate, onClick: () => switchView('private-company', { loadCidMeta: true }) },
          { key: 'private-corporation' as const, title: '股份公司', visible: capabilities.canViewPrivate, onClick: () => switchView('private-corporation', { loadCidMeta: true }) },
          { key: 'private-welfare' as const, title: '公益组织', visible: capabilities.canViewPrivate, onClick: () => switchView('private-welfare', { loadCidMeta: true }) },
          { key: 'private-association' as const, title: '注册协会', visible: capabilities.canViewPrivate, onClick: () => switchView('private-association', { loadCidMeta: true }) },
          {
            key: 'education' as const,
            title: '教育机构',
            visible: capabilities.canViewEducation,
            onClick: () => switchView('education', { loadCidMeta: true }),
          },
          {
            key: 'gov' as const,
            title: '公权机构',
            visible: capabilities.canViewInstitutions,
            onClick: () => switchView('gov', { loadCidMeta: true }),
          },
          { key: 'address' as const, title: '地址库', visible: capabilities.canViewInstitutions, onClick: () => switchView('address') },
          { key: 'legislation' as const, title: '立法与表决', visible: capabilities.canViewLegislation, onClick: () => switchView('legislation') },
          { key: 'own-admins' as const, title: '本机构管理员', visible: capabilities.canViewOwnAdmins, onClick: () => switchView('own-admins') },
          { key: 'city-registry' as const, title: '市注册局', visible: capabilities.canViewCityRegistry, onClick: () => switchView('city-registry') },
          { key: 'federal-registry' as const, title: '联邦注册局', visible: capabilities.canViewFederalRegistry, onClick: () => switchView('federal-registry') },
        ] as const)
          .filter((tab) => tab.visible && (!passkeyLockedRegistryView || tab.key === passkeyLockedRegistryView))
          .map((tab) => (
            <button
              key={tab.key}
              onClick={tab.onClick}
              style={{
                padding: '8px 20px',
                borderRadius: 10,
                border: 'none',
                cursor: 'pointer',
                fontSize: 14,
                fontWeight: 500,
                transition: 'all 0.2s ease',
                ...(activeView === tab.key
                  ? {
                      background: 'linear-gradient(135deg, #0d9488, #0f766e)',
                      color: '#fff',
                      boxShadow: '0 2px 8px rgba(13,148,136,0.35)',
                    }
                  : { background: 'transparent', color: 'rgba(255,255,255,0.7)' }),
              }}
            >
              {tab.title}
            </button>
          ))}
      </div>

      {activeView === 'gov' && capabilities.canManageInstitutions ? (
        <GovView key={`gov-${viewResetToken}`} auth={auth} cidMeta={cidMeta} resetToken={viewResetToken} />
      ) : routedPrivateType && capabilities.canViewPrivate ? (
        <PrivateShell
          key={`${activeView}-${viewResetToken}`}
          auth={auth}
          cidMeta={cidMeta}
          privateType={routedPrivateType}
        />
      ) : activeView === 'education' && capabilities.canViewEducation ? (
        <EducationView key={`education-${viewResetToken}`} auth={auth} cidMeta={cidMeta} />
      ) : activeView === 'address' && capabilities.canViewInstitutions ? (
        <AddressManageView key={`address-${viewResetToken}`} auth={auth} />
      ) : activeView === 'own-admins' && capabilities.canViewOwnAdmins ? (
        <OwnInstitutionAdminsView key={`own-admins-${viewResetToken}`} />
      ) : activeView === 'city-registry' && capabilities.canViewCityRegistry ? (
        <RegistryAdminsView key={`city-registry-${viewResetToken}`} mode="city-registry" />
      ) : activeView === 'federal-registry' && capabilities.canViewFederalRegistry ? (
        <RegistryAdminsView key={`federal-registry-${viewResetToken}`} mode="federal-registry" />
      ) : activeView === 'legislation' && capabilities.canViewLegislation ? (
        <LegislationView key={`legislation-${viewResetToken}`} auth={auth} />
      ) : (
        <CitizensView />
      )}
    </>
  );
}
