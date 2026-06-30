@echo off
chcp 65001 >nul

echo ================================================================
echo   MoneyRobert Pro - 停止本地开发服务
echo ================================================================
echo.

echo [STOP] 停止后端服务...

:: 通过端口 8001 查找并杀死后端进程树
echo        检查端口 8001...
for /f "tokens=5" %%a in ('netstat -ano ^| findstr ":8001" ^| findstr "LISTENING"') do (
    echo        杀死端口 8001 的进程 PID: %%a
    taskkill /F /T /PID %%a
)

:: 杀死后端相关进程（兜底）
echo        杀死 moneyrobert.exe...
taskkill /F /T /IM moneyrobert.exe 2>nul && echo        [OK] 已杀死 moneyrobert.exe || echo        [INFO] 无 moneyrobert.exe 进程

echo        杀死 cargo.exe...
taskkill /F /T /IM cargo.exe 2>nul && echo        [OK] 已杀死 cargo.exe || echo        [INFO] 无 cargo.exe 进程

echo        杀死 rustc.exe...
taskkill /F /T /IM rustc.exe 2>nul && echo        [OK] 已杀死 rustc.exe || echo        [INFO] 无 rustc.exe 进程

echo.
echo [STOP] 停止前端服务...

:: 通过端口 3000 查找并杀死前端进程树
echo        检查端口 3000...
for /f "tokens=5" %%a in ('netstat -ano ^| findstr ":3000" ^| findstr "LISTENING"') do (
    echo        杀死端口 3000 的进程 PID: %%a
    taskkill /F /T /PID %%a
)

echo.
echo [STOP] 验证端口状态...

:: 检查端口 8001
netstat -ano | findstr ":8001" | findstr "LISTENING" >nul 2>&1
if errorlevel 1 (
    echo        [OK] 端口 8001 已释放
) else (
    echo        [WARN] 端口 8001 仍被占用
    netstat -ano | findstr ":8001" | findstr "LISTENING"
)

:: 检查端口 3000
netstat -ano | findstr ":3000" | findstr "LISTENING" >nul 2>&1
if errorlevel 1 (
    echo        [OK] 端口 3000 已释放
) else (
    echo        [WARN] 端口 3000 仍被占用
    netstat -ano | findstr ":3000" | findstr "LISTENING"
)

echo.
echo ================================================================
echo   所有服务已停止
echo.
echo   如果端口仍被占用，请手动执行：
echo     taskkill /F /PID 进程号
echo   或重启电脑
echo ================================================================
echo.

pause
