import { expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { JudicialOperations, JudicialWorkspace } from "./Workspace";
import { Fragment, createElement } from "react";

vi.mock('@gmb/onchina-platform/institutions/InfoPanel', () => ({ OwnInstitutionInfoPanel: () => createElement('span', null, '司法显示') }));
vi.mock('@gmb/onchina-platform/institutions/AdminsView', () => ({ OwnInstitutionAdminsView: () => null }));

// 真实司法操作组件按后端能力清单展示。



it('无操作清单显示空态，禁用动作仍为禁用按钮', () => {
  expect(renderToStaticMarkup(<JudicialOperations auth={{} as never} />)).toContain('暂无可执行操作');
  const auth = { workspace: { workspace_sections: [{ workspace_section: 'operations', workspace_actions: [{ action: 'sign_legislation', title: '护宪终审', enabled: false }] }] } } as never;
  const html = renderToStaticMarkup(<JudicialOperations auth={auth} />);
  expect(html).toContain('护宪终审'); expect(html).toMatch(/<button[^>]*disabled/u);
});
it('后端明确开放动作才启用，其他分区动作不混入操作页', () => {
  const auth = { workspace: { workspace_sections: [{ workspace_section: 'display', workspace_actions: [{ action: 'other', title: '其他分区', enabled: true }] }, { workspace_section: 'operations', workspace_actions: [{ action: 'sign_legislation', title: '护宪终审', enabled: true }] }] } } as never;
  const html = renderToStaticMarkup(<JudicialOperations auth={auth} />);
  expect(html).toContain('护宪终审'); expect(html).not.toContain('其他分区'); expect(html).not.toMatch(/<button[^>]*disabled/u);
});

// 真实司法工作台只组合已有操作、显示和记录入口。



vi.mock('@gmb/onchina-platform/workspace/WorkspaceShell', () => ({ WorkspaceShell: ({operations, display, records}: Record<string, never>) => createElement(Fragment, null, operations, display, records) }));
vi.mock('@gmb/onchina-platform/accounts/AccountManageSection', () => ({ AccountManageSection: () => createElement('span', null, '本机构账户') }));

it('缺少后端工作台时不猜测或创建入口', () => {
  expect(renderToStaticMarkup(<JudicialWorkspace auth={{} as never} />)).toBe('');
});
it('工作台保留本机构账户、司法显示和记录并沿用禁用条件', () => {
  const auth = { workspace: { workspace_sections: [{ workspace_section: 'operations', workspace_actions: [{ action: 'sign_legislation', title: '护宪终审', enabled: false }] }] } } as never;
  const html = renderToStaticMarkup(<JudicialWorkspace auth={auth} />);
  for (const text of ['本机构账户', '司法显示', '暂无记录', '护宪终审']) expect(html).toContain(text);
  expect(html).toMatch(/<button[^>]*disabled/u);
});
