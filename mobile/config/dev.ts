import type { UserConfigExport } from '@tarojs/cli';
export default {
  logger: {
    quiet: false,
    stats: true,
  },
  mini: {},
  h5: {
    devServer: {
      open: false,
      proxy: [
        {
          context: ['/api'],
          target: 'http://127.0.0.1:8001',
          changeOrigin: true,
        },
      ],
    },
  },
} satisfies UserConfigExport<'webpack5'>;