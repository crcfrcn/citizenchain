// 调用公共Rust的WASM，统一二维码播放、绿色静区、会话释放和接收进度。

import {useEffect, useLayoutEffect, useRef, useState} from 'react';
import {QRCodeSVG} from 'qrcode.react';

declare const __CITIZEN_QR_WASM__: string;
type Core = WebAssembly.Exports & {
  memory: WebAssembly.Memory;
  qr_alloc(length: number): number;
  qr_free(pointer: number, length: number): void;
  qr_open(mode: number, input: number, length: number): number;
  qr_step(handle: number, input: number, length: number, output: number, capacity: number): number;
  qr_close(handle: number): void;
};
export type QrProfile = {fragment_bytes: number; interval_ms: number; foreground: string;
  background: string; quiet_zone: number; max_version: number};
type Frame = {text: string; progress: number; animated: boolean; complete: boolean};
let loading: Promise<Core> | undefined;

/** 测试可交付同一正式WASM字节；生产只读取产品构建时内嵌的模块，不请求远端脚本。 */
export function loadQrTransport(bytes?: Uint8Array): Promise<Core> {
  return loading ??= (async () => {
    const binary = bytes ?? Uint8Array.from(atob(__CITIZEN_QR_WASM__), char => char.charCodeAt(0));
    const result = await WebAssembly.instantiate(binary as BufferSource, {env:{qr_now_ms:()=>performance.now()}});
    const core = result.instance.exports as Core;
    for (const name of ['qr_alloc', 'qr_free', 'qr_open', 'qr_step', 'qr_close']) {
      if (typeof core[name] !== 'function') throw Error('二维码传输模块不完整');
    }
    return core;
  })();
}

function borrow<T>(core: Core, value: Uint8Array, action: (pointer: number) => T): T {
  if (!value.length) return action(0);
  const pointer = core.qr_alloc(value.length);
  if (!pointer) throw Error('二维码消息长度超限');
  try { new Uint8Array(core.memory.buffer, pointer, value.length).set(value); return action(pointer); }
  finally { core.qr_free(pointer, value.length); }
}

export class QrSession {
  private constructor(private readonly core: Core, private handle: number, readonly profile: QrProfile) {}
  static async open(mode: 'encode' | 'decode', text = ''): Promise<QrSession> {
    const core = await loadQrTransport(), input = new TextEncoder().encode(text);
    const handle = borrow(core, input, pointer => core.qr_open(mode === 'encode' ? 1 : 2, pointer, input.length));
    if (!handle) throw Error('二维码会话创建失败');
    try {
      const profile = JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(QrSession.packet(core, 0, '').subarray(2))) as QrProfile;
      return new QrSession(core, handle, profile);
    } catch (error) { core.qr_close(handle); throw error; }
  }
  private static packet(core: Core, handle: number, text: string): Uint8Array {
    const input = new TextEncoder().encode(text);
    return borrow(core, input, pointer => {
      let capacity = 2048, output = core.qr_alloc(capacity);
      if (!output) throw Error('二维码输出分配失败');
      try {
        let count = core.qr_step(handle, pointer, input.length, output, capacity);
        if (count < -1) {
          core.qr_free(output, capacity); output = 0; capacity = -count;
          if (capacity > 2 * 1024 * 1024 + 2048) throw Error('二维码消息超限');
          output = core.qr_alloc(capacity);
          if (!output) throw Error('二维码输出分配失败');
          count = core.qr_step(handle, pointer, input.length, output, capacity);
        }
        if (count < 2 || count > capacity) throw Error('二维码传输失败');
        const value = new Uint8Array(core.memory.buffer, output, count).slice();
        if (value[0] === 255) throw Error(new TextDecoder().decode(value.subarray(2)) || '二维码会话已关闭');
        return value;
      } finally { if (output) core.qr_free(output, capacity); }
    });
  }
  next(raw = ''): Frame {
    if (!this.handle) throw Error('二维码会话已关闭');
    const value = QrSession.packet(this.core, this.handle, raw);
    return {text: new TextDecoder('utf-8', {fatal: true}).decode(value.subarray(2)), progress: value[1]!,
      animated: value[0] === 1 && value[1] !== 0, complete: value[0] === 2};
  }
  close(): void { const handle = this.handle; this.handle = 0; if (handle) this.core.qr_close(handle); }
}

export function createQrAssembler(onProgress?: (value: number) => void) {
  let session: Promise<QrSession> | undefined, closed = false;
  const accept = async (raw: string): Promise<string | null> => {
    if (closed) throw Error('二维码接收已关闭');
    if (raw.startsWith('GMBQR1:')) throw Error('二维码传输格式已更新');
    // 普通账户、联系人等业务码仍交给所属业务解析；所有签名播放统一使用UR。
    if (!raw.toLowerCase().startsWith('ur:')) return raw;
    const current = await (session ??= QrSession.open('decode'));
    if (closed) { current.close(); return null; }
    const value = current.next(raw); onProgress?.(value.progress);
    if (!value.complete) return null;
    current.close(); session = undefined; return value.text;
  };
  return Object.assign(accept, {close: () => { closed = true; void session?.then(value => value.close(), () => {}); session = undefined; }});
}

export function SigningQr({value, size = 320}: {value: string; size?: number}) {
  const [source, setSource] = useState<string | null>(null);
  const [frame, setFrame] = useState<Frame | null>(null), [profile, setProfile] = useState<QrProfile | null>(null);
  const [error, setError] = useState<string | null>(null), [paused, setPaused] = useState(false), [side, setSide] = useState(size);
  const player = useRef<QrSession | null>(null), pausedRef = useRef(false), history = useRef<Frame[]>([]), index = useRef(0);
  const svg = useRef<SVGSVGElement>(null), container = useRef<HTMLDivElement>(null);
  const advance = () => {
    try {
      let next: Frame;
      if (index.current + 1 < history.current.length) next = history.current[++index.current]!;
      else {
        next = player.current!.next(); history.current.push(next);
        if (history.current.length > 64) history.current.shift(); index.current = history.current.length - 1;
      }
      setFrame(next);
    } catch (failure) { setError(String(failure)); pausedRef.current = true; setPaused(true); }
  };
  useEffect(() => {
    let active = true, timer: ReturnType<typeof setInterval> | undefined, current: QrSession | undefined;
    setFrame(null); setError(null); const reduced = Boolean(window.matchMedia?.('(prefers-reduced-motion: reduce)').matches); setPaused(reduced); pausedRef.current = reduced; history.current = []; index.current = 0;
    void QrSession.open('encode', value).then(session => {
      current = session;
      if (!active) { session.close(); return; }
      player.current = session; setProfile(session.profile);
      const first = session.next(); history.current = [first]; setSource(value); setFrame(first);
      if (first.animated) timer = setInterval(() => { if (!pausedRef.current) advance(); }, session.profile.interval_ms);
    }).catch(failure => { if (active) setError(String(failure)); });
    return () => { active = false; clearInterval(timer); current?.close(); player.current = null; };
  }, [value]);
  useLayoutEffect(() => {
    const modules = svg.current?.viewBox?.baseVal?.width;
    if (!modules) return;
    const ratio = window.devicePixelRatio || 1, available = Math.min(size, container.current?.clientWidth || size);
    setSide(Math.floor(available * ratio / modules) * modules / ratio);
  }, [frame?.text, size]);
  const pause = (next: boolean) => { pausedRef.current = next; setPaused(next); };
  return <div ref={container} className="citizen-qr">
    <style>{`.citizen-qr{max-width:100%;width:320px;text-align:center}.citizen-qr-image{display:block;margin:auto;max-width:100%;height:auto;shape-rendering:crispEdges}.citizen-qr-controls{display:flex;justify-content:center;gap:8px}.citizen-qr-controls button{min-height:44px;min-width:48px;padding:8px;cursor:pointer}.citizen-qr-controls button:disabled{cursor:default;opacity:.5}`}</style>
    {error ? <p role="alert">{error}</p> : frame && profile && source === value ? <>
      <QRCodeSVG ref={svg} value={frame.text} size={side} level="M" boostLevel={false}
        minVersion={frame.animated ? profile.max_version : 1} marginSize={profile.quiet_zone}
        fgColor={profile.foreground} bgColor={profile.background} className="citizen-qr-image" />
      {frame.animated && <><p>请持续扫码，接收端会显示进度</p><div className="citizen-qr-controls">
        <button type="button" disabled={index.current === 0} onClick={() => { pause(true); setFrame(history.current[--index.current]!); }}>上一帧</button>
        <button type="button" aria-pressed={paused} onClick={() => pause(!paused)}> {paused ? '继续' : '暂停'} </button>
        <button type="button" onClick={() => { pause(true); advance(); }}>下一帧</button>
      </div></>}
    </> : <p role="status">正在准备二维码</p>}
  </div>;
}
