// 编辑和展示CID输入，协调缩写选区与原始完整标识。

import { forwardRef, useState } from 'react';
import { Input, type InputProps, type InputRef } from 'antd';
import { formatIdentityCidText } from '../codes/identityCid';

/** 原输入控件继续接收/提交/复制完整值；非编辑时用短号覆盖显示。 */
export const IdentityCidInput = forwardRef<InputRef, InputProps>(function IdentityCidInput(props, ref) {
  const [editing, setEditing] = useState(false);
  const raw = props.value == null ? '' : String(props.value);
  const display = formatIdentityCidText(raw);
  const masked = (!editing || props.readOnly || props.disabled) && display !== raw;
  return <span style={{ display: 'inline-grid', position: 'relative', width: '100%' }}
    onFocusCapture={() => setEditing(true)} onBlurCapture={() => setEditing(false)}>
    <Input {...props} ref={ref} style={{ ...props.style, ...(masked ? { color: 'transparent', WebkitTextFillColor: 'transparent' } : {}) }} />
    {masked && <span aria-hidden="true" style={{ position: 'absolute', inset: '0 44px 0 12px', display: 'flex', alignItems: 'center', pointerEvents: 'none', overflow: 'hidden', whiteSpace: 'nowrap', fontSize: 14 }}>{display}</span>}
  </span>;
});
