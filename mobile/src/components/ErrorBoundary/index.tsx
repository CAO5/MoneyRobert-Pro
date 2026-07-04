import React, { Component } from 'react';
import { View, Text, Button } from '@tarojs/components';

interface Props {
  children: React.ReactNode;
}
interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * 全局错误边界
 * 捕获子组件树渲染错误，避免整个应用白页崩溃。
 * 生产环境显示友好提示，开发环境显示错误堆栈。
 */
class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // 输出到控制台便于调试
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      const isDev = process.env.NODE_ENV !== 'production';
      return (
        <View style={{
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          minHeight: '100vh',
          padding: '32rpx',
          background: '#f5f7fa',
          color: '#1d2129',
          boxSizing: 'border-box',
        }}>
          <Text style={{ fontSize: '36rpx', fontWeight: 600, marginBottom: '16rpx' }}>
            页面渲染出错
          </Text>
          {isDev && this.state.error && (
            <Text style={{
              fontSize: '24rpx',
              color: '#f53f3f',
              marginTop: '16rpx',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
              maxHeight: '600rpx',
              overflow: 'auto',
            }}>
              {this.state.error.message}
              {'\n\n'}
              {this.state.error.stack}
            </Text>
          )}
          <Button
            size="mini"
            type="primary"
            onClick={this.handleReset}
            style={{ marginTop: '32rpx' }}
          >
            重试
          </Button>
        </View>
      );
    }
    return this.props.children;
  }
}

export default ErrorBoundary;
