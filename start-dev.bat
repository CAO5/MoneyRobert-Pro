@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

echo ================================================================
echo   MoneyRobert Pro - 本地开发环境一键启动脚本
echo ================================================================
echo.

:: 设置项目根目录
set "PROJECT_ROOT=%~dp0"
cd /d "%PROJECT_ROOT%"

:: ---- 环境检查 ----

:: 检查 Rust/Cargo
echo [CHECK] 检查 Rust 环境...
cargo --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust/Cargo 未安装，请先安装 Rust
    echo         下载地址: https://rustup.rs
    pause
    exit /b 1
)
echo [OK] Rust 环境正常

:: 检查 Node.js
echo [CHECK] 检查 Node.js 环境...
node --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Node.js 未安装，请先安装 Node.js
    echo         下载地址: https://nodejs.org
    pause
    exit /b 1
)
echo [OK] Node.js 环境正常

:: 检查 PostgreSQL 服务
echo [CHECK] 检查 PostgreSQL 服务...
set "PG_RUNNING=0"
for %%V in (17 16 15 14) do (
    sc query "postgresql-x64-%%V" 2>nul | find "RUNNING" >nul && set "PG_RUNNING=1"
)
sc query postgresql 2>nul | find "RUNNING" >nul && set "PG_RUNNING=1"
if "!PG_RUNNING!"=="1" (
    echo [OK] PostgreSQL 服务正在运行
) else (
    echo [WARN] PostgreSQL 服务未运行，请手动启动 PostgreSQL
    echo        可尝试: net start postgresql-x64-15
)

:: 检查 Redis 服务
echo [CHECK] 检查 Redis 服务...
redis-cli ping 2>nul | find "PONG" >nul
if errorlevel 1 (
    echo [WARN] Redis 未运行，请手动启动 Redis
) else (
    echo [OK] Redis 服务正在运行
)

:: 检查环境变量文件
if not exist "backend\.env" (
    echo [INFO] 未找到 backend\.env，从 .env.example 复制...
    if exist "backend\.env.example" (
        copy "backend\.env.example" "backend\.env" >nul
        echo [OK] 已创建 backend\.env
    ) else (
        echo [WARN] 未找到 backend\.env.example
    )
)

:: 检查前端依赖
echo [CHECK] 检查前端依赖...
if not exist "frontend\node_modules" (
    echo [INFO] 安装前端依赖（可能需要几分钟）...
    pushd "%PROJECT_ROOT%frontend"
    call npm install
    popd
    if errorlevel 1 (
        echo [ERROR] 前端依赖安装失败
        pause
        exit /b 1
    )
    echo [OK] 前端依赖安装完成
) else (
    echo [OK] 前端依赖已存在
)

echo.
echo ================================================================
echo   启动服务...
echo ================================================================
echo.
echo   后端服务: http://localhost:8001
echo   前端服务: http://localhost:3000
echo.
echo   关闭窗口或运行 stop-dev.bat 可停止服务
echo ================================================================
echo.

:: 启动后端服务（在新窗口）
echo [START] 启动后端服务...
start "MoneyRobert-Backend" cmd /k "pushd "%PROJECT_ROOT%backend" && cargo run"

:: 等待后端编译/启动
echo [WAIT] 等待后端启动中（首次编译可能需要几分钟）...
timeout /t 10 /nobreak >nul

:: 启动前端服务（在新窗口）
echo [START] 启动前端服务...
start "MoneyRobert-Frontend" cmd /k "pushd "%PROJECT_ROOT%frontend" && npm run dev"

:: 等待前端启动
timeout /t 3 /nobreak >nul

echo.
echo [OK] 服务启动完成！
echo.
echo   后端 API:  http://localhost:8001
echo   前端页面:  http://localhost:3000
echo.
echo   停止方式: 运行 stop-dev.bat 或关闭新打开的命令行窗口
echo.

pause
