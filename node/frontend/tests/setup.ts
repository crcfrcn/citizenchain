// 初始化前端测试环境和共享浏览器替身。

import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

afterEach(() => { cleanup(); });
