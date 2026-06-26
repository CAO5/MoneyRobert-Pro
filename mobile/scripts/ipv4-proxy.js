// IPv4 -> IPv6 反向代理 + 后端 API 转发
// 背景：Trae 内置预览服务器只监听 IPv6 [::1]:58603，
// Windows 上 localhost 默认解析到 IPv4 127.0.0.1，导致浏览器无法访问。
//
// 本脚本在 0.0.0.0:58604 上监听，按路径分流：
//   - /api/*  → 127.0.0.1:8001（Rust 后端，IPv4）用 http.request 正确转发 HTTP 协议
//   - 其他    → [::1]:58603（Trae 预览服务器，IPv6）用原始 TCP pipe（兼容 WS 升级）
//
// H5 端 API_BASE_URL 为相对路径 /api/v1，浏览器请求同源 localhost:58604/api/v1/*，
// 由本代理转发到后端 8001，无需处理 CORS（同源请求）。
// 对后端请求删除 Origin/Referer 头，避免后端 CORS 中间件误判跨域。
//
// 用法：node scripts/ipv4-proxy.js
// 停止：Ctrl+C

const http = require('http');
const net = require('net');

const LISTEN_PORT = 58604;
const PREVIEW_HOST = '::1';        // Trae 预览服务器（IPv6 回环）
const PREVIEW_PORT = 58603;
const BACKEND_HOST = '127.0.0.1';  // Rust 后端（IPv4）
const BACKEND_PORT = 8001;

// 判断是否为后端 API 请求（/api 开头）
function isApiRequest(url) {
  return url.startsWith('/api');
}

// 转发 HTTP 请求行 + headers 到上游 TCP 连接（用于 Trae 预览的原始 TCP 转发）
function pipeHttpRaw(req, upstream) {
  const reqLines = [
    `${req.method} ${req.url} HTTP/1.1`,
    ...Object.entries(req.headers).map(([k, v]) => `${k}: ${v}`),
    '', '',
  ];
  upstream.write(reqLines.join('\r\n'));
  req.pipe(upstream);
}

const server = http.createServer((req, res) => {
  const isApi = isApiRequest(req.url);

  if (isApi) {
    // ===== 后端 API：用 http.request 正确转发 HTTP 协议 =====
    // 删除 Origin/Referer 避免触发后端 CORS 校验（同源代理）
    // 改写 Host 为后端地址
    const headers = { ...req.headers };
    delete headers.origin;
    delete headers.referer;
    headers.host = `localhost:${BACKEND_PORT}`;

    const proxyReq = http.request(
      {
        hostname: BACKEND_HOST,
        port: BACKEND_PORT,
        path: req.url,
        method: req.method,
        headers,
      },
      (proxyRes) => {
        // 原样转发后端响应 status + headers + body
        res.writeHead(proxyRes.statusCode, proxyRes.headers);
        proxyRes.pipe(res);
      }
    );

    proxyReq.on('error', (err) => {
      console.error(`[proxy] backend error: ${err.code} ${err.message}`);
      if (!res.headersSent) res.writeHead(502);
      res.end(JSON.stringify({ error: 'Backend unavailable', detail: err.message }));
    });

    console.log(`[proxy] HTTP ${req.method} ${req.url} -> backend ${BACKEND_HOST}:${BACKEND_PORT}`);
    req.pipe(proxyReq);
  } else {
    // ===== Trae 预览：原始 TCP pipe（兼容页面加载与 WebSocket 升级） =====
    const target = `preview [${PREVIEW_HOST}]:${PREVIEW_PORT}`;
    console.log(`[proxy] HTTP ${req.method} ${req.url} -> ${target}`);

    const upstream = net.connect(
      { port: PREVIEW_PORT, host: PREVIEW_HOST, family: 6 },
      () => pipeHttpRaw(req, upstream)
    );

    upstream.pipe(res);

    upstream.on('error', (err) => {
      console.error(`[proxy] preview upstream error: ${err.code} ${err.message}`);
      if (!res.headersSent) res.writeHead(502);
      res.end();
    });
    req.on('error', () => upstream.destroy());
    res.on('error', () => upstream.destroy());
  }
});

// WebSocket 升级请求：双向转发原始 TCP（两种上游都支持）
server.on('upgrade', (req, socket, head) => {
  const isApi = isApiRequest(req.url);
  const target = isApi
    ? `backend ws ${BACKEND_HOST}:${BACKEND_PORT}`
    : `preview ws [${PREVIEW_HOST}]:${PREVIEW_PORT}`;
  console.log(`[proxy] WS upgrade ${req.url} -> ${target}`);

  const upstream = isApi
    ? net.connect({ port: BACKEND_PORT, host: BACKEND_HOST, family: 4 }, onConnect)
    : net.connect({ port: PREVIEW_PORT, host: PREVIEW_HOST, family: 6 }, onConnect);

  function onConnect() {
    // WS 升级用原始 TCP 转发请求行 + headers
    const headers = Object.entries(req.headers);
    let finalHeaders = headers;
    if (isApi) {
      finalHeaders = headers.filter(([k]) => k !== 'origin' && k !== 'referer');
      finalHeaders.push(['host', `localhost:${BACKEND_PORT}`]);
    }
    const reqLines = [
      `${req.method} ${req.url} HTTP/1.1`,
      ...finalHeaders.map(([k, v]) => `${k}: ${v}`),
      '', '',
    ];
    upstream.write(reqLines.join('\r\n'));
    if (head && head.length) upstream.write(head);
    socket.pipe(upstream);
    upstream.pipe(socket);
  }

  upstream.on('error', () => socket.destroy());
  socket.on('error', () => upstream.destroy());
});

server.on('error', (err) => {
  if (err.code === 'EADDRINUSE') {
    console.error(`[proxy] 端口 ${LISTEN_PORT} 已被占用`);
  } else {
    console.error('[proxy] 服务器错误:', err.message);
  }
  process.exit(1);
});

server.listen(LISTEN_PORT, '0.0.0.0', () => {
  console.log('[proxy] 监听 0.0.0.0:' + LISTEN_PORT);
  console.log('[proxy] /api/* -> ' + BACKEND_HOST + ':' + BACKEND_PORT + ' (后端 8001, http.request)');
  console.log('[proxy] 其他   -> [' + PREVIEW_HOST + ']:' + PREVIEW_PORT + ' (Trae 预览, TCP pipe)');
  console.log('[proxy] 浏览器访问: http://localhost:' + LISTEN_PORT + '/');
  console.log('[proxy] 按 Ctrl+C 停止');
});
