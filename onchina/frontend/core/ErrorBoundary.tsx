import React from 'react';

type Props = {
  children: React.ReactNode;
};

type State = {
  hasError: boolean;
  message: string;
};

export default class ErrorBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, message: '' };
  }

  static getDerivedStateFromError(error: unknown): State {
    const message = error instanceof Error ? error.message : 'Unknown error';
    return { hasError: true, message };
  }

  componentDidCatch(error: unknown) {
    // Keep output minimal to avoid leaking sensitive runtime details.
    console.error('UI render failure', error);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div style={{ padding: 24, fontFamily: 'sans-serif' }}>
          <h2>页面渲染失败</h2>
          <p>请刷新页面并重新登录。</p>
          <p style={{ color: '#666' }}>{this.state.message}</p>
        </div>
      );
    }
    return this.props.children;
  }
}
