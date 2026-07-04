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
      host: '0.0.0.0',
      port: 10086,
      proxy: [
        {
          context: ['/api'],
          target: process.env.DOCKER_ENV ? 'http://backend:8001' : 'http://127.0.0.1:8001',
          changeOrigin: true,
        },
      ],
    },
  },
} satisfies UserConfigExport<'webpack5'>;