import {expect,it,vi} from 'vitest';
import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {RuntimeConstitutionViewer} from '../guard/RuntimeConstitutionViewer';
const mock=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mock.invoke,sanitizeError:(e:Error)=>e.message}));
it('错误不会作为宪法正文，重试后保留隔离边界',async()=>{mock.invoke.mockRejectedValueOnce(new Error('合成读取失败')).mockResolvedValueOnce({html:'<p>原正文</p>',blake2256:'公开校验标识',source:'runtime'});const view=render(<RuntimeConstitutionViewer/>);await screen.findByText('合成读取失败');expect(view.container.querySelector('iframe')).toBeNull();fireEvent.click(screen.getByRole('button',{name:'重试'}));await waitFor(()=>expect(view.container.querySelector('iframe')?.getAttribute('srcdoc')).toBe('<p>原正文</p>'));expect(view.container.querySelector('iframe')?.getAttribute('sandbox')).toBe('allow-scripts');expect(mock.invoke).toHaveBeenCalledTimes(2);});
