import {beforeEach,expect,it,vi} from 'vitest';
import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import type {DesktopUpdateInfo,DesktopUpdateProgress} from '../control/types';
import App from '../desktop/App';
const updates=vi.hoisted(()=>({check:vi.fn(),status:vi.fn(),download:vi.fn(),install:vi.fn()}));
vi.mock('../control/api',()=>({settingsApi:{checkDesktopUpdate:updates.check,desktopUpdateStatus:updates.status,
  downloadDesktopUpdate:updates.download,installDesktopUpdate:updates.install}}));
vi.mock('../control/index',()=>({HomeNodeSection:()=> <p>首页面板</p>,
  SettingsSection:({desktopUpdateInfo,onUpdateDesktop}:{desktopUpdateInfo:DesktopUpdateInfo;onUpdateDesktop:()=>Promise<void>})=> <>
    <p>设置面板</p><output data-testid="update-state">{JSON.stringify(desktopUpdateInfo)}</output>
    <button disabled={['downloading','installing'].includes(desktopUpdateInfo.status)} onClick={()=>void onUpdateDesktop()}>操作更新</button>
  </>,OtherTabsSection:({activeKey}:{activeKey:string})=> <p>{activeKey}</p>}));
beforeEach(()=>{
  updates.check.mockReset().mockResolvedValue(null);
  updates.status.mockReset().mockResolvedValue({status:'idle',version:null,downloaded:0,total:0,error:null});
  updates.download.mockReset();updates.install.mockReset().mockResolvedValue(undefined);
});
vi.mock('../onchain/TransactionPanel',()=>({TransactionPanel:()=> <p>交易面板</p>}));
vi.mock('../institution/NrcSection',()=>({NrcSection:()=> <p>NRC</p>}));
vi.mock('../institution/PrcSection',()=>({PrcSection:()=> <p>PRC</p>}));
vi.mock('../institution/PrbSection',()=>({PrbSection:()=> <p>PRB</p>}));
vi.mock('../offchain/section',()=>({ClearingBankSection:()=> <p>清算面板</p>}));
vi.mock('../mining/index',()=>({MiningDashboardSection:()=> <p>挖矿面板</p>}));
it('九个既有入口保持顺序：首页双面板与其余单面板切换一致',()=>{const view=render(<App/>);expect([...view.container.querySelectorAll('nav button')].map(e=>e.textContent)).toEqual(['首页','挖矿','国家储委会','省储委会','省储行','清算行','白皮书','公民宪法','设置']);expect(screen.getByText('首页面板')).toBeTruthy();expect(screen.getByText('交易面板')).toBeTruthy();for(const [tab,content]of [['挖矿','挖矿面板'],['国家储委会','NRC'],['省储委会','PRC'],['省储行','PRB'],['清算行','清算面板'],['白皮书','whitepaper'],['公民宪法','constitution'],['设置','设置面板']]){fireEvent.click(screen.getByRole('button',{name:tab}));expect(screen.getByText(content)).toBeTruthy();expect(screen.queryByText('交易面板')).toBeNull();}fireEvent.click(screen.getByRole('button',{name:'首页'}));expect(screen.getByText('交易面板')).toBeTruthy();});

it('下载进度跨Tab保留，完成后再次点击只安装而不重新下载',async()=>{
  updates.check.mockResolvedValue({currentVersion:'1.0.0',version:'1.0.1',downloadUrl:'https://github.com/package',size:100});
  let send!:(progress:DesktopUpdateProgress)=>void,finish!:()=>void;
  updates.download.mockImplementation((callback:(progress:DesktopUpdateProgress)=>void)=>{
    send=callback;return new Promise<void>(resolve=>{finish=resolve;});
  });
  const view=render(<App/>);
  fireEvent.click(screen.getByRole('button',{name:/^设置/}));
  const state=()=>JSON.parse(screen.getByTestId('update-state').textContent!);
  await waitFor(()=>expect(state().status).toBe('available'));
  fireEvent.click(screen.getByRole('button',{name:'操作更新'}));
  expect(updates.download).toHaveBeenCalledOnce();
  await act(async()=>send({status:'downloading',version:'1.0.1',downloaded:45,total:100,error:null}));
  expect(state().downloaded).toBe(45);
  fireEvent.click(screen.getByRole('button',{name:'首页'}));
  await act(async()=>{send({status:'ready',version:'1.0.1',downloaded:100,total:100,error:null});finish();});
  fireEvent.click(screen.getByRole('button',{name:/^设置/}));
  expect(state().status).toBe('ready');expect(state().downloaded).toBe(100);
  fireEvent.click(screen.getByRole('button',{name:'操作更新'}));
  await waitFor(()=>expect(updates.install).toHaveBeenCalledOnce());
  expect(updates.download).toHaveBeenCalledOnce();view.unmount();
});
it('已下载包离线恢复，安装启动失败保留可重试状态',async()=>{
  updates.status.mockResolvedValue({status:'ready',version:'1.0.1',downloaded:100,total:100,error:null});
  updates.install.mockRejectedValue(new Error('安装授权入口不可用'));
  const view=render(<App/>);fireEvent.click(screen.getByRole('button',{name:/^设置/}));
  const state=()=>JSON.parse(screen.getByTestId('update-state').textContent!);
  await waitFor(()=>expect(state().status).toBe('ready'));
  expect(updates.check).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button',{name:'操作更新'}));
  await waitFor(()=>expect(state().error).toBe('安装授权入口不可用'));
  expect(state().status).toBe('ready');expect(updates.download).not.toHaveBeenCalled();view.unmount();
});

it('缓存包缺失后允许重新下载，不反复尝试安装坏包',async()=>{
  updates.status.mockResolvedValueOnce({status:'ready',version:'1.0.1',downloaded:100,total:100,error:null})
    .mockResolvedValue({status:'error',version:'1.0.1',downloaded:100,total:100,error:'安装包已变化，请重新下载'});
  updates.install.mockRejectedValue(new Error('安装包已变化，请重新下载'));
  updates.download.mockImplementation(async(send:(progress:DesktopUpdateProgress)=>void)=>{
    send({status:'ready',version:'1.0.1',downloaded:100,total:100,error:null});
  });
  const view=render(<App/>);fireEvent.click(screen.getByRole('button',{name:/^设置/}));
  const state=()=>JSON.parse(screen.getByTestId('update-state').textContent!);
  await waitFor(()=>expect(state().status).toBe('ready'));
  fireEvent.click(screen.getByRole('button',{name:'操作更新'}));
  await waitFor(()=>expect(state().status).toBe('error'));
  fireEvent.click(screen.getByRole('button',{name:'操作更新'}));
  await waitFor(()=>expect(state().status).toBe('ready'));
  expect(updates.install).toHaveBeenCalledOnce();expect(updates.download).toHaveBeenCalledOnce();view.unmount();
});
